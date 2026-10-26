//! Original synthetic exFAT fixtures. Every byte is generated here from the public
//! Microsoft specification; no real disk or third-party recovery code is involved.
use recovery_core::{read_entry, recover_entry, scan, ScanOptions};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const PDF: &[u8] = include_bytes!("fixtures/original.pdf");
const SECTOR: usize = 512;
const FAT: usize = 24 * SECTOR;
const HEAP: usize = 128 * SECTOR;
const CLUSTERS: usize = 8192;
const BITMAP: usize = HEAP + SECTOR;
const FILE: usize = HEAP + 8 * SECTOR;
const FILESET: usize = HEAP + 64;
struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "memory-regainer-exfat-{}-{}.img",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::write(&path, bytes).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn p16(b: &mut [u8], p: usize, n: u16) {
    b[p..p + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], p: usize, n: u32) {
    b[p..p + 4].copy_from_slice(&n.to_le_bytes());
}
fn p64(b: &mut [u8], p: usize, n: u64) {
    b[p..p + 8].copy_from_slice(&n.to_le_bytes());
}
fn boot_checksum(image: &mut [u8]) {
    let mut sum = 0u32;
    for (i, b) in image[..11 * SECTOR].iter().enumerate() {
        if ![106, 107, 112].contains(&i) {
            sum = sum.rotate_right(1).wrapping_add(u32::from(*b));
        }
    }
    for p in (11 * SECTOR..12 * SECTOR).step_by(4) {
        p32(image, p, sum);
    }
    let main = image[..12 * SECTOR].to_vec();
    image[12 * SECTOR..24 * SECTOR].copy_from_slice(&main);
}
fn entry_checksum(bytes: &mut [u8], deleted: bool) {
    let mut sum = 0u16;
    for (i, b) in bytes.iter().enumerate() {
        if i == 2 || i == 3 {
            continue;
        }
        let b = if deleted && i % 32 == 0 {
            *b | 0x80
        } else {
            *b
        };
        sum = sum.rotate_right(1).wrapping_add(u16::from(b));
    }
    p16(bytes, 2, sum);
}
fn file_set(name: &str, first: u32, length: u64, deleted: bool, directory: bool) -> Vec<u8> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let names = name.len().div_ceil(15);
    let mut set = vec![0u8; (2 + names) * 32];
    set[0] = if deleted { 5 } else { 0x85 };
    set[1] = (1 + names) as u8;
    p16(&mut set, 4, if directory { 0x10 } else { 0x20 });
    // Valid timestamps for 2026-01-01 00:00:00.
    for at in [8, 12, 16] {
        p32(&mut set, at, 0x5c21_0000);
    }
    set[32] = if deleted { 0x40 } else { 0xc0 };
    set[33] = 3;
    set[35] = name.len() as u8;
    let mut hash = 0u16;
    for unit in &name {
        let upper = if (b'a' as u16..=b'z' as u16).contains(unit) {
            *unit - 32
        } else {
            *unit
        };
        for b in upper.to_le_bytes() {
            hash = hash.rotate_right(1).wrapping_add(u16::from(b));
        }
    }
    p16(&mut set, 36, hash);
    p64(&mut set, 40, length);
    p32(&mut set, 52, first);
    p64(&mut set, 56, length);
    for i in 0..names {
        set[64 + i * 32] = if deleted { 0x41 } else { 0xc1 };
        for j in 0..15 {
            if let Some(unit) = name.get(i * 15 + j) {
                p16(&mut set, 66 + i * 32 + j * 2, *unit);
            }
        }
    }
    entry_checksum(&mut set, deleted);
    set
}
fn image(nested: bool) -> Vec<u8> {
    let mut b = vec![0u8; (128 + CLUSTERS) * SECTOR];
    b[..3].copy_from_slice(&[0xeb, 0x76, 0x90]);
    b[3..11].copy_from_slice(b"EXFAT   ");
    p64(&mut b, 72, (128 + CLUSTERS) as u64);
    p32(&mut b, 80, 24);
    p32(&mut b, 84, 65);
    p32(&mut b, 88, 128);
    p32(&mut b, 92, CLUSTERS as u32);
    p32(&mut b, 96, 2);
    p32(&mut b, 100, 0x4d524558);
    p16(&mut b, 104, 0x0100);
    b[108] = 9;
    b[109] = 0;
    b[110] = 1;
    b[111] = 0x80;
    b[112] = 0xff;
    b[510..512].copy_from_slice(&[0x55, 0xaa]);
    for sector in 1..=8 {
        p32(&mut b, (sector + 1) * SECTOR - 4, 0xaa55_0000);
    }
    for (cluster, next) in [
        (0, 0xfffffff8),
        (1, u32::MAX),
        (2, u32::MAX),
        (3, 4),
        (4, u32::MAX),
        (5, u32::MAX),
        (6, u32::MAX),
    ] {
        p32(&mut b, FAT + cluster * 4, next);
    }
    // Active allocation bitmap, a valid two-cluster FAT chain.
    b[HEAP] = 0x81;
    p32(&mut b, HEAP + 20, 3);
    p64(&mut b, HEAP + 24, (CLUSTERS / 8) as u64);
    b[BITMAP] = if nested { 0b1_1111 } else { 0b1111 };
    // Original compressed up-case table: ASCII uppercase; other UTF-16 units identity.
    let mut upcase = vec![0xff, 0xff, 0x61, 0];
    for c in b'A'..=b'Z' {
        upcase.extend_from_slice(&(c as u16).to_le_bytes());
    }
    upcase.extend_from_slice(&[0xff, 0xff]);
    upcase.extend_from_slice(&(65536u32 - 0x7b).to_le_bytes()[..2]);
    let checksum = upcase.iter().fold(0u32, |sum, b| {
        sum.rotate_right(1).wrapping_add(u32::from(*b))
    });
    b[HEAP + 32] = 0x82;
    p32(&mut b, HEAP + 36, checksum);
    p32(&mut b, HEAP + 52, 5);
    p64(&mut b, HEAP + 56, upcase.len() as u64);
    b[HEAP + 3 * SECTOR..HEAP + 3 * SECTOR + upcase.len()].copy_from_slice(&upcase);
    let deleted = file_set("Удалённый документ.pdf", 10, PDF.len() as u64, true, false);
    if nested {
        let dir = file_set("Документы", 6, SECTOR as u64, false, true);
        b[FILESET..FILESET + dir.len()].copy_from_slice(&dir);
        b[HEAP + 4 * SECTOR..HEAP + 4 * SECTOR + deleted.len()].copy_from_slice(&deleted);
    } else {
        b[FILESET..FILESET + deleted.len()].copy_from_slice(&deleted);
    }
    b[FILE..FILE + PDF.len()].copy_from_slice(PDF);
    boot_checksum(&mut b);
    b
}
fn quick(b: &[u8]) -> (Fixture, recovery_core::ScanResult) {
    let f = Fixture::new(b);
    let result = scan(
        &f.0,
        &ScanOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    (f, result)
}
#[test]
fn deleted_pdf_exfat_unicode_name_exact_sha256_and_source_unchanged() {
    let bytes = image(false);
    if let Some(path) = std::env::var_os("MEMORY_REGAINER_EXFAT_FIXTURE") {
        let path = PathBuf::from(path);
        assert!(!path.exists(), "Refusing to replace an existing image");
        fs::write(path, &bytes).unwrap();
    }
    let before = Sha256::digest(&bytes);
    let (f, r) = quick(&bytes);
    assert_eq!(r.partitions[0].filesystem, "exFAT");
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.method_notes.len(), 1);
    assert!(r.method_notes[0].contains("single-FAT revision 1.00"));
    let e = &r.entries[0];
    assert_eq!(e.name, "Удалённый документ.pdf");
    assert_eq!(e.kind, "pdf");
    assert_eq!(e.method, "exfat-undelete");
    assert_eq!(e.confidence, "low");
    assert_eq!(e.offset, FILE as u64);
    let mut recovered = vec![];
    recover_entry(&f.0, e, &mut recovered, &AtomicBool::new(false)).unwrap();
    assert_eq!(recovered, PDF);
    assert_eq!(Sha256::digest(&recovered), Sha256::digest(PDF));
    assert_eq!(Sha256::digest(fs::read(&f.0).unwrap()), before);
}
#[test]
fn deleted_pdf_in_live_exfat_subdirectory_is_recovered() {
    let (f, r) = quick(&image(true));
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert!(r.entries[0].notes[0].contains("/Документы/"));
    assert_eq!(read_entry(&f.0, &r.entries[0], 100000).unwrap(), PDF);
}
#[test]
fn exfat_allocated_data_conflict_is_rejected() {
    let mut b = image(false);
    b[BITMAP + 1] |= 1;
    let (_, r) = quick(&b);
    assert!(r.entries.is_empty());
    assert!(r.warnings.iter().any(|s| s.contains("currently allocated")));
}
#[test]
fn exfat_fragmented_or_uninitialized_streams_are_skipped() {
    for uninitialized in [false, true] {
        let mut b = image(false);
        let set = &mut b[FILESET..FILESET + 128];
        if uninitialized {
            p64(set, 40, 1);
        } else {
            set[33] = 1;
        }
        entry_checksum(set, true);
        let (_, r) = quick(&b);
        assert!(r.entries.is_empty());
        assert!(r.warnings.iter().any(|s| s.contains("streams skipped")));
    }
}
#[test]
fn exfat_damaged_boot_checksum_geometry_and_dirty_state_are_rejected() {
    for case in 0..5 {
        let mut b = image(false);
        match case {
            0 => b[100] ^= 1,
            1 => {
                b[108] = 63;
                boot_checksum(&mut b);
            }
            2 => {
                p64(&mut b, 72, u64::MAX);
                boot_checksum(&mut b);
            }
            3 => p16(&mut b, 106, 2),
            _ => {
                p32(&mut b, 88, 1);
                boot_checksum(&mut b);
            }
        }
        let (_, r) = quick(&b);
        assert!(r.entries.is_empty());
        assert!(
            r.warnings.iter().any(|s| s.contains("exFAT:")),
            "{:?}",
            r.warnings
        );
    }
}
#[test]
fn exfat_missing_bitmap_truncated_chain_and_cycles_are_rejected() {
    for case in 0..4 {
        let mut b = image(false);
        match case {
            0 => b[HEAP] = 0x01,
            1 => p32(&mut b, FAT + 3 * 4, u32::MAX),
            2 => p32(&mut b, FAT + 4 * 4, 3),
            _ => b[BITMAP] &= !1,
        };
        let (_, r) = quick(&b);
        assert!(r.entries.is_empty());
        assert!(r.warnings.iter().any(|s| s.contains("exFAT:")));
    }
}
#[test]
fn exfat_invalid_entry_checksum_utf16_and_mixed_state_are_rejected() {
    for case in 0..4 {
        let mut b = image(false);
        let set = &mut b[FILESET..FILESET + 128];
        match case {
            0 => set[68] ^= 1,
            1 => {
                p16(set, 66, 0xd800);
                entry_checksum(set, true);
            }
            2 => set[64] = 0xc1,
            _ => {
                p32(set, 52, u32::MAX);
                entry_checksum(set, true);
            }
        }
        let (_, r) = quick(&b);
        assert!(r.entries.is_empty());
        assert!(r.warnings.iter().any(|s| s.contains("skipped")));
    }
}
#[test]
fn exfat_partial_range_cannot_read_directory_outside_range() {
    let f = Fixture::new(&image(false));
    let options = ScanOptions {
        mode: "advanced".into(),
        start: Some(0),
        end: Some(24 * SECTOR as u64),
        types: vec![],
    };
    let r = scan(&f.0, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r.entries.is_empty());
    assert_eq!(r.bytes_scanned, 24 * SECTOR as u64);
    assert!(r.warnings.iter().any(|s| s.contains("geometry")));
}
#[test]
fn exfat_scan_is_cancellable_between_directory_batches() {
    let f = Fixture::new(&image(false));
    let cancel = AtomicBool::new(false);
    let r = scan(&f.0, &ScanOptions::default(), &cancel, |p| {
        if p.scanned > 0 {
            cancel.store(true, Ordering::SeqCst);
        }
    });
    assert!(matches!(r, Err(recovery_core::Error::Cancelled)));
    assert!(cancel.load(Ordering::SeqCst));
    assert!(matches!(
        scan(&f.0, &ScanOptions::default(), &cancel, |_| {}),
        Err(recovery_core::Error::Cancelled)
    ));
}

#[test]
fn exfat_live_fragmented_directory_chain_is_followed() {
    let mut b = image(true);
    let dir = &mut b[FILESET..FILESET + 96];
    dir[33] = 1;
    p64(dir, 40, 1024);
    p64(dir, 56, 1024);
    entry_checksum(dir, false);
    p32(&mut b, FAT + 6 * 4, 8);
    p32(&mut b, FAT + 8 * 4, u32::MAX);
    b[BITMAP] |= 1 << 6;
    let first = HEAP + 4 * SECTOR;
    b[first..first + SECTOR].fill(0);
    for offset in (first..first + SECTOR).step_by(32) {
        b[offset] = 1;
    }
    let deleted = file_set("Удалённый документ.pdf", 10, PDF.len() as u64, true, false);
    let second = HEAP + 6 * SECTOR;
    b[second..second + deleted.len()].copy_from_slice(&deleted);
    let (f, r) = quick(&b);
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert!(r.entries[0].notes[0].contains("/Документы/"));
    assert_eq!(read_entry(&f.0, &r.entries[0], 100000).unwrap(), PDF);
}

#[test]
fn exfat_entry_set_crosses_directory_cluster_and_read_page_boundary() {
    let mut b = image(true);
    let dir_length = 129 * SECTOR;
    let dir = &mut b[FILESET..FILESET + 96];
    p64(dir, 40, dir_length as u64);
    p64(dir, 56, dir_length as u64);
    entry_checksum(dir, false);
    for cluster in 6..6 + 129 {
        let bit = cluster - 2;
        b[BITMAP + bit / 8] |= 1 << (bit % 8);
    }
    let directory = HEAP + 4 * SECTOR;
    b[directory..directory + dir_length].fill(0);
    for offset in (directory..directory + 65536 - 32).step_by(32) {
        b[offset] = 1;
    }
    let deleted = file_set("Удалённый документ.pdf", 300, PDF.len() as u64, true, false);
    let set_start = directory + 65536 - 32;
    b[set_start..set_start + deleted.len()].copy_from_slice(&deleted);
    let content = HEAP + (300 - 2) * SECTOR;
    b[content..content + PDF.len()].copy_from_slice(PDF);
    let (f, r) = quick(&b);
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert_eq!(r.entries[0].name, "Удалённый документ.pdf");
    assert_eq!(r.entries[0].offset, content as u64);
    assert_eq!(read_entry(&f.0, &r.entries[0], 100000).unwrap(), PDF);
}

fn set_allocated(image: &mut [u8], cluster: usize, allocated: bool) {
    let bit = cluster - 2;
    if allocated {
        image[BITMAP + bit / 8] |= 1 << (bit % 8);
    } else {
        image[BITMAP + bit / 8] &= !(1 << (bit % 8));
    }
}
fn live_video_image(fragmented: bool) -> (Vec<u8>, Vec<usize>) {
    let video = include_bytes!("fixtures/video/original-tail.mp4");
    let mut b = image(false);
    b[FILESET..HEAP + SECTOR].fill(0);
    let mut set = file_set("existing.mp4", 10, video.len() as u64, false, false);
    set[33] = if fragmented { 1 } else { 3 };
    entry_checksum(&mut set, false);
    b[FILESET..FILESET + set.len()].copy_from_slice(&set);
    let clusters: Vec<usize> = (0..video.len().div_ceil(SECTOR))
        .map(|n| 10 + n * if fragmented { 2 } else { 1 })
        .collect();
    for (i, cluster) in clusters.iter().copied().enumerate() {
        let begin = i * SECTOR;
        let end = (begin + SECTOR).min(video.len());
        let at = HEAP + (cluster - 2) * SECTOR;
        b[at..at + end - begin].copy_from_slice(&video[begin..end]);
        set_allocated(&mut b, cluster, true);
        if fragmented {
            p32(
                &mut b,
                FAT + cluster * 4,
                clusters.get(i + 1).copied().map_or(u32::MAX, |c| c as u32),
            );
        }
    }
    (b, clusters)
}
#[test]
fn exfat_existing_video_uses_allocated_contiguous_or_fragmented_chain_exactly() {
    let video = include_bytes!("fixtures/video/original-tail.mp4");
    for fragmented in [false, true] {
        let (b, _) = live_video_image(fragmented);
        let before = Sha256::digest(&b);
        let (f, r) = quick(&b);
        assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
        let e = &r.entries[0];
        assert_eq!(e.deletion_status, "existing");
        assert_eq!(e.relative_path.as_deref(), Some("existing.mp4"));
        assert_eq!(e.method, "exfat-metadata");
        assert_eq!(e.extents.len() > 1, fragmented);
        let mut recovered = Vec::new();
        recover_entry(&f.0, e, &mut recovered, &AtomicBool::new(false)).unwrap();
        assert_eq!(Sha256::digest(&recovered), Sha256::digest(video));
        assert_eq!(Sha256::digest(fs::read(&f.0).unwrap()), before);
    }
}
#[test]
fn exfat_existing_file_free_cluster_bad_chain_or_cycle_does_not_hide_other_records() {
    for mode in 0..3 {
        let (mut b, clusters) = live_video_image(true);
        match mode {
            0 => set_allocated(&mut b, clusters[1], false),
            1 => p32(&mut b, FAT + clusters[0] * 4, u32::MAX),
            _ => p32(&mut b, FAT + clusters[0] * 4, clusters[0] as u32),
        }
        let other = file_set("independent.pdf", 200, PDF.len() as u64, true, false);
        b[FILESET + 96..FILESET + 96 + other.len()].copy_from_slice(&other);
        let data = HEAP + 198 * SECTOR;
        b[data..data + PDF.len()].copy_from_slice(PDF);
        let (f, r) = quick(&b);
        assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
        assert_eq!(r.entries[0].name, "independent.pdf");
        assert_eq!(read_entry(&f.0, &r.entries[0], 100000).unwrap(), PDF);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("Other independent records were retained")));
    }
}
fn deleted_tree_image() -> Vec<u8> {
    let mut b = image(true);
    let dir = file_set("Документы", 6, SECTOR as u64, true, true);
    b[FILESET..FILESET + dir.len()].copy_from_slice(&dir);
    set_allocated(&mut b, 6, false);
    let first = HEAP + 4 * SECTOR;
    b[first..first + SECTOR].fill(0);
    let child = file_set("surviving.pdf", 10, PDF.len() as u64, false, false);
    b[first..first + child.len()].copy_from_slice(&child);
    let nested = file_set("Вложенные", 7, SECTOR as u64, false, true);
    b[first + child.len()..first + child.len() + nested.len()].copy_from_slice(&nested);
    let second = HEAP + 5 * SECTOR;
    b[second..second + child.len()].copy_from_slice(&child);
    b
}
#[test]
fn exfat_deleted_directory_keeps_unicode_path_and_inherits_tentative_child_deletion() {
    let b = deleted_tree_image();
    let (f, r) = quick(&b);
    assert_eq!(r.entries.len(), 2, "{:?}", r.warnings);
    let mut paths: Vec<_> = r
        .entries
        .iter()
        .map(|e| e.relative_path.as_deref().unwrap())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "Документы/surviving.pdf",
            "Документы/Вложенные/surviving.pdf"
        ]
    );
    for e in &r.entries {
        assert_eq!(e.deletion_status, "deleted");
        assert_eq!(e.confidence, "low");
        assert!(e.notes.iter().any(|n| n.contains("ancestry")));
        assert_eq!(read_entry(&f.0, e, 100000).unwrap(), PDF);
    }
    assert!(r.warnings.iter().any(|w| w.contains("Deleted directory")));
}
#[test]
fn exfat_deleted_directory_requires_free_bitmap_and_valid_entry_checksum_and_length() {
    for mode in 0..4 {
        let mut b = deleted_tree_image();
        match mode {
            0 => set_allocated(&mut b, 6, true),
            1 => b[FILESET + 2] ^= 1,
            2 => {
                let set = &mut b[FILESET..FILESET + 96];
                set[33] = 1;
                entry_checksum(set, true);
            }
            _ => {
                let set = &mut b[FILESET..FILESET + 96];
                p64(set, 40, u64::MAX);
                p64(set, 56, u64::MAX);
                entry_checksum(set, true);
            }
        }
        let (_, r) = quick(&b);
        assert!(r.entries.is_empty(), "mode {mode}: {:?}", r.entries);
        assert!(!r.warnings.is_empty());
    }
}
#[test]
fn exfat_deleted_directory_cycle_is_bounded_without_losing_valid_files() {
    let mut b = deleted_tree_image();
    let nested = HEAP + 5 * SECTOR;
    let cycle = file_set("loop", 6, SECTOR as u64, false, true);
    b[nested + 96..nested + 96 + cycle.len()].copy_from_slice(&cycle);
    let (_, r) = quick(&b);
    assert_eq!(r.entries.len(), 2);
    assert!(r.warnings.iter().any(|w| w.contains("alias or cycle")));
}
#[test]
fn exfat_entry_callback_precedes_completion_and_observes_cancellation() {
    let f = Fixture::new(&deleted_tree_image());
    let cancel = AtomicBool::new(false);
    let mut seen = Vec::new();
    let result = recovery_core::scan_with_events(
        &f.0,
        &ScanOptions::default(),
        None,
        &cancel,
        |_| {},
        |event| {
            if let recovery_core::ScanEvent::Entry(e) = event {
                seen.push(e);
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    assert!(matches!(result, Err(recovery_core::Error::Cancelled)));
    assert_eq!(seen.len(), 1);
    assert_eq!(read_entry(&f.0, &seen[0], 100000).unwrap(), PDF);
}

#[test]
fn invalid_exfat_alias_does_not_block_a_later_valid_stream_length() {
    let mut b = image(true);
    b[FILESET..HEAP + SECTOR].fill(0);
    // Live directories are queued at the front, so BAD is visited first. Its
    // second claimed cluster is free; GOOD describes the intact one-cluster dir.
    let good = file_set("GOOD", 6, SECTOR as u64, false, true);
    let bad = file_set("BAD", 6, 2 * SECTOR as u64, false, true);
    b[FILESET..FILESET + good.len()].copy_from_slice(&good);
    b[FILESET + good.len()..FILESET + good.len() + bad.len()].copy_from_slice(&bad);
    let (f, r) = quick(&b);
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert_eq!(
        r.entries[0].relative_path.as_deref(),
        Some("GOOD/Удалённый документ.pdf")
    );
    assert_eq!(read_entry(&f.0, &r.entries[0], 100000).unwrap(), PDF);
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("BAD/") && w.contains("live metadata cluster as free")));
}
