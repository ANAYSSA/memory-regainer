//! Read-only recovery primitives. No function in this crate opens a source for writing.
//! A recognisable record is evidence, never a guarantee of recoverable content.
mod carve;
mod fat;
mod partition;

use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
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

pub(crate) struct Source {
    file: File,
    pub len: u64,
    lower_bound: u64,
}
impl Source {
    fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        let len = metadata.len();
        // Raw device sizes may be supplied by native enumeration. The scanner
        // requires an explicit end in that case; read_exact still detects short reads.
        let len = if len == 0 && !metadata.is_file() {
            file.seek(SeekFrom::End(0))
                .ok()
                .filter(|n| *n > 0)
                .unwrap_or(u64::MAX)
        } else {
            len
        };
        if len == 0 {
            return Err(Error::Invalid("Source is empty".into()));
        }
        Ok(Self {
            file,
            len,
            lower_bound: 0,
        })
    }
    pub(crate) fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let end = offset
            .checked_add(buf.len() as u64)
            .ok_or_else(|| Error::Invalid("Read range overflow".into()))?;
        if offset < self.lower_bound || end > self.len {
            return Err(Error::Invalid(format!(
                "Read beyond source at byte {offset}"
            )));
        }
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buf)?;
        Ok(())
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
    mut on_progress: impl FnMut(Progress),
) -> Result<ScanResult> {
    check_cancel(cancel)?;
    if !["quick", "deep", "advanced"].contains(&options.mode.as_str()) {
        return Err(Error::Invalid("Unknown scan mode".into()));
    }
    let mut source = Source::open(path)?;
    if source.len == u64::MAX {
        source.len = options
            .end
            .filter(|n| *n > 0 && *n < u64::MAX)
            .ok_or_else(|| {
                Error::Invalid(
                    "Raw source size unavailable; select a known byte range or create an image"
                        .into(),
                )
            })?;
    }
    let start = options.start.unwrap_or(0);
    let end = options.end.unwrap_or(source.len);
    if start >= end || end > source.len {
        return Err(Error::Invalid(
            "Scan range must be inside the source and have a positive length".into(),
        ));
    }
    let mut warnings = Vec::new();
    let partial_range = start != 0 || end != source.len;
    let partitions = if partial_range {
        // Enforce the range in the read primitive too. Global partition tables
        // and filesystem structures outside the user's range must not be read.
        source.lower_bound = start;
        source.len = end;
        warnings.push("A bounded range reads only the selected bytes. Global partition tables are skipped; metadata undelete requires a complete FAT volume beginning at the selected start.".into());
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
    on_progress(Progress {
        scanned: 0,
        total,
        found: 0,
    });
    for part in &partitions {
        check_cancel(cancel)?;
        if part.offset < start || part.offset >= end {
            continue;
        }
        if part.filesystem.starts_with("FAT") {
            match fat::scan_deleted(&mut source, part, cancel, &mut |n, count| {
                scanned = scanned.saturating_add(n);
                on_progress(Progress {
                    scanned: scanned.min(total),
                    total,
                    found: entries.len() + count,
                });
            }) {
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
        let mut carved = carve::scan(
            &mut source,
            start,
            end,
            &options.types,
            cancel,
            |done, found| {
                on_progress(Progress {
                    scanned: done,
                    total,
                    found: base_count + found,
                });
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
    on_progress(Progress {
        scanned: scanned.min(total),
        total,
        found: entries.len(),
    });
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
    if max > 16 * 1024 * 1024 {
        return Err(Error::Invalid("Preview is limited to 16 MiB".into()));
    }
    let mut source = Source::open(path)?;
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
    let mut source = Source::open(path)?;
    validated_extents(&source, entry)?;
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
        }
    }
    check_cancel(cancel)?;
    Ok(written)
}

/// Formats with implemented structural boundary handlers (not an extension count).
pub fn supported_carving_types() -> &'static [&'static str] {
    &["jpg", "png", "pdf", "gif", "zip", "bmp", "wav"]
}
