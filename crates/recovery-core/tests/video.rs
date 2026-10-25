//! Original FFmpeg-generated media exercises the public recovery API independently
//! of the parser implementation. CI only reads these checked-in bytes; no codec
//! tools, external processes, physical disks or proprietary media are required.
use recovery_core::{read_entry, recover_entry, scan, Error, Extent, ScanOptions};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::io::{Seek, SeekFrom, Write};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

const MP4_TAIL: &[u8] = include_bytes!("fixtures/video/original-tail.mp4");
const MP4_HEAD: &[u8] = include_bytes!("fixtures/video/original-faststart.mp4");
const MOV_TAIL: &[u8] = include_bytes!("fixtures/video/original-tail.mov");
const MOV_HEAD: &[u8] = include_bytes!("fixtures/video/original-faststart.mov");
const MP4_CO64: &[u8] = include_bytes!("fixtures/video/original-co64-extended.mp4");
const MP4_HEVC: &[u8] = include_bytes!("fixtures/video/original-hevc.mp4");
const ORIGINALS: &[(&str, &str, &[u8])] = &[
    ("original-tail.mp4", "mp4", MP4_TAIL),
    ("original-faststart.mp4", "mp4", MP4_HEAD),
    ("original-tail.mov", "mov", MOV_TAIL),
    ("original-faststart.mov", "mov", MOV_HEAD),
    ("original-co64-extended.mp4", "mp4", MP4_CO64),
    ("original-hevc.mp4", "mp4", MP4_HEVC),
];
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Image(PathBuf);
impl Image {
    fn new(bytes: &[u8]) -> Self {
        let p = std::env::temp_dir().join(format!(
            "memory-regainer-video-{}-{}.img",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&p, bytes).unwrap();
        Self(p)
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn options() -> ScanOptions {
    ScanOptions {
        mode: "deep".into(),
        types: vec!["mp4".into(), "mov".into()],
        ..ScanOptions::default()
    }
}
fn scan_bytes(bytes: &[u8]) -> recovery_core::ScanResult {
    let image = Image::new(bytes);
    scan(&image.0, &options(), &AtomicBool::new(false), |_| {}).unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn set32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
// Independently locate atoms by their declared boundaries, never by searching
// media payload strings. Test mutations then address public QTFF fields.
fn atom_at_path(bytes: &[u8], path: &[[u8; 4]]) -> usize {
    let mut begin = 0;
    let mut end = bytes.len();
    let mut found = 0;
    for kind in path {
        let mut at = begin;
        loop {
            assert!(at + 8 <= end, "Missing atom {kind:?}");
            let size32 = be32(bytes, at);
            let (size, header) = if size32 == 1 {
                (
                    u64::from_be_bytes(bytes[at + 8..at + 16].try_into().unwrap()) as usize,
                    16,
                )
            } else {
                (size32 as usize, 8)
            };
            assert!(size >= header && at + size <= end);
            if &bytes[at + 4..at + 8] == kind {
                found = at;
                begin = at + header;
                end = at + size;
                break;
            }
            at += size;
        }
    }
    found
}
fn table(bytes: &[u8], kind: [u8; 4]) -> usize {
    atom_at_path(
        bytes,
        &[*b"moov", *b"trak", *b"mdia", *b"minf", *b"stbl", kind],
    )
}
fn reject(bytes: &[u8], label: &str) {
    let r = scan_bytes(bytes);
    assert!(r.entries.is_empty(), "{label}: {:#?}", r.entries);
}

#[test]
fn original_video_fixture_hashes_match_recorded_generation() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/video/manifest.json")).unwrap();
    for (name, _, bytes) in ORIGINALS {
        let row = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["file"] == *name)
            .unwrap();
        assert_eq!(row["sha256"], digest(bytes));
        assert_eq!(row["bytes"], bytes.len());
        assert_eq!(row["full_decode_exit_code"], 0);
    }
}

#[test]
fn playable_mp4_mov_head_tail_and_co64_round_trip_exactly() {
    for (name, kind, bytes) in ORIGINALS {
        let prefix = 137usize;
        let mut disk = vec![0x55; prefix];
        disk.extend_from_slice(bytes);
        disk.extend_from_slice(&[0xaa; 97]);
        let image = Image::new(&disk);
        let r = scan(&image.0, &options(), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(r.entries.len(), 1, "{name}: {r:#?}");
        let entry = &r.entries[0];
        assert_eq!(entry.kind, *kind, "{name}");
        assert_eq!(entry.offset, prefix as u64, "{name}");
        assert_eq!(entry.size, bytes.len() as u64, "{name}");
        assert_eq!(
            entry.extents,
            vec![Extent {
                offset: prefix as u64,
                length: bytes.len() as u64
            }]
        );
        assert_eq!(entry.confidence, "medium");
        assert_eq!(read_entry(&image.0, entry, bytes.len()).unwrap(), *bytes);
        let mut recovered = Vec::new();
        let n = recover_entry(&image.0, entry, &mut recovered, &AtomicBool::new(false)).unwrap();
        assert_eq!(n, bytes.len() as u64);
        assert_eq!(digest(&recovered), digest(bytes), "{name}");
        assert_eq!(recovered, *bytes, "{name}");
        assert_eq!(fs::read(&image.0).unwrap(), disk, "source changed: {name}");
        // Optional local evidence export. CI does not set this variable and never
        // invokes ffmpeg. All names are the fixed original fixture names above.
        if let Some(directory) = std::env::var_os("MEMORY_REGAINER_VIDEO_EXPORT_DIR") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join(name), &recovered).unwrap();
        }
    }
}

#[test]
fn unaligned_ftyp_header_crosses_carving_buffer_boundary() {
    // This offset straddles both a usual MiB window and the scanner's overlap
    // region; it is deliberately neither sector- nor atom-aligned.
    for prefix in [1024 * 1024 - 19, 1024 * 1024 - 3] {
        let mut disk = vec![0x55; prefix];
        disk.extend_from_slice(MP4_HEAD);
        let r = scan_bytes(&disk);
        assert_eq!(r.entries.len(), 1, "prefix {prefix}");
        assert_eq!(r.entries[0].offset, prefix as u64);
        assert_eq!(r.entries[0].size, MP4_HEAD.len() as u64);
    }
}

#[test]
fn adjacent_mp4_and_mov_are_not_joined_and_filters_are_specific() {
    let disk = [MP4_TAIL, MOV_HEAD].concat();
    let image = Image::new(&disk);
    let all = scan(&image.0, &options(), &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(all.entries.len(), 2);
    assert_eq!(all.entries[0].size, MP4_TAIL.len() as u64);
    assert_eq!(all.entries[1].offset, MP4_TAIL.len() as u64);
    assert_eq!(all.entries[1].size, MOV_HEAD.len() as u64);
    for (filter, expected, offset) in [("MP4", "mp4", 0), ("mov", "mov", MP4_TAIL.len())] {
        let mut opts = options();
        opts.types = vec![filter.into()];
        let r = scan(&image.0, &opts, &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].kind, expected);
        assert_eq!(r.entries[0].offset, offset as u64);
    }
}

#[test]
fn quick_scan_does_not_claim_video_carving() {
    let image = Image::new(MP4_TAIL);
    let mut opts = options();
    opts.mode = "quick".into();
    assert!(scan(&image.0, &opts, &AtomicBool::new(false), |_| {})
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn partial_range_does_not_borrow_a_missing_header_or_tail() {
    let disk = [MP4_TAIL, MOV_HEAD].concat();
    let image = Image::new(&disk);
    let mut opts = options();
    opts.start = Some(MP4_TAIL.len() as u64);
    opts.end = Some(disk.len() as u64);
    let r = scan(&image.0, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.entries[0].kind, "mov");
    for (start, end) in [(1, MP4_TAIL.len()), (0, MP4_TAIL.len() - 1)] {
        opts.start = Some(start as u64);
        opts.end = Some(end as u64);
        assert!(scan(&image.0, &opts, &AtomicBool::new(false), |_| {})
            .unwrap()
            .entries
            .is_empty());
    }
}

#[test]
#[cfg(unix)]
fn sparse_source_above_four_gib_uses_file_relative_media_offsets() {
    let image = Image::new(&[]);
    let base = (1u64 << 32) + 131;
    let mut file = fs::OpenOptions::new().write(true).open(&image.0).unwrap();
    file.seek(SeekFrom::Start(base)).unwrap();
    file.write_all(MP4_CO64).unwrap();
    drop(file);
    let mut opts = options();
    opts.start = Some(base);
    opts.end = Some(base + MP4_CO64.len() as u64);
    let r = scan(&image.0, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.entries[0].offset, base);
    assert_eq!(
        read_entry(&image.0, &r.entries[0], MP4_CO64.len()).unwrap(),
        MP4_CO64
    );
}

#[test]
fn truncated_movie_metadata_or_media_is_rejected() {
    for (_, _, bytes) in ORIGINALS {
        reject(&bytes[..bytes.len() - 1], "one byte truncated");
        reject(&bytes[..bytes.len() / 2], "half truncated");
    }
}

#[test]
fn chunk_offsets_must_point_to_available_mdat_payload() {
    for value in [0, 4, u32::MAX - 1, MP4_TAIL.len() as u32 + 100] {
        let mut bad = MP4_TAIL.to_vec();
        let at = table(&bad, *b"stco");
        set32(&mut bad, at + 16, value);
        reject(&bad, "invalid chunk location");
    }
    let mut bad = MP4_CO64.to_vec();
    let at = table(&bad, *b"co64");
    bad[at + 16..at + 24].copy_from_slice(&u64::MAX.to_be_bytes());
    reject(&bad, "overflowed 64-bit chunk location");
}

#[test]
fn sample_sizes_cannot_escape_the_media_atom() {
    let mut bad = MP4_TAIL.to_vec();
    let at = table(&bad, *b"stsz");
    assert_eq!(be32(&bad, at + 12), 0); // FFmpeg emitted variable video sample sizes.
    set32(&mut bad, at + 20, u32::MAX);
    reject(&bad, "sample exceeds mdat");
}

#[test]
fn sample_to_chunk_table_rejects_zero_or_missing_references() {
    for (field, value) in [(16, 0), (20, 0), (24, 0), (24, 999_999), (12, u32::MAX)] {
        let mut bad = MP4_TAIL.to_vec();
        let at = table(&bad, *b"stsc");
        set32(&mut bad, at + field, value);
        reject(&bad, "invalid sample/chunk/description reference");
    }
}

#[test]
fn timing_and_size_tables_must_describe_the_same_samples() {
    let mut bad = MP4_TAIL.to_vec();
    let at = table(&bad, *b"stts");
    let count = be32(&bad, at + 16);
    set32(&mut bad, at + 16, count + 1);
    reject(&bad, "timing count mismatch");
    let mut bad = MP4_TAIL.to_vec();
    let at = table(&bad, *b"stsz");
    set32(&mut bad, at + 16, u32::MAX);
    reject(&bad, "declared sample table exceeds its atom");
}

#[test]
fn external_data_reference_is_not_a_self_contained_video() {
    let mut bad = MP4_TAIL.to_vec();
    let dref = atom_at_path(
        &bad,
        &[*b"moov", *b"trak", *b"mdia", *b"minf", *b"dinf", *b"dref"],
    );
    assert_eq!(&bad[dref + 20..dref + 24], b"url ");
    bad[dref + 27] = 0; // Clear the self-contained flag in the first url entry.
    reject(&bad, "external media reference");
}

#[test]
fn audio_only_handlers_do_not_establish_a_video_candidate() {
    let mut bad = MP4_TAIL.to_vec();
    let hdlr = atom_at_path(&bad, &[*b"moov", *b"trak", *b"mdia", *b"hdlr"]);
    assert_eq!(&bad[hdlr + 16..hdlr + 20], b"vide");
    bad[hdlr + 16..hdlr + 20].copy_from_slice(b"soun");
    reject(&bad, "no video track");
}

#[test]
fn fragmented_movie_marker_and_unbounded_media_are_rejected() {
    let moov = atom_at_path(MP4_TAIL, &[*b"moov"]);
    let mut fragmented = MP4_TAIL.to_vec();
    fragmented.splice(moov..moov, [0, 0, 0, 8, b'm', b'o', b'o', b'f']);
    reject(&fragmented, "fragmented moof layout");
    let mut zero = MP4_TAIL.to_vec();
    let mdat = atom_at_path(&zero, &[*b"mdat"]);
    set32(&mut zero, mdat, 0);
    reject(&zero, "mdat extends to unknown end");
    let mut huge = MP4_CO64.to_vec();
    let mdat = atom_at_path(&huge, &[*b"mdat"]);
    huge[mdat + 8..mdat + 16].copy_from_slice(&u64::MAX.to_be_bytes());
    reject(&huge, "extended atom size overflow");
}

#[test]
fn cancellation_is_an_error_and_cancelled_video_export_writes_nothing() {
    let image = Image::new(MP4_TAIL);
    let cancel = AtomicBool::new(false);
    let result = scan(&image.0, &options(), &cancel, |p| {
        if p.stage == "carving" {
            cancel.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    let r = scan_bytes(MP4_TAIL);
    assert_eq!(r.entries.len(), 1);
    let mut output = Vec::new();
    assert!(matches!(
        recover_entry(&image.0, &r.entries[0], &mut output, &cancel),
        Err(Error::Cancelled)
    ));
    assert!(output.is_empty());
}

#[test]
fn encrypted_sample_entries_are_rejected() {
    for encrypted in [*b"encv", *b"enca", *b"drms", *b"drmi"] {
        let mut bad = MP4_TAIL.to_vec();
        let stsd = table(&bad, *b"stsd");
        bad[stsd + 20..stsd + 24].copy_from_slice(&encrypted);
        reject(&bad, "encrypted sample entry");
    }
}

#[test]
fn unknown_brands_and_oversized_file_type_metadata_are_rejected() {
    let mut unknown = MP4_TAIL.to_vec();
    let ftyp_end = be32(&unknown, 0) as usize;
    unknown[8..12].copy_from_slice(b"zzzz");
    for at in (16..ftyp_end).step_by(4) {
        unknown[at..at + 4].copy_from_slice(b"zzzz");
    }
    reject(&unknown, "unknown ftyp brand");
    let mut huge = MP4_TAIL.to_vec();
    set32(&mut huge, 0, 5000);
    reject(&huge, "oversized ftyp metadata");
}

#[test]
fn sample_description_requires_an_existing_data_reference() {
    for reference in [0u16, 2, u16::MAX] {
        let mut bad = MP4_TAIL.to_vec();
        let stsd = table(&bad, *b"stsd");
        bad[stsd + 30..stsd + 32].copy_from_slice(&reference.to_be_bytes());
        reject(&bad, "invalid sample-description data reference");
    }
}

#[test]
fn composition_timing_cannot_add_missing_samples() {
    let mut bad = MP4_TAIL.to_vec();
    let ctts = table(&bad, *b"ctts");
    let first_count = be32(&bad, ctts + 16);
    set32(&mut bad, ctts + 16, first_count + 1);
    reject(&bad, "composition sample count mismatch");
}

#[test]
fn duplicate_required_sample_table_is_rejected() {
    let mut bad = MP4_TAIL.to_vec();
    let path = [*b"moov", *b"trak", *b"mdia", *b"minf", *b"stbl"];
    let ancestors: Vec<_> = (1..=path.len())
        .map(|n| atom_at_path(&bad, &path[..n]))
        .collect();
    let stbl = *ancestors.last().unwrap();
    let insert_at = stbl + be32(&bad, stbl) as usize;
    let original = table(&bad, *b"stsz");
    let length = be32(&bad, original) as usize;
    let duplicate = bad[original..original + length].to_vec();
    for at in ancestors {
        let length_before = be32(&bad, at);
        set32(&mut bad, at, length_before + length as u32);
    }
    bad.splice(insert_at..insert_at, duplicate);
    reject(&bad, "duplicate stsz singleton");
}

fn visual_extension(bytes: &[u8], wanted: [u8; 4]) -> (usize, usize) {
    let stsd = table(bytes, *b"stsd");
    let entry = stsd + 16; // FullBox/version + sample-description count.
    assert!(matches!(&bytes[entry + 4..entry + 8], b"avc1" | b"hvc1"));
    let end = entry + be32(bytes, entry) as usize;
    let mut at = entry + 8 + 78; // VisualSampleEntry's fixed fields.
    while at < end {
        let size = be32(bytes, at) as usize;
        assert!(size >= 8 && at + size <= end);
        if bytes[at + 4..at + 8] == wanted {
            return (at, size);
        }
        at += size;
    }
    panic!("Required original fixture extension not found: {wanted:?}");
}

#[test]
fn visual_codec_extension_cannot_escape_its_sample_entry() {
    for (original, kind, first_nal_length) in [
        (MP4_TAIL, *b"avcC", 8 + 6),
        (MP4_HEVC, *b"hvcC", 8 + 23 + 3),
    ] {
        let mut bad = original.to_vec();
        let (config, _) = visual_extension(&bad, kind);
        set32(&mut bad, config, u32::MAX);
        reject(&bad, "codec extension exceeds its sample-entry boundary");
        let mut bad_nal = original.to_vec();
        let at = config + first_nal_length;
        bad_nal[at..at + 2].copy_from_slice(&u16::MAX.to_be_bytes());
        reject(&bad_nal, "parameter-set length exceeds codec configuration");
    }
}

#[test]
fn video_handler_requires_the_complete_fixed_header() {
    let mut bad = MP4_TAIL.to_vec();
    let path = [*b"moov", *b"trak", *b"mdia"];
    let ancestors: Vec<_> = (1..=path.len())
        .map(|n| atom_at_path(&bad, &path[..n]))
        .collect();
    let hdlr = atom_at_path(&bad, &[*b"moov", *b"trak", *b"mdia", *b"hdlr"]);
    let old_size = be32(&bad, hdlr) as usize;
    let short_size = 8 + 12; // Preserves the video type, omits required fields.
    let removed = old_size - short_size;
    for at in ancestors {
        let old = be32(&bad, at);
        set32(&mut bad, at, old - removed as u32);
    }
    set32(&mut bad, hdlr, short_size as u32);
    bad.drain(hdlr + short_size..hdlr + old_size);
    // Parent and following sibling boundaries remain valid. This is a semantic
    // hdlr truncation, not an accidental malformed child after the shorter atom.
    reject(&bad, "handler fixed fields truncated to twelve bytes");
}

#[test]
fn avc1_requires_its_decoder_configuration() {
    let mut bad = MP4_TAIL.to_vec();
    let (avcc, removed) = visual_extension(&bad, *b"avcC");
    let path = [*b"moov", *b"trak", *b"mdia", *b"minf", *b"stbl", *b"stsd"];
    let mut ancestors: Vec<_> = (1..=path.len())
        .map(|n| atom_at_path(&bad, &path[..n]))
        .collect();
    ancestors.push(table(&bad, *b"stsd") + 16);
    for at in ancestors {
        let old = be32(&bad, at);
        set32(&mut bad, at, old - removed as u32);
    }
    bad.drain(avcc..avcc + removed);
    reject(&bad, "avc1 has no avcC configuration");
}
