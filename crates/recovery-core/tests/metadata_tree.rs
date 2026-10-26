//! Reproducible synthetic metadata trees. Payloads are original playable test
//! clips and generated photos; no physical disk or personal data is accessed.
use recovery_core::{recover_entry, scan, scan_with_events, Error, ScanEvent, ScanOptions};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const CLUSTERS: u32 = 65_526;
const FAT: u64 = 32 * 512;
const FAT_SECTORS: u32 = ((CLUSTERS + 2) * 4).div_ceil(512);
const HEAP: u64 = (32 + FAT_SECTORS) as u64 * 512;
const VIDEO: &[u8] = include_bytes!("fixtures/video/original-tail.mp4");
const PHOTO: &[u8] = include_bytes!("fixtures/original.jpg");
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn p16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn p32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn cluster(cluster: u32) -> u64 {
    HEAP + u64::from(cluster - 2) * 512
}
fn write(file: &mut File, offset: u64, bytes: &[u8]) {
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(bytes).unwrap();
}
fn fat(file: &mut File, c: u32, next: u32) {
    write(file, FAT + u64::from(c) * 4, &next.to_le_bytes());
}
fn record(short: &[u8; 11], first: u32, size: usize, directory: bool, deleted: bool) -> [u8; 32] {
    let mut r = [0u8; 32];
    r[..11].copy_from_slice(short);
    if deleted {
        r[0] = 0xe5;
    }
    r[11] = if directory { 0x10 } else { 0x20 };
    p16(&mut r, 20, (first >> 16) as u16);
    p16(&mut r, 26, first as u16);
    p32(&mut r, 28, size as u32);
    r
}
fn lfn(name: &str, short: &[u8; 11], deleted: bool) -> Vec<[u8; 32]> {
    let mut units: Vec<u16> = name.encode_utf16().collect();
    units.push(0);
    let count = units.len().div_ceil(13);
    units.resize(count * 13, 0xffff);
    let sum = short
        .iter()
        .fold(0u8, |sum, b| sum.rotate_right(1).wrapping_add(*b));
    (0..count)
        .rev()
        .map(|i| {
            let mut r = [0u8; 32];
            r[0] = if deleted {
                0xe5
            } else {
                (i + 1) as u8 | if i + 1 == count { 0x40 } else { 0 }
            };
            r[11] = 0x0f;
            r[13] = sum;
            for (j, at) in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30]
                .iter()
                .enumerate()
            {
                p16(&mut r, *at, units[i * 13 + j]);
            }
            r
        })
        .collect()
}
fn directory(file: &mut File, first: u32, parent: u32, records: &[[u8; 32]], live: bool) {
    let mut data = Vec::new();
    if first != 2 {
        data.extend_from_slice(&record(b".          ", first, 0, true, false));
        data.extend_from_slice(&record(
            b"..         ",
            if parent == 2 { 0 } else { parent },
            0,
            true,
            false,
        ));
    }
    for r in records {
        data.extend_from_slice(r);
    }
    assert!(
        data.len() < 512,
        "one-cluster directory must retain an end marker"
    );
    data.resize(512, 0);
    write(file, cluster(first), &data);
    fat(file, first, if live { 0x0fff_ffff } else { 0 });
}
struct Tree {
    fixture: Fixture,
    expected: BTreeMap<String, &'static [u8]>,
    fragmented: u32,
}
fn tree() -> Tree {
    let path = std::env::temp_dir().join(format!(
        "memory-regainer-tree-{}-{}.img",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let fixture = Fixture(path);
    let mut f = File::create(&fixture.0).unwrap();
    let total = 32 + FAT_SECTORS + CLUSTERS;
    f.set_len(u64::from(total) * 512).unwrap();
    let mut b = [0u8; 512];
    b[..3].copy_from_slice(&[0xeb, 0x58, 0x90]);
    b[3..11].copy_from_slice(b"MRTREE  ");
    p16(&mut b, 11, 512);
    b[13] = 1;
    p16(&mut b, 14, 32);
    b[16] = 1;
    b[21] = 0xf8;
    p32(&mut b, 32, total);
    p32(&mut b, 36, FAT_SECTORS);
    p32(&mut b, 44, 2);
    p16(&mut b, 48, 1);
    p16(&mut b, 50, 6);
    b[64] = 0x80;
    b[66] = 0x29;
    p32(&mut b, 67, 0x4d525432);
    b[71..82].copy_from_slice(b"NO NAME    ");
    b[82..90].copy_from_slice(b"FAT32   ");
    b[510..].copy_from_slice(&[0x55, 0xaa]);
    write(&mut f, 0, &b);
    write(&mut f, 6 * 512, &b);
    let mut fsinfo = [0u8; 512];
    p32(&mut fsinfo, 0, 0x41615252);
    p32(&mut fsinfo, 484, 0x61417272);
    let allocated = 5 + 2 * VIDEO.len().div_ceil(512) as u32 + PHOTO.len().div_ceil(512) as u32;
    p32(&mut fsinfo, 488, CLUSTERS - allocated);
    p32(&mut fsinfo, 492, 4);
    p32(&mut fsinfo, 508, 0xaa550000);
    write(&mut f, 512, &fsinfo);
    write(&mut f, 7 * 512, &fsinfo);
    fat(&mut f, 0, 0x0fff_fff8);
    fat(&mut f, 1, 0x0fff_ffff);
    let trash_name = "Удалённые видео";
    let mut root = vec![record(b"VIDEOS     ", 3, 0, true, false)];
    root.extend(lfn(trash_name, b"DELETED    ", true));
    root.push(record(b"DELETED    ", 4, 0, true, true));
    root.push(record(b"PHOTOS     ", 5, 0, true, false));
    directory(&mut f, 2, 0, &root, true);
    directory(
        &mut f,
        3,
        2,
        &[
            record(b"CAMERA1    ", 6, 0, true, false),
            record(b"CAMERA2    ", 7, 0, true, false),
        ],
        true,
    );
    let mut expected = BTreeMap::new();
    let mut next = 64;
    let fragmented = next;
    for (dir, path, count, inherited) in [
        (6, "VIDEOS/CAMERA1/".to_string(), 11, false),
        (7, "VIDEOS/CAMERA2/".to_string(), 11, false),
        (4, format!("{trash_name}/"), 10, true),
        (8, format!("{trash_name}/NESTED/"), 10, true),
    ] {
        let mut records = Vec::new();
        for i in 0..count {
            let existing = !inherited && i == 0;
            let short: [u8; 11] = format!("VID{i:05}MP4").as_bytes().try_into().unwrap();
            let first = next;
            let count = VIDEO.len().div_ceil(512) as u32;
            let stride = if existing && dir == 6 { 2 } else { 1 };
            for n in 0..count {
                let c = first + n * stride;
                let begin = n as usize * 512;
                let end = (begin + 512).min(VIDEO.len());
                write(&mut f, cluster(c), &VIDEO[begin..end]);
                if existing {
                    fat(
                        &mut f,
                        c,
                        if n + 1 == count {
                            0x0fff_ffff
                        } else {
                            c + stride
                        },
                    );
                }
            }
            next += count * stride;
            let marked_deleted = !existing && !inherited;
            records.push(record(&short, first, VIDEO.len(), false, marked_deleted));
            let name = format!("{}ID{i:05}.MP4", if marked_deleted { "_" } else { "V" });
            expected.insert(format!("{path}{name}"), VIDEO);
        }
        if dir == 4 {
            records.push(record(b"NESTED     ", 8, 0, true, false));
        }
        directory(
            &mut f,
            dir,
            if dir == 8 {
                4
            } else if dir == 6 || dir == 7 {
                3
            } else {
                2
            },
            &records,
            !inherited,
        );
    }
    let mut photos = Vec::new();
    for i in 0..2 {
        let first = next;
        let count = PHOTO.len().div_ceil(512) as u32;
        write(&mut f, cluster(first), PHOTO);
        for n in 0..count {
            if i == 0 {
                fat(
                    &mut f,
                    first + n,
                    if n + 1 == count {
                        0x0fff_ffff
                    } else {
                        first + n + 1
                    },
                );
            }
        }
        next += count;
        photos.push(record(
            if i == 0 {
                b"PHOTO001JPG"
            } else {
                b"PHOTO002JPG"
            },
            first,
            PHOTO.len(),
            false,
            i == 1,
        ));
        expected.insert(
            if i == 0 {
                "PHOTOS/PHOTO001.JPG".into()
            } else {
                "PHOTOS/_HOTO002.JPG".into()
            },
            PHOTO,
        );
    }
    directory(&mut f, 5, 2, &photos, true);
    f.sync_all().unwrap();
    Tree {
        fixture,
        expected,
        fragmented,
    }
}
fn scan_tree(t: &Tree) -> recovery_core::ScanResult {
    scan(
        &t.fixture.0,
        &ScanOptions::default(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap()
}
#[test]
fn fat32_tree_finds_42_videos_existing_fragmentation_photos_and_deleted_ancestry() {
    let t = tree();
    if let Some(path) = std::env::var_os("MEMORY_REGAINER_TREE_FIXTURE") {
        let destination = PathBuf::from(path);
        assert!(
            !destination.exists(),
            "Refusing to replace an existing image"
        );
        fs::copy(&t.fixture.0, destination).unwrap();
    }
    let before = Sha256::digest(fs::read(&t.fixture.0).unwrap());
    let result = scan_tree(&t);
    assert_eq!(result.entries.len(), 44, "{:?}", result.warnings);
    assert_eq!(
        result.entries.iter().filter(|e| e.kind == "mp4").count(),
        42
    );
    assert_eq!(
        result
            .entries
            .iter()
            .filter(|e| e.deletion_status == "existing")
            .count(),
        3
    );
    for e in &result.entries {
        let path = e.relative_path.as_ref().unwrap();
        let original = t
            .expected
            .get(path)
            .unwrap_or_else(|| panic!("unexpected path {path}"));
        let mut recovered = Vec::new();
        recover_entry(&t.fixture.0, e, &mut recovered, &AtomicBool::new(false)).unwrap();
        assert_eq!(recovered.len(), original.len());
        assert_eq!(Sha256::digest(&recovered), Sha256::digest(original));
        assert_eq!(e.partition.as_ref(), Some(&result.partitions[0].name));
        if path.starts_with("Удалённые видео/") {
            assert_eq!(e.deletion_status, "deleted");
            assert_eq!(e.confidence, "low");
            assert!(e.notes.iter().any(|n| n.contains("ancestry")));
        }
    }
    let fragmented = result
        .entries
        .iter()
        .find(|e| e.offset == cluster(t.fragmented))
        .unwrap();
    assert!(fragmented.extents.len() > 1);
    assert_eq!(fragmented.deletion_status, "existing");
    assert!(result
        .warnings
        .iter()
        .any(|w| w.contains("only its confirmed first cluster")));
    assert_eq!(Sha256::digest(fs::read(&t.fixture.0).unwrap()), before);
}
#[test]
fn deleted_directory_allocated_cluster_or_bad_dot_records_are_rejected() {
    for mode in 0..3 {
        let t = tree();
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(&t.fixture.0)
            .unwrap();
        match mode {
            0 => fat(&mut f, 4, 0x0fff_ffff),
            1 => write(&mut f, cluster(4), b"X"),
            _ => write(&mut f, cluster(4) + 32 + 26, &999u16.to_le_bytes()),
        }
        let r = scan_tree(&t);
        assert_eq!(r.entries.len(), 24, "{:?}", r.warnings);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("Deleted directory") && w.contains("skipped")));
    }
}
#[test]
fn existing_file_cycle_and_short_chain_are_skipped_without_losing_independent_files() {
    for cycle in [false, true] {
        let t = tree();
        let mut f = fs::OpenOptions::new()
            .write(true)
            .open(&t.fixture.0)
            .unwrap();
        fat(
            &mut f,
            t.fragmented,
            if cycle { t.fragmented } else { 0x0fff_ffff },
        );
        let r = scan_tree(&t);
        assert_eq!(r.entries.len(), 43, "{:?}", r.warnings);
        assert!(!r.entries.iter().any(|e| e.offset == cluster(t.fragmented)));
        assert!(r
            .warnings
            .iter()
            .any(|w| w.contains("Other independent records were retained")));
    }
}
#[test]
fn deleted_directory_only_scans_proven_first_cluster_and_breaks_parent_cycle() {
    let t = tree();
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(&t.fixture.0)
        .unwrap();
    // A surviving child directory references its deleted parent: no recursion.
    write(
        &mut f,
        cluster(8) + 12 * 32,
        &record(b"LOOP       ", 4, 0, true, false),
    );
    let r = scan_tree(&t);
    assert_eq!(r.entries.len(), 44);
    assert!(r.warnings.iter().any(|w| w.contains("alias or cycle")));
}
#[test]
fn metadata_entries_stream_before_completion_and_survive_cancellation_for_the_caller() {
    let t = tree();
    let cancel = AtomicBool::new(false);
    let mut events = Vec::new();
    let result = scan_with_events(
        &t.fixture.0,
        &ScanOptions::default(),
        None,
        &cancel,
        |_| {},
        |event| {
            if let ScanEvent::Entry(entry) = event {
                events.push(entry);
                cancel.store(true, Ordering::SeqCst);
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(events.len(), 1);
    let entry = &events[0];
    let mut output = Vec::new();
    recover_entry(&t.fixture.0, entry, &mut output, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        Sha256::digest(&output),
        Sha256::digest(t.expected[entry.relative_path.as_ref().unwrap()])
    );
}
