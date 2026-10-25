use recovery_core::{read_entry, recover_entry, scan, Entry, Error, Extent, ScanOptions};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "memory-regainer-test-{}-{}.img",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        )))
    }
    fn raw(bytes: &[u8]) -> Self {
        let f = Self::new();
        fs::write(&f.0, bytes).unwrap();
        f
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn write_at(f: &mut File, at: u64, b: &[u8]) {
    f.seek(SeekFrom::Start(at)).unwrap();
    f.write_all(b).unwrap();
}
fn digest(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn opts(mode: &str) -> ScanOptions {
    ScanOptions {
        mode: mode.into(),
        ..ScanOptions::default()
    }
}
fn scan_now(path: &Path, mode: &str) -> recovery_core::ScanResult {
    scan(path, &opts(mode), &AtomicBool::new(false), |_| {}).unwrap()
}

const JPG: &[u8] = include_bytes!("fixtures/original.jpg");
const PNG: &[u8] = include_bytes!("fixtures/original.png");
const PDF: &[u8] = include_bytes!("fixtures/original.pdf");
const GIF: &[u8] = include_bytes!("fixtures/original.gif");
const ZIP: &[u8] = include_bytes!("fixtures/original.zip");
const BMP: &[u8] = include_bytes!("fixtures/original.bmp");
const WAV: &[u8] = include_bytes!("fixtures/original.wav");

// A valid FAT volume with a deleted directory entry whose FAT chain was cleared.
// All images are sparse ordinary files. No tests access a real device.
fn fat_image(bits: u8, payload: &[u8], mbr: bool) -> (Fixture, u64, u64, u64) {
    fat_image_with_tables(bits, payload, mbr, 1)
}
fn fat_image_with_tables(
    bits: u8,
    payload: &[u8],
    mbr: bool,
    tables: u8,
) -> (Fixture, u64, u64, u64) {
    let clusters: u32 = match bits {
        12 => 1000,
        16 => 4096,
        _ => 65526,
    };
    let reserved: u32 = if bits == 32 { 32 } else { 1 };
    let roots: u32 = if bits == 32 { 0 } else { 512 };
    let fat_sectors = ((clusters + 2) * u32::from(bits)).div_ceil(8 * 512);
    let root_sectors = (roots * 32).div_ceil(512);
    let total = reserved + fat_sectors * u32::from(tables) + root_sectors + clusters;
    let base = if mbr { 1024 * 1024 } else { 0 };
    let root = base + u64::from(reserved + fat_sectors * u32::from(tables)) * 512;
    let data = root + u64::from(root_sectors) * 512;
    let first: u32 = if bits == 32 { 3 } else { 2 };
    let body = data + u64::from(first - 2) * 512;
    let fixture = Fixture::new();
    let mut file = File::create(&fixture.0).unwrap();
    file.set_len(base + u64::from(total) * 512).unwrap();
    if mbr {
        let mut m = [0u8; 512];
        m[446 + 4] = if bits == 32 { 0x0c } else { 6 };
        put32(&mut m, 446 + 8, (base / 512) as u32);
        put32(&mut m, 446 + 12, total);
        m[510..].copy_from_slice(&[0x55, 0xaa]);
        write_at(&mut file, 0, &m);
    }
    let mut b = [0u8; 512];
    b[..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    b[3..11].copy_from_slice(b"MRTEST  ");
    put16(&mut b, 11, 512);
    b[13] = 1;
    put16(&mut b, 14, reserved as u16);
    b[16] = tables;
    put16(&mut b, 17, roots as u16);
    b[21] = 0xf8;
    put32(&mut b, 32, total);
    if bits == 32 {
        put32(&mut b, 36, fat_sectors);
        put32(&mut b, 44, 2);
        put16(&mut b, 48, 1);
        b[82..90].copy_from_slice(b"FAT32   ");
    } else {
        put16(&mut b, 22, fat_sectors as u16);
        b[54..62].copy_from_slice(if bits == 12 { b"FAT12   " } else { b"FAT16   " });
    }
    b[510..].copy_from_slice(&[0x55, 0xaa]);
    write_at(&mut file, base, &b);
    let fat = base + u64::from(reserved) * 512;
    let init: &[u8] = match bits {
        12 => &[0xf8, 0xff, 0xff],
        16 => &[0xf8, 0xff, 0xff, 0xff],
        _ => &[
            0xf8, 0xff, 0xff, 0x0f, 0xff, 0xff, 0xff, 0x0f, 0xff, 0xff, 0xff, 0x0f,
        ],
    };
    for table in 0..tables {
        write_at(
            &mut file,
            fat + u64::from(table) * u64::from(fat_sectors) * 512,
            init,
        );
    }
    let dir = if bits == 32 { data } else { root };
    let mut record = [0u8; 32];
    record[..11].copy_from_slice(b"\xe5EMORY  JPG");
    record[11] = 0x20;
    put16(&mut record, 26, first as u16);
    put32(&mut record, 28, payload.len() as u32);
    write_at(&mut file, dir, &record);
    write_at(&mut file, body, payload);
    (fixture, body, dir, fat)
}

#[test]
fn deleted_jpeg_fat32_hash_matches_and_source_is_unchanged() {
    let (f, offset, _, _) = fat_image(32, JPG, false);
    let before = digest(&fs::read(&f.0).unwrap());
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.partitions[0].filesystem, "FAT32");
    assert_eq!(r.entries.len(), 1);
    let e = &r.entries[0];
    assert_eq!(e.offset, offset);
    assert_eq!(e.name, "_EMORY.JPG");
    assert_eq!(e.method, "fat-undelete");
    assert_eq!(e.confidence, "low");
    let mut recovered = Vec::new();
    let written = recover_entry(&f.0, e, &mut recovered, &AtomicBool::new(false)).unwrap();
    assert_eq!(written, JPG.len() as u64);
    assert_eq!(digest(&recovered), digest(JPG));
    assert_eq!(recovered, JPG);
    assert_eq!(before, digest(&fs::read(&f.0).unwrap()));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.method_notes.len(), 1);
    assert!(r.method_notes[0].contains("live directories"));
}

#[test]
fn fat32_invalid_root_cluster_is_reported_instead_of_empty_success() {
    for root in [0u32, 1, 65528, 0x0fff_ffff] {
        let (f, _, _, _) = fat_image(32, JPG, false);
        let mut file = File::options().write(true).open(&f.0).unwrap();
        write_at(&mut file, 44, &root.to_le_bytes());
        let r = scan_now(&f.0, "quick");
        assert!(r.entries.is_empty());
        assert!(
            r.warnings
                .iter()
                .any(|s| s.contains("FAT32 root cluster is out of range")),
            "root {root}: {:?}",
            r.warnings
        );
    }
}

#[test]
fn conflicting_fat_candidate_preserves_valid_records_before_and_after_it() {
    let (f, _, dir, fat) = fat_image_with_tables(32, JPG, false, 2);
    let fat_bytes = (dir - fat) / 2;
    let mut file = File::options().write(true).open(&f.0).unwrap();
    for (index, cluster, name, payload) in [
        (1, 10u16, b"\xe5ROKEN  PNG", PNG),
        (2, 12u16, b"\xe5ATER   PDF", PDF),
    ] {
        let mut record = [0u8; 32];
        record[..11].copy_from_slice(name);
        record[11] = 0x20;
        put16(&mut record, 26, cluster);
        put32(&mut record, 28, payload.len() as u32);
        write_at(&mut file, dir + index * 32, &record);
        write_at(&mut file, dir + u64::from(cluster - 2) * 512, payload);
    }
    write_at(
        &mut file,
        fat + fat_bytes + 10 * 4,
        &0x0fff_ffffu32.to_le_bytes(),
    );
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.entries.len(), 2, "{:?}", r.warnings);
    assert_eq!(r.entries[0].name, "_EMORY.JPG");
    assert_eq!(r.entries[1].name, "_ATER.PDF");
    assert_eq!(
        digest(&read_entry(&f.0, &r.entries[0], 100_000).unwrap()),
        digest(JPG)
    );
    assert_eq!(
        digest(&read_entry(&f.0, &r.entries[1], 100_000).unwrap()),
        digest(PDF)
    );
    assert_eq!(r.warnings.len(), 1);
    assert!(r.warnings[0].contains("Partial FAT scan"));
    assert!(r.warnings[0].contains("_ROKEN.PNG"));
    assert!(r.warnings[0].contains("Mirrored FAT tables disagree"));
}

#[test]
fn conflicting_directory_chain_preserves_already_found_records() {
    let (f, _, dir, fat) = fat_image_with_tables(32, JPG, false, 2);
    let fat_bytes = (dir - fat) / 2;
    let mut file = File::options().write(true).open(&f.0).unwrap();
    // Fill the remaining root records with volume-label records so traversal
    // reaches the FAT chain, rather than a legitimate zero end marker.
    let mut label = [b' '; 32];
    label[11] = 0x08;
    for index in 1..16 {
        write_at(&mut file, dir + index * 32, &label);
    }
    write_at(&mut file, fat + fat_bytes + 2 * 4, &8u32.to_le_bytes());
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.entries.len(), 1, "{:?}", r.warnings);
    assert_eq!(read_entry(&f.0, &r.entries[0], 100_000).unwrap(), JPG);
    assert!(r
        .warnings
        .iter()
        .any(|s| s.contains("directory / stopped at cluster 2")
            && s.contains("Mirrored FAT tables disagree")));
}

#[test]
fn metadata_read_failure_remains_fatal() {
    let (f, _, _, _) = fat_image(32, JPG, false);
    let result = scan(&f.0, &opts("quick"), &AtomicBool::new(false), |p| {
        if p.stage == "metadata" {
            // Simulate an ordinary image disappearing beneath an active reader.
            // The directory lies beyond any discovery read-ahead cache page.
            File::options()
                .write(true)
                .open(&f.0)
                .unwrap()
                .set_len(64 * 1024)
                .unwrap();
        }
    });
    assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
}

#[test]
fn metadata_cancellation_remains_fatal() {
    let (f, _, _, _) = fat_image(32, JPG, false);
    let cancel = AtomicBool::new(false);
    let result = scan(&f.0, &opts("quick"), &cancel, |p| {
        if p.stage == "metadata" {
            cancel.store(true, Ordering::SeqCst);
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
}
#[test]
fn deleted_files_fat12_and_fat16_inside_mbr() {
    for bits in [12, 16] {
        let (f, _, _, _) = fat_image(bits, JPG, true);
        let r = scan_now(&f.0, "quick");
        assert_eq!(r.entries.len(), 1);
        assert!(r.partitions[0].scheme.starts_with("MBR"));
        assert_eq!(r.partitions[0].filesystem, format!("FAT{bits}"));
        assert_eq!(read_entry(&f.0, &r.entries[0], 1_000_000).unwrap(), JPG);
    }
}
#[test]
fn all_seven_signature_boundaries_and_hashes_match() {
    let formats = [
        ("jpg", JPG),
        ("png", PNG),
        ("pdf", PDF),
        ("gif", GIF),
        ("zip", ZIP),
        ("bmp", BMP),
        ("wav", WAV),
    ];
    let mut image = vec![0; 2048];
    for (_, bytes) in &formats {
        image.extend_from_slice(bytes);
        image.extend_from_slice(&[0; 153]);
    }
    let f = Fixture::raw(&image);
    let r = scan_now(&f.0, "deep");
    assert_eq!(r.entries.len(), 7, "{:?}", r.entries);
    for (e, (kind, expected)) in r.entries.iter().zip(formats) {
        assert_eq!(e.kind, kind);
        assert_eq!(e.size, expected.len() as u64);
        let actual = read_entry(&f.0, e, 1_000_000).unwrap();
        assert_eq!(digest(&actual), digest(expected));
    }
}
#[test]
fn header_split_across_scanner_chunk_is_detected() {
    let mut image = vec![0; 1024 * 1024 - 19];
    image.extend_from_slice(PNG);
    let f = Fixture::raw(&image);
    let r = scan_now(&f.0, "deep");
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.entries[0].offset, 1024 * 1024 - 19);
}
#[test]
fn allocated_or_fragmented_fat_candidate_is_not_guessed() {
    let payload = vec![7u8; 1300];
    let (f, _, _, fat) = fat_image(32, &payload, false);
    let mut file = fs::OpenOptions::new().write(true).open(&f.0).unwrap();
    write_at(&mut file, fat + 3 * 4, &5u32.to_le_bytes());
    write_at(&mut file, fat + 5 * 4, &0x0fff_ffffu32.to_le_bytes());
    let r = scan_now(&f.0, "quick");
    assert!(r.entries.is_empty());
    assert!(r.warnings.iter().any(|s| s.contains("currently allocated")));
}
#[test]
fn deleted_zero_length_file_is_supported() {
    let (f, _, _, _) = fat_image(16, b"", false);
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.entries[0].size, 0);
    let mut out = vec![];
    assert_eq!(
        recover_entry(&f.0, &r.entries[0], &mut out, &AtomicBool::new(false)).unwrap(),
        0
    );
}
#[test]
fn corrupt_png_crc_and_short_signatures_are_rejected() {
    let mut bytes = PNG.to_vec();
    bytes[29] ^= 1;
    bytes.extend_from_slice(b"\xff\xd8\xff\0%PDF-1.4\nBAD\n%%EOF\nGIF89aBM");
    let f = Fixture::raw(&bytes);
    assert!(scan_now(&f.0, "deep").entries.is_empty());
}
#[test]
fn quick_does_not_claim_unusable_metadata_recovery() {
    for (magic, label) in [
        (b"EXFAT   ".as_slice(), "exFAT"),
        (b"NTFS    ".as_slice(), "NTFS"),
    ] {
        let mut bytes = vec![0; 8192];
        bytes[3..11].copy_from_slice(magic);
        let f = Fixture::raw(&bytes);
        let r = scan_now(&f.0, "quick");
        assert!(r.entries.is_empty());
        assert!(r.partitions[0].filesystem.starts_with(label));
        assert!(r.warnings.iter().any(|s| s.contains(if label == "exFAT" {
            "invalid boot sector"
        } else {
            "unavailable"
        })));
    }
}
#[test]
fn cancellation_before_scan_and_mid_scan_are_errors() {
    let f = Fixture::raw(&vec![0; 3 * 1024 * 1024]);
    assert!(matches!(
        scan(&f.0, &opts("deep"), &AtomicBool::new(true), |_| {}),
        Err(Error::Cancelled)
    ));
    let cancel = AtomicBool::new(false);
    let r = scan(&f.0, &opts("deep"), &cancel, |p| {
        if p.scanned > 0 {
            cancel.store(true, Ordering::SeqCst);
        }
    });
    assert!(matches!(r, Err(Error::Cancelled)));
}
#[test]
fn cancelled_recovery_never_writes_data() {
    let (f, _, _, _) = fat_image(16, JPG, false);
    let r = scan_now(&f.0, "quick");
    let mut output = vec![];
    assert!(matches!(
        recover_entry(&f.0, &r.entries[0], &mut output, &AtomicBool::new(true)),
        Err(Error::Cancelled)
    ));
    assert!(output.is_empty());
}
#[test]
fn explicit_byte_range_and_file_type_filter_are_enforced() {
    let mut image = vec![0; 4096];
    image[100..100 + PNG.len()].copy_from_slice(PNG);
    image[1000..1000 + JPG.len()].copy_from_slice(JPG);
    let f = Fixture::raw(&image);
    let mut options = opts("advanced");
    options.start = Some(900);
    options.end = Some(4096);
    options.types = vec!["jpg".into()];
    let r = scan(&f.0, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.entries[0].kind, "jpg");
    assert_eq!(r.bytes_scanned, 3196);
    options.end = Some(900);
    assert!(scan(&f.0, &options, &AtomicBool::new(false), |_| {}).is_err());
}
#[test]
fn preview_is_prefix_and_untrusted_extents_are_checked() {
    let (f, _, _, _) = fat_image(16, JPG, false);
    let mut e = scan_now(&f.0, "quick").entries.remove(0);
    assert_eq!(read_entry(&f.0, &e, 12).unwrap(), &JPG[..12]);
    e.extents[0].offset = u64::MAX;
    assert!(read_entry(&f.0, &e, 12).is_err());
    e.extents = vec![];
    assert!(read_entry(&f.0, &e, 12).is_err());
}
#[test]
fn truncated_and_random_inputs_do_not_panic() {
    let mut state = 0x12345678u32;
    for n in [1, 7, 31, 128, 511, 512, 1024, 2048, 4096, 10000] {
        let bytes: Vec<u8> = (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        let f = Fixture::raw(&bytes);
        let r = scan(&f.0, &opts("deep"), &AtomicBool::new(false), |_| {});
        assert!(r.is_ok());
    }
    for size in [1, 3, 8, 15, PNG.len() - 1] {
        let f = Fixture::raw(&PNG[..size]);
        assert!(scan_now(&f.0, "deep").entries.is_empty());
    }
}
#[test]
fn output_io_failures_propagate() {
    struct Fails;
    impl Write for Fails {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "simulated full destination",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let f = Fixture::raw(JPG);
    let e = Entry {
        id: "x".into(),
        name: "x.jpg".into(),
        kind: "jpg".into(),
        size: JPG.len() as u64,
        offset: 0,
        confidence: "unknown".into(),
        method: "test".into(),
        notes: vec![],
        extents: vec![Extent {
            offset: 0,
            length: JPG.len() as u64,
        }],
    };
    assert!(matches!(
        recover_entry(&f.0, &e, &mut Fails, &AtomicBool::new(false)),
        Err(Error::Io(_))
    ));
}
fn crc(b: &[u8]) -> u32 {
    let mut c = u32::MAX;
    for v in b {
        c ^= *v as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
fn gpt_image(sector: usize) -> Vec<u8> {
    let mut b = vec![0u8; sector * 100];
    b[510..512].copy_from_slice(&[0x55, 0xaa]);
    b[450] = 0xee;
    put32(&mut b, 454, 1);
    put32(&mut b, 458, 99);
    let t = sector * 2;
    b[t] = 1;
    put64(&mut b, t + 32, 10);
    put64(&mut b, t + 40, 80);
    b[t + 56] = b'D';
    let table_crc = crc(&b[t..t + 128]);
    let h = &mut b[sector..sector * 2];
    h[..8].copy_from_slice(b"EFI PART");
    put32(h, 8, 0x00010000);
    put32(h, 12, 92);
    put64(h, 24, 1);
    put64(h, 32, 99);
    put64(h, 40, 10);
    put64(h, 48, 98);
    put64(h, 72, 2);
    put32(h, 80, 1);
    put32(h, 84, 128);
    put32(h, 88, table_crc);
    let header_crc = crc(&h[..92]);
    put32(h, 16, header_crc);
    b
}
#[test]
fn gpt_512_and_4096_sector_tables_validate_crc() {
    for sector in [512, 4096] {
        let b = gpt_image(sector);
        let f = Fixture::raw(&b);
        let r = scan_now(&f.0, "quick");
        assert_eq!(r.partitions.len(), 1);
        assert!(r.partitions[0].scheme.starts_with("GPT"));
        assert_eq!(r.partitions[0].offset, (sector * 10) as u64);
        assert_eq!(r.partitions[0].size, (sector * 71) as u64);
    }
}
#[test]
fn damaged_gpt_is_rejected_without_inventing_partitions() {
    let mut b = gpt_image(512);
    b[512 + 24] ^= 1;
    let f = Fixture::raw(&b);
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.partitions[0].scheme, "Raw");
    assert!(r.warnings.iter().any(|s| s.contains("checksum mismatch")));
}
#[test]
fn overlapping_mbr_partitions_are_rejected() {
    let mut b = vec![0u8; 8192];
    b[510..512].copy_from_slice(&[0x55, 0xaa]);
    for p in [446, 462] {
        b[p + 4] = 6;
        put32(&mut b, p + 8, 1);
        put32(&mut b, p + 12, 8);
    }
    let f = Fixture::raw(&b);
    let r = scan_now(&f.0, "quick");
    assert_eq!(r.partitions.len(), 1);
    assert!(r.warnings.iter().any(|s| s.contains("overlaps")));
}

#[test]
fn deleted_unicode_long_name_and_duplicate_records_have_distinct_ids() {
    let (f, _, dir, _) = fat_image(32, JPG, false);
    let image = fs::read(&f.0).unwrap();
    let record = image[dir as usize..dir as usize + 32].to_vec();
    let short = b"MEMORY  JPG";
    let check = short
        .iter()
        .fold(0u8, |s, b| s.rotate_right(1).wrapping_add(*b));
    let mut lfn = [0xffu8; 32];
    lfn[0] = 0xe5;
    lfn[11] = 0x0f;
    lfn[12] = 0;
    lfn[13] = check;
    lfn[26] = 0;
    lfn[27] = 0;
    let mut units: Vec<u16> = "Память.jpg".encode_utf16().collect();
    units.push(0);
    units.resize(13, 0xffff);
    for (at, value) in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
        .into_iter()
        .zip(units)
    {
        put16(&mut lfn, at, value);
    }
    let mut file = fs::OpenOptions::new().write(true).open(&f.0).unwrap();
    write_at(&mut file, dir, &lfn);
    write_at(&mut file, dir + 32, &record);
    write_at(&mut file, dir + 64, &record);
    let result = scan_now(&f.0, "quick");
    assert_eq!(result.entries.len(), 2);
    assert_eq!(result.entries[0].name, "Память.jpg");
    assert_eq!(result.entries[1].name, "_EMORY.JPG");
    assert_ne!(result.entries[0].id, result.entries[1].id);
    assert_eq!(read_entry(&f.0, &result.entries[0], 10000).unwrap(), JPG);
}

#[test]
fn mutated_structural_inputs_preserve_extent_bounds() {
    // Deterministic mutation/property smoke test; not a claim of exhaustive fuzzing.
    let mut base = vec![0u8; 512];
    for content in [PNG, JPG, PDF, GIF, BMP, WAV, ZIP] {
        base.extend_from_slice(content);
        base.extend_from_slice(&[0u8; 17]);
    }
    let mut state = 0x5a173bcdu32;
    for _ in 0..128 {
        let mut bytes = base.clone();
        for _ in 0..12 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let at = state as usize % bytes.len();
            bytes[at] ^= (state >> 24) as u8 | 1;
        }
        let f = Fixture::raw(&bytes);
        let result = scan_now(&f.0, "deep");
        for entry in result.entries {
            assert!(entry.extents.iter().all(|e| e
                .offset
                .checked_add(e.length)
                .is_some_and(|n| n <= bytes.len() as u64)));
            assert_eq!(
                entry.extents.iter().map(|e| e.length).sum::<u64>(),
                entry.size
            );
            assert_eq!(
                read_entry(&f.0, &entry, 1024 * 1024).unwrap().len() as u64,
                entry.size
            );
        }
    }
}

#[test]
fn partial_ranges_never_follow_metadata_outside_selected_bytes() {
    let (f, _, _, _) = fat_image(32, JPG, false);
    let mut options = opts("advanced");
    options.end = Some(512);
    let r = scan(&f.0, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r.entries.is_empty());
    assert_eq!(r.partitions[0].scheme, "Selected range");
    assert_eq!(r.partitions[0].size, 512);
    assert_eq!(r.partitions[0].filesystem, "Unknown");
    assert_eq!(r.bytes_scanned, 512);
    options.start = Some(4096);
    options.end = Some(8192);
    let r = scan(&f.0, &options, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(r.entries.is_empty());
    assert_eq!(r.partitions[0].offset, 4096);
}

#[test]
fn stages_distinguish_metadata_work_from_measured_carving() {
    let f = Fixture::raw(&vec![0; 2 * 1024 * 1024]);
    for mode in ["quick", "deep"] {
        let mut events = Vec::new();
        scan(&f.0, &opts(mode), &AtomicBool::new(false), |p| {
            events.push(p)
        })
        .unwrap();
        let stages: Vec<&str> = events.iter().map(|p| p.stage.as_str()).collect();
        assert_eq!(stages.first(), Some(&"opening-source"));
        assert!(stages.contains(&"partitions"));
        assert!(stages.contains(&"metadata"));
        assert_eq!(stages.last(), Some(&"finished"));
        assert_eq!(stages.contains(&"carving"), mode == "deep");
        assert!(events
            .windows(2)
            .all(|w| w[0].bytes_read <= w[1].bytes_read));
        for p in events
            .iter()
            .filter(|p| ["opening-source", "partitions", "metadata"].contains(&p.stage.as_str()))
        {
            assert!(p.indeterminate);
            assert_eq!(p.total, 0);
        }
        for p in events.iter().filter(|p| p.stage == "carving") {
            assert!(!p.indeterminate);
            assert_eq!(p.total, 2 * 1024 * 1024);
            assert!(p.scanned <= p.total);
        }
        assert!(events.last().unwrap().bytes_read > 0);
    }
}

#[test]
fn opening_stage_is_delivered_before_source_error() {
    let f = Fixture::new();
    let mut events = Vec::new();
    assert!(
        scan(&f.0, &opts("deep"), &AtomicBool::new(false), |p| events
            .push(p))
        .is_err()
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].stage, "opening-source");
    assert!(events[0].indeterminate);
}

#[test]
fn large_recovery_reports_written_chunks_and_cancels_before_next_write() {
    let payload = vec![7u8; 3 * 1024 * 1024];
    let f = Fixture::raw(&payload);
    let e = Entry {
        id: "progress".into(),
        name: "large.bin".into(),
        kind: "bin".into(),
        size: payload.len() as u64,
        offset: 0,
        confidence: "unknown".into(),
        method: "test".into(),
        notes: vec![],
        extents: vec![Extent {
            offset: 0,
            length: payload.len() as u64,
        }],
    };
    let cancel = AtomicBool::new(false);
    let mut output = Vec::new();
    let mut events = Vec::new();
    let result = recovery_core::recover_entry_with_progress(
        &f.0,
        &e,
        &mut output,
        &cancel,
        |written, total| {
            assert_eq!(total, payload.len() as u64);
            events.push(written);
            if written >= 1024 * 1024 {
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(events, vec![0, 1024 * 1024]);
    assert_eq!(output, payload[..1024 * 1024]);
}

#[test]
fn legacy_progress_deserialization_is_indeterminate_by_default() {
    let p: recovery_core::Progress =
        serde_json::from_str(r#"{"scanned":12,"total":100,"found":1}"#).unwrap();
    assert_eq!(p.stage, "opening-source");
    assert!(p.indeterminate);
    assert_eq!(p.bytes_read, 0);
}

#[test]
fn registered_capacity_apis_keep_image_bounds_and_range_independent() {
    let bytes = vec![7u8; 4096];
    let f = Fixture::raw(&bytes);
    let options = ScanOptions {
        mode: "advanced".into(),
        start: Some(512),
        end: Some(1024),
        types: vec![],
    };
    let result = recovery_core::scan_with_size(
        &f.0,
        &options,
        Some(u64::MAX),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(result.bytes_scanned, 512);
    assert_eq!(result.partitions[0].offset, 512);
    assert_eq!(result.partitions[0].size, 512);
    let mut entry = Entry {
        id: "hint".into(),
        name: "data.bin".into(),
        kind: "bin".into(),
        size: 100,
        offset: 2000,
        confidence: "unknown".into(),
        method: "test".into(),
        notes: vec![],
        extents: vec![Extent {
            offset: 2000,
            length: 100,
        }],
    };
    assert_eq!(
        recovery_core::read_entry_with_size(&f.0, &entry, 100, Some(1)).unwrap(),
        bytes[2000..2100]
    );
    let mut out = Vec::new();
    recovery_core::recover_entry_with_progress_and_size(
        &f.0,
        &entry,
        &mut out,
        &AtomicBool::new(false),
        Some(1),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(out, bytes[2000..2100]);
    entry.extents[0].offset = 4096;
    assert!(recovery_core::read_entry_with_size(&f.0, &entry, 100, Some(u64::MAX)).is_err());
}
