//! Shared read-only source access for scans, fingerprints, previews and imaging.
//! Device handles are not ordinary files: Windows raw paths must not be queried
//! with GetFileInformationByHandle, and reads must obey sector alignment.
use crate::{Error, Result};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const PAGE: usize = 64 * 1024;
const CACHE_PAGES: usize = 8;
struct CachePage {
    start: u64,
    storage: Vec<u8>,
    head: usize,
    len: usize,
    used: u64,
}
impl CachePage {
    fn empty(alignment: usize) -> Self {
        let storage = vec![0u8; PAGE + alignment - 1];
        let head = (alignment - storage.as_ptr() as usize % alignment) % alignment;
        Self {
            start: 0,
            storage,
            head,
            len: 0,
            used: 0,
        }
    }
    fn bytes(&self) -> &[u8] {
        &self.storage[self.head..self.head + self.len]
    }
}

/// A read-only reader with bounded LRU caching and sector-aligned device reads.
/// `size_hint` is used for raw devices only when the OS cannot supply a length.
/// Ordinary image files always retain their real metadata length and errors.
pub struct SourceReader {
    file: File,
    len: u64,
    raw: bool,
    sector: u32,
    bounds: (u64, u64),
    cache: Vec<CachePage>,
    tick: u64,
    bytes_read: u64,
    read_operations: u64,
    current_offset: Option<u64>,
}
impl SourceReader {
    pub fn open(path: &Path, size_hint: Option<u64>) -> Result<Self> {
        let raw = raw_path(path);
        let file = open_read_only(path)?;
        let (len, sector) = if raw {
            device_geometry(&file, size_hint)?
        } else {
            let metadata = file
                .metadata()
                .map_err(|e| io_error("inspect image metadata", None, 0, e))?;
            if !metadata.is_file() {
                return Err(Error::Invalid(
                    "Source is not a regular image or a recognized raw device".into(),
                ));
            }
            (metadata.len(), 1)
        };
        if len == 0 {
            return Err(Error::Invalid("Source is empty".into()));
        }
        if raw
            && (!sector.is_power_of_two()
                || !(512..=65536).contains(&sector)
                || !len.is_multiple_of(u64::from(sector)))
        {
            return Err(Error::Invalid("Raw source length or logical sector size is invalid; create an image using a platform-native tool".into()));
        }
        Ok(Self {
            file,
            len,
            raw,
            sector,
            bounds: (0, len),
            cache: Vec::new(),
            tick: 0,
            bytes_read: 0,
            read_operations: 0,
            current_offset: None,
        })
    }
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn is_raw(&self) -> bool {
        self.raw
    }
    pub fn sector_size(&self) -> u32 {
        self.sector
    }
    /// Actual bytes returned by underlying reads, including any revisits/read-ahead.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
    pub fn read_operations(&self) -> u64 {
        self.read_operations
    }
    pub fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<()> {
        self.read_observed(offset, buffer, |_, _| Ok(()))
    }
    pub(crate) fn set_bounds(&mut self, lower: u64, upper: u64) -> Result<()> {
        if lower > upper || upper > self.len {
            return Err(Error::Invalid("Read bounds exceed source".into()));
        }
        if self.bounds != (lower, upper) {
            self.cache.clear();
            self.bounds = (lower, upper);
        }
        Ok(())
    }
    pub(crate) fn read_observed(
        &mut self,
        offset: u64,
        buffer: &mut [u8],
        mut observe: impl FnMut(u64, Option<u64>) -> Result<()>,
    ) -> Result<()> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| Error::Invalid("Read range overflow".into()))?;
        if offset < self.bounds.0 || end > self.bounds.1 {
            return Err(Error::Invalid(format!(
                "Read at byte {offset} for {} bytes exceeds source/range",
                buffer.len()
            )));
        }
        let mut copied = 0;
        while copied < buffer.len() {
            observe(self.bytes_read, self.current_offset)?;
            let position = offset + copied as u64;
            self.tick = self.tick.wrapping_add(1);
            let hit = self
                .cache
                .iter()
                .position(|page| position >= page.start && position < page.start + page.len as u64);
            let index = if let Some(i) = hit {
                i
            } else {
                let sector = u64::from(self.sector);
                let mut start = position / PAGE as u64 * PAGE as u64;
                let mut finish = start.saturating_add(PAGE as u64).min(self.len);
                if self.raw {
                    // Edge sector bytes are read only to satisfy the device's
                    // transfer constraints; parsers receive only requested bytes.
                    start = start.max(self.bounds.0 / sector * sector);
                    finish = finish
                        .min(self.bounds.1.div_ceil(sector) * sector)
                        .min(self.len);
                } else {
                    start = start.max(self.bounds.0);
                    finish = finish.min(self.bounds.1);
                }
                let count = (finish - start) as usize;
                let index = if self.cache.len() < CACHE_PAGES {
                    self.cache.push(CachePage::empty(self.sector as usize));
                    self.cache.len() - 1
                } else {
                    self.cache
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, p)| p.used)
                        .map(|(i, _)| i)
                        .expect("non-empty cache")
                };
                let page = &mut self.cache[index];
                page.len = 0;
                self.file
                    .seek(SeekFrom::Start(start))
                    .map_err(|e| io_error("seek source", Some(start), count, e))?;
                let mut read = 0;
                while read < count {
                    observe(self.bytes_read, self.current_offset)?;
                    self.read_operations = self.read_operations.saturating_add(1);
                    match self
                        .file
                        .read(&mut page.storage[page.head + read..page.head + count])
                    {
                        Ok(0) => {
                            return Err(io_error(
                                "read source",
                                Some(start + read as u64),
                                count - read,
                                std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "source ended or was disconnected during read",
                                ),
                            ))
                        }
                        Ok(n) => {
                            read += n;
                            self.bytes_read = self.bytes_read.saturating_add(n as u64);
                            self.current_offset = Some(start + read as u64);
                            if self.raw && read < count && !n.is_multiple_of(self.sector as usize) {
                                return Err(io_error("read source", Some(start + read as u64 - n as u64), count - read + n,
                                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("raw device returned an unaligned short transfer ({n} bytes)"))));
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            return Err(io_error(
                                "read source",
                                Some(start + read as u64),
                                count - read,
                                e,
                            ))
                        }
                    }
                    observe(self.bytes_read, self.current_offset)?;
                }
                page.start = start;
                page.len = count;
                index
            };
            let page = &mut self.cache[index];
            page.used = self.tick;
            let inside = (position - page.start) as usize;
            let count = (page.len - inside).min(buffer.len() - copied);
            buffer[copied..copied + count].copy_from_slice(&page.bytes()[inside..inside + count]);
            copied += count;
        }
        observe(self.bytes_read, self.current_offset)?;
        Ok(())
    }
}
fn io_error(operation: &str, offset: Option<u64>, count: usize, error: std::io::Error) -> Error {
    let at = offset
        .map(|n| format!(" at byte {n}, requested {count} bytes"))
        .unwrap_or_default();
    Error::Io(std::io::Error::new(
        error.kind(),
        format!("{operation}{at}: {error}"),
    ))
}
fn open_read_only(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0x1 | 0x2);
    }
    options
        .open(path)
        .map_err(|e| io_error("open source read-only", None, 0, e))
}
fn raw_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        windows_raw_path(&value)
    }
    #[cfg(not(windows))]
    {
        value.starts_with("/dev/")
    }
}
#[cfg(any(windows, test))]
fn windows_raw_path(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    if let Some(number) = value.strip_prefix(r"\\.\physicaldrive") {
        return !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit());
    }
    if let Some(drive) = value.strip_prefix(r"\\.\") {
        return drive.len() == 2
            && drive.as_bytes()[0].is_ascii_alphabetic()
            && drive.as_bytes()[1] == b':';
    }
    if let Some(guid) = value
        .strip_prefix(r"\\?\volume{")
        .and_then(|v| v.strip_suffix('}'))
    {
        return guid.len() == 36 && guid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    }
    false
}
#[cfg(windows)]
fn device_geometry(file: &File, size_hint: Option<u64>) -> Result<(u64, u32)> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};
    #[link(name = "kernel32")]
    extern "system" {
        fn DeviceIoControl(
            handle: *mut c_void,
            code: u32,
            input: *mut c_void,
            input_len: u32,
            output: *mut c_void,
            output_len: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }
    fn query(file: &File, code: u32, output: &mut [u8]) -> std::io::Result<usize> {
        let mut returned = 0;
        // SAFETY: the synchronous call borrows a live read-only File handle and
        // a writable buffer of exactly output_len bytes; no input/OVERLAPPED.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                code,
                std::ptr::null_mut(),
                0,
                output.as_mut_ptr().cast(),
                output.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(returned as usize)
        }
    }
    let mut length = [0u8; 8];
    let known_length = query(file, 0x0007_405c, &mut length)
        .ok()
        .filter(|n| *n >= 8)
        .map(|_| u64::from_le_bytes(length))
        .filter(|n| *n > 0 && *n <= i64::MAX as u64);
    let len=known_length.or(size_hint.filter(|n|*n>0)).ok_or_else(||Error::Invalid("Windows could not determine raw disk length; select an enumerated device with a known capacity or scan an image".into()))?;
    // IOCTL_DISK_GET_DRIVE_GEOMETRY has BytesPerSector at offset20.
    let mut geometry = [0u8; 24];
    let sector = if query(file, 0x0007_0000, &mut geometry).is_ok_and(|n| n >= 24) {
        u32::from_le_bytes(geometry[20..24].try_into().expect("sector field"))
    } else {
        // 4096 is a conservative transfer alignment for 512/4096-sector media.
        // No successful geometry query means no claim of a measured sector size.
        if !len.is_multiple_of(4096) {
            return Err(Error::Invalid(
                "Raw sector geometry unavailable; scan a disk image instead".into(),
            ));
        }
        4096
    };
    Ok((len, sector))
}
#[cfg(not(windows))]
fn device_geometry(file: &File, size_hint: Option<u64>) -> Result<(u64, u32)> {
    use std::{ffi::c_void, os::fd::AsRawFd};
    extern "C" {
        fn ioctl(fd: i32, request: usize, ...) -> i32;
    }
    let mut sector = 0u32;
    let mut length = 0u64;
    #[cfg(target_os = "macos")]
    {
        // Constants from the local macOS SDK sys/disk.h and sys/ioccom.h:
        // DKIOCGETBLOCKSIZE = _IOR('d',24,uint32_t), BLOCKCOUNT = ...25,uint64_t.
        let mut blocks = 0u64;
        // SAFETY: ioctl receives a live File descriptor and correctly sized,
        // aligned scalar output pointers; these are read-only geometry queries.
        let ok = unsafe {
            ioctl(
                file.as_raw_fd(),
                0x4004_6418,
                &mut sector as *mut u32 as *mut c_void,
            )
        } == 0;
        let count_ok = unsafe {
            ioctl(
                file.as_raw_fd(),
                0x4008_6419,
                &mut blocks as *mut u64 as *mut c_void,
            )
        } == 0;
        if ok && count_ok {
            length = blocks
                .checked_mul(u64::from(sector))
                .ok_or_else(|| Error::Invalid("Raw device length overflow".into()))?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Linux uapi/linux/fs.h: BLKSSZGET and 64-bit BLKGETSIZE64.
        unsafe {
            ioctl(
                file.as_raw_fd(),
                0x1268,
                &mut sector as *mut u32 as *mut c_void,
            );
        }
        unsafe {
            ioctl(
                file.as_raw_fd(),
                0x8008_1272,
                &mut length as *mut u64 as *mut c_void,
            );
        }
    }
    let length = if length > 0 {
        length
    } else if let Some(n) = size_hint.filter(|n| *n > 0) {
        n
    } else {
        let mut handle = file;
        handle.seek(SeekFrom::End(0)).ok().filter(|n| *n > 0).ok_or_else(|| Error::Invalid("Raw source size unavailable; supply the enumerated byte length or scan an image".into()))?
    };
    if sector == 0 {
        sector = if length.is_multiple_of(4096) {
            4096
        } else {
            512
        };
    }
    Ok((length, sector))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> (std::path::PathBuf, Vec<u8>) {
        let path = std::env::temp_dir().join(format!(
            "memory-reader-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let bytes: Vec<u8> = (0..PAGE * 3 + 19).map(|n| (n % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();
        (path, bytes)
    }
    #[test]
    fn windows_device_names_are_distinguished_from_images() {
        assert!(windows_raw_path(r"\\.\PhysicalDrive12"));
        assert!(windows_raw_path(r"\\.\E:"));
        assert!(windows_raw_path(
            r"\\?\Volume{12345678-abcd-1234-abcd-123456789012}"
        ));
        assert!(!windows_raw_path(r"C:\images\PhysicalDrive0.img"));
        assert!(!windows_raw_path(r"\\.\PhysicalDrive1\private"));
    }
    #[test]
    fn small_mirrored_metadata_reads_are_cached() {
        let (path, bytes) = fixture();
        let mut reader = SourceReader::open(&path, None).unwrap();
        for n in 0..500 {
            for base in [0, PAGE] {
                let at = base + n * 4;
                let mut out = [0; 4];
                reader.read_at(at as u64, &mut out).unwrap();
                assert_eq!(&out, &bytes[at..at + 4]);
            }
        }
        assert_eq!(reader.read_operations(), 2);
        assert_eq!(reader.bytes_read(), (2 * PAGE) as u64);
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn image_size_hint_never_expands_real_file_and_bounds_are_enforced() {
        let (path, bytes) = fixture();
        let mut reader = SourceReader::open(&path, Some(u64::MAX)).unwrap();
        assert_eq!(reader.len(), bytes.len() as u64);
        reader.set_bounds(13, 300).unwrap();
        let mut out = [0; 5];
        reader.read_at(13, &mut out).unwrap();
        assert_eq!(&out, &bytes[13..18]);
        assert_eq!(reader.bytes_read(), 287);
        assert!(reader.read_at(12, &mut out).is_err());
        assert!(reader.read_at(299, &mut out).is_err());
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn unaligned_raw_requests_use_aligned_cache_without_exposing_extra_bytes() {
        let (path, bytes) = fixture();
        let mut reader = SourceReader::open(&path, None).unwrap();
        reader.raw = true;
        reader.sector = 4096;
        reader.len = 3 * PAGE as u64;
        reader.bounds = (13, 70000);
        let mut out = vec![0; 66001];
        reader.read_at(17, &mut out).unwrap();
        assert_eq!(&out, &bytes[17..66018]);
        assert_eq!(reader.read_operations(), 2);
        assert!(reader.cache.iter().all(|p| p.start.is_multiple_of(4096)
            && p.len.is_multiple_of(4096)
            && (p.storage.as_ptr() as usize + p.head).is_multiple_of(4096)));
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn observed_reads_cancel_between_bounded_transfers() {
        let (path, _) = fixture();
        let mut reader = SourceReader::open(&path, None).unwrap();
        let mut out = vec![0; PAGE * 3];
        let result = reader.read_observed(0, &mut out, |bytes, _| {
            if bytes >= PAGE as u64 {
                Err(Error::Cancelled)
            } else {
                Ok(())
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(reader.bytes_read(), PAGE as u64);
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn raw_unaligned_short_transfer_is_an_io_error_with_offset() {
        let (path, _) = fixture();
        let mut reader = SourceReader::open(&path, None).unwrap();
        reader.raw = true;
        reader.sector = 512;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(17)
            .unwrap();
        let error = reader.read_at(0, &mut [0u8; 32]).unwrap_err();
        match error {
            Error::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof);
                assert!(e.to_string().contains("byte 0"));
                assert!(e.to_string().contains("unaligned short transfer"));
            }
            other => {
                panic!("Transfer failures must not be downgraded to metadata warnings: {other}")
            }
        }
        drop(reader);
        std::fs::remove_file(path).unwrap();
    }
}
