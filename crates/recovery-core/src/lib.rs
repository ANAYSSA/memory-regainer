//! Read-only recovery primitives. No function in this crate opens a source for writing.
//! A recognisable record is evidence, never a guarantee of recoverable content.
mod carve;
mod exfat;
mod fat;
mod partition;
mod progress;
mod reader;
pub use reader::SourceReader;

use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid source or options: {0}")]
    Invalid(String),
    #[error("Operation cancelled")]
    Cancelled,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanOptions {
    pub mode: String,
    pub start: Option<u64>,
    pub end: Option<u64>,
    #[serde(default)]
    pub types: Vec<String>,
}
impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            mode: "quick".into(),
            start: None,
            end: None,
            types: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub offset: u64,
    pub length: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub offset: u64,
    pub confidence: String,
    pub method: String,
    pub notes: Vec<String>,
    pub extents: Vec<Extent>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Progress {
    pub scanned: u64,
    pub total: u64,
    pub found: usize,
    #[serde(default = "default_stage")]
    pub stage: String,
    #[serde(default = "default_indeterminate")]
    pub indeterminate: bool,
    #[serde(default)]
    pub bytes_read: u64,
    #[serde(default)]
    pub current_offset: Option<u64>,
}
fn default_stage() -> String {
    "opening-source".into()
}
fn default_indeterminate() -> bool {
    true
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            scanned: 0,
            total: 0,
            found: 0,
            stage: default_stage(),
            indeterminate: true,
            bytes_read: 0,
            current_offset: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Partition {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub scheme: String,
    pub filesystem: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanResult {
    pub entries: Vec<Entry>,
    pub partitions: Vec<Partition>,
    pub warnings: Vec<String>,
    pub bytes_scanned: u64,
}

type ReadObserver<'a> = Box<dyn FnMut(u64, Option<u64>) -> Result<()> + 'a>;

pub(crate) struct Source<'a> {
    reader: SourceReader,
    pub len: u64,
    lower_bound: u64,
    observer: Option<ReadObserver<'a>>,
}
impl<'a> Source<'a> {
    fn open_with_size(path: &Path, size_hint: Option<u64>) -> Result<Self> {
        let reader = SourceReader::open(path, size_hint)?;
        Ok(Self {
            len: reader.len(),
            reader,
            lower_bound: 0,
            observer: None,
        })
    }
    pub(crate) fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<()> {
        self.reader.set_bounds(self.lower_bound, self.len)?;
        let observer = &mut self.observer;
        self.reader.read_observed(offset, buf, |bytes, offset| {
            if let Some(observer) = observer {
                observer(bytes, offset)?;
            }
            Ok(())
        })
    }
    pub(crate) fn bytes(&mut self, offset: u64, count: usize) -> Result<Vec<u8>> {
        if count > 1024 * 1024 {
            return Err(Error::Invalid("Parser allocation exceeds 1 MiB".into()));
        }
        let mut bytes = vec![0; count];
        self.read(offset, &mut bytes)?;
        Ok(bytes)
    }
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
pub(crate) fn le16(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
pub(crate) fn le32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().expect("bounded field"))
}
pub(crate) fn le64(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().expect("bounded field"))
}

/// Scan user-selected bytes. Quick mode examines deleted FAT records; deep/advanced
/// additionally carve structural file signatures. Scan ranges are end-exclusive.
pub fn scan(
    path: &Path,
    options: &ScanOptions,
    cancel: &AtomicBool,
    on_progress: impl FnMut(Progress),
) -> Result<ScanResult> {
    scan_with_size(path, options, None, cancel, on_progress)
}

/// Scan with an enumerated full-device capacity fallback. A selected range end
/// is not a device capacity; native geometry remains authoritative when available.
pub fn scan_with_size(
    path: &Path,
    options: &ScanOptions,
    size_hint: Option<u64>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(Progress),
) -> Result<ScanResult> {
    check_cancel(cancel)?;
    if !["quick", "deep", "advanced"].contains(&options.mode.as_str()) {
        return Err(Error::Invalid("Unknown scan mode".into()));
    }
    let reporter = RefCell::new(progress::Reporter::new(&mut on_progress));
    reporter.borrow_mut().stage("opening-source", true, 0, 0, 0);
    check_cancel(cancel)?;
    let mut source = Source::open_with_size(path, size_hint)?;
    source.observer = Some(Box::new(|bytes, offset| {
        check_cancel(cancel)?;
        reporter.borrow_mut().io(bytes, offset);
        check_cancel(cancel)
    }));
    let start = options.start.unwrap_or(0);
    let end = options.end.unwrap_or(source.len);
    if start >= end || end > source.len {
        return Err(Error::Invalid(
            "Scan range must be inside the source and have a positive length".into(),
        ));
    }
    if source.reader.is_raw() {
        let sector = u64::from(source.reader.sector_size());
        if !start.is_multiple_of(sector) || !end.is_multiple_of(sector) {
            return Err(Error::Invalid(format!("Raw scan range must start and end on {sector}-byte sector boundaries; use a disk image for arbitrary byte ranges")));
        }
    }
    reporter.borrow_mut().stage("partitions", true, 0, 0, 0);
    check_cancel(cancel)?;
    let mut warnings = Vec::new();
    let partial_range = start != 0 || end != source.len;
    let partitions = if partial_range {
        // Enforce the range in the read primitive too. Global partition tables
        // and filesystem structures outside the user's range must not be read.
        source.lower_bound = start;
        source.len = end;
        warnings.push("A bounded range reads only the selected bytes. Global partition tables are skipped; metadata undelete requires a complete FAT/exFAT volume beginning at the selected start.".into());
        let filesystem = partition::filesystem(&mut source, start, end - start)?;
        vec![Partition {
            name: "Selected byte range".into(),
            offset: start,
            size: end - start,
            scheme: "Selected range".into(),
            filesystem,
        }]
    } else {
        partition::discover(&mut source, &mut warnings, cancel)?
    };
    let mut entries = Vec::new();
    let mut scanned = 0u64;
    let total = end - start;
    reporter.borrow_mut().stage("metadata", true, 0, 0, 0);
    check_cancel(cancel)?;
    for part in &partitions {
        check_cancel(cancel)?;
        if part.offset < start || part.offset >= end {
            continue;
        }
        if part.filesystem.starts_with("FAT") || part.filesystem == "exFAT" {
            let mut report = |n, count| {
                scanned = scanned.saturating_add(n);
                reporter
                    .borrow_mut()
                    .position(scanned, entries.len() + count);
            };
            let result = if part.filesystem == "exFAT" {
                exfat::scan_deleted(&mut source, part, cancel, &mut report)
            } else {
                fat::scan_deleted(&mut source, part, cancel, &mut report)
            };
            match result {
                Ok((mut found, notices)) => {
                    entries.append(&mut found);
                    warnings.extend(notices);
                }
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(Error::Io(e)) => return Err(Error::Io(e)),
                Err(e) => warnings.push(format!("{}: {e}", part.name)),
            }
        } else if options.mode == "quick" {
            warnings.push(format!("{}: {} metadata undelete is unavailable. Deep Scan can search supported signatures.", part.name, part.filesystem));
        }
    }
    entries.retain(|e| e.offset >= start && e.offset.checked_add(e.size).is_some_and(|n| n <= end));
    if options.mode != "quick" {
        let base_count = entries.len();
        reporter
            .borrow_mut()
            .stage("carving", false, total, 0, base_count);
        check_cancel(cancel)?;
        let mut carved = carve::scan(
            &mut source,
            start,
            end,
            &options.types,
            cancel,
            |done, found| {
                reporter.borrow_mut().position(done, base_count + found);
            },
        )?;
        carved.retain(|e| {
            !entries
                .iter()
                .any(|old| old.offset == e.offset && old.size == e.size)
        });
        entries.extend(carved);
        scanned = total;
    }
    if !options.types.is_empty() {
        entries.retain(|e| {
            options
                .types
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&e.kind))
        });
    }
    check_cancel(cancel)?;
    reporter.borrow_mut().stage(
        "finished",
        false,
        if options.mode == "quick" { 0 } else { total },
        scanned.min(total),
        entries.len(),
    );
    check_cancel(cancel)?;
    warnings.sort();
    warnings.dedup();
    Ok(ScanResult {
        entries,
        partitions,
        warnings,
        bytes_scanned: scanned.min(total),
    })
}

fn validated_extents(source: &Source, entry: &Entry) -> Result<()> {
    let mut sum = 0u64;
    if entry.extents.len() > 1_000_000 {
        return Err(Error::Invalid("Too many extents".into()));
    }
    for extent in &entry.extents {
        let end = extent
            .offset
            .checked_add(extent.length)
            .ok_or_else(|| Error::Invalid("Extent overflow".into()))?;
        if end > source.len {
            return Err(Error::Invalid("Entry extent exceeds source".into()));
        }
        sum = sum
            .checked_add(extent.length)
            .ok_or_else(|| Error::Invalid("Entry length overflow".into()))?;
    }
    if sum != entry.size {
        return Err(Error::Invalid(
            "Extent lengths do not match entry size".into(),
        ));
    }
    Ok(())
}

/// Read a bounded prefix for a preview. No recovered content is executed.
pub fn read_entry(path: &Path, entry: &Entry, max: usize) -> Result<Vec<u8>> {
    read_entry_with_size(path, entry, max, None)
}

/// Preview with the same full-device capacity fallback used during scanning.
pub fn read_entry_with_size(
    path: &Path,
    entry: &Entry,
    max: usize,
    size_hint: Option<u64>,
) -> Result<Vec<u8>> {
    if max > 16 * 1024 * 1024 {
        return Err(Error::Invalid("Preview is limited to 16 MiB".into()));
    }
    let mut source = Source::open_with_size(path, size_hint)?;
    validated_extents(&source, entry)?;
    let mut out = Vec::with_capacity((entry.size.min(max as u64)) as usize);
    for extent in &entry.extents {
        let count = extent.length.min((max - out.len()) as u64) as usize;
        if count == 0 {
            break;
        }
        let p = out.len();
        out.resize(p + count, 0);
        source.read(extent.offset, &mut out[p..])?;
    }
    Ok(out)
}

/// Stream an entry to a caller-approved writer. The caller must use a safe
/// destination and an atomic temporary file, and verify its output hash.
pub fn recover_entry(
    path: &Path,
    entry: &Entry,
    writer: &mut impl Write,
    cancel: &AtomicBool,
) -> Result<u64> {
    recover_entry_with_progress(path, entry, writer, cancel, |_, _| {})
}

/// As `recover_entry`, reporting bytes actually written after each bounded chunk.
/// Cancellation is cooperative between reads/writes; an OS-blocked device read
/// must first return before its error or cancellation can be reported.
pub fn recover_entry_with_progress(
    path: &Path,
    entry: &Entry,
    writer: &mut impl Write,
    cancel: &AtomicBool,
    on_progress: impl FnMut(u64, u64),
) -> Result<u64> {
    recover_entry_with_progress_and_size(path, entry, writer, cancel, None, on_progress)
}

/// Recovery with the same registered full-device capacity fallback used by scan
/// and preview. The hint never expands an ordinary image's real metadata bounds.
pub fn recover_entry_with_progress_and_size(
    path: &Path,
    entry: &Entry,
    writer: &mut impl Write,
    cancel: &AtomicBool,
    size_hint: Option<u64>,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<u64> {
    check_cancel(cancel)?;
    let mut source = Source::open_with_size(path, size_hint)?;
    source.observer = Some(Box::new(|_, _| check_cancel(cancel)));
    validated_extents(&source, entry)?;
    on_progress(0, entry.size);
    check_cancel(cancel)?;
    let mut buf = vec![0u8; 1024 * 1024];
    let mut written = 0;
    for extent in &entry.extents {
        let mut pos = 0;
        while pos < extent.length {
            check_cancel(cancel)?;
            let count = (extent.length - pos).min(buf.len() as u64) as usize;
            source.read(extent.offset + pos, &mut buf[..count])?;
            writer.write_all(&buf[..count])?;
            written += count as u64;
            pos += count as u64;
            on_progress(written, entry.size);
        }
    }
    check_cancel(cancel)?;
    Ok(written)
}

/// Formats with implemented structural boundary handlers (not an extension count).
pub fn supported_carving_types() -> &'static [&'static str] {
    &["jpg", "png", "pdf", "gif", "zip", "bmp", "wav"]
}
