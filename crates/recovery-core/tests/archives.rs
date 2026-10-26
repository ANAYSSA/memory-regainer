use recovery_core::{recover_entry, scan, scan_with_events, Error, ScanEvent, ScanOptions};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
const ZIP: &[u8] = include_bytes!("fixtures/archives/classic.zip");
const DD: &[u8] = include_bytes!("fixtures/archives/descriptor.zip");
const DD_UNSIGNED: &[u8] = include_bytes!("fixtures/archives/descriptor-unsigned.zip");
const DD64: &[u8] = include_bytes!("fixtures/archives/descriptor-zip64.zip");
const ZIP64: &[u8] = include_bytes!("fixtures/archives/full-zip64.zip");
const DOCX: &[u8] = include_bytes!("fixtures/archives/original.docx");
const DOCX_DD: &[u8] = include_bytes!("fixtures/archives/descriptor.docx");
const RAR4: &[u8] = include_bytes!("fixtures/archives/stored-rar4.rar");
const RAR5: &[u8] = include_bytes!("fixtures/archives/stored-rar5.rar");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Image(PathBuf);
impl Image {
    fn new(b: &[u8]) -> Self {
        let p = std::env::temp_dir().join(format!(
            "memory-regainer-archive-{}-{}.img",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&p, b).unwrap();
        Self(p)
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn opts(types: &[&str]) -> ScanOptions {
    ScanOptions {
        mode: "deep".into(),
        types: types.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}
fn found(b: &[u8], types: &[&str]) -> Vec<recovery_core::Entry> {
    let f = Image::new(b);
    scan(&f.0, &opts(types), &AtomicBool::new(false), |_| {})
        .unwrap()
        .entries
}
fn marker(b: &[u8], tag: &[u8]) -> usize {
    b.windows(tag.len()).position(|w| w == tag).unwrap()
}
fn set16(b: &mut [u8], p: usize, v: u16) {
    b[p..p + 2].copy_from_slice(&v.to_le_bytes());
}
fn set32(b: &mut [u8], p: usize, v: u32) {
    b[p..p + 4].copy_from_slice(&v.to_le_bytes());
}
fn crc(b: &[u8]) -> u32 {
    let mut v = u32::MAX;
    for byte in b {
        v ^= u32::from(*byte);
        for _ in 0..8 {
            v = (v >> 1) ^ if v & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !v
}
fn rar4_recrc(b: &mut [u8], at: usize) {
    let n = u16::from_le_bytes(b[at + 5..at + 7].try_into().unwrap()) as usize;
    let c = crc(&b[at + 2..at + n]) as u16;
    set16(b, at, c);
}
fn rar5_recrc(b: &mut [u8], at: usize) {
    let n = b[at + 4] as usize;
    assert!(n < 128);
    let c = crc(&b[at + 4..at + 5 + n]);
    set32(b, at, c);
}
#[test]
fn classic_streaming_unsigned_zip64_docx_and_rar_roundtrip_exactly() {
    for (name, kind, bytes) in [
        ("classic.zip", "zip", ZIP),
        ("descriptor.zip", "zip", DD),
        ("descriptor-unsigned.zip", "zip", DD_UNSIGNED),
        ("descriptor-zip64.zip", "zip", DD64),
        ("full-zip64.zip", "zip", ZIP64),
        ("original.docx", "docx", DOCX),
        ("descriptor.docx", "docx", DOCX_DD),
        ("stored-rar4.rar", "rar", RAR4),
        ("stored-rar5.rar", "rar", RAR5),
    ] {
        let prefix = 4099;
        let mut disk = vec![0x66; prefix];
        disk.extend_from_slice(bytes);
        disk.extend_from_slice(&[0x77; 2048]);
        let f = Image::new(&disk);
        let r = scan(&f.0, &opts(&[kind]), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(r.entries.len(), 1, "{name}: {r:#?}");
        let e = &r.entries[0];
        assert_eq!(e.kind, kind);
        assert_eq!(e.offset, prefix as u64);
        assert_eq!(e.size, bytes.len() as u64);
        assert_eq!(e.deletion_status, "unknown");
        assert!(e.relative_path.is_none());
        let mut out = Vec::new();
        recover_entry(&f.0, e, &mut out, &AtomicBool::new(false)).unwrap();
        assert_eq!(out, bytes);
        assert_eq!(Sha256::digest(&out), Sha256::digest(bytes));
        assert_eq!(fs::read(&f.0).unwrap(), disk);
        if let Some(p) = std::env::var_os("MEMORY_REGAINER_ARCHIVE_EXPORT_DIR") {
            fs::create_dir_all(&p).unwrap();
            fs::write(PathBuf::from(p).join(name), out).unwrap();
        }
    }
}
#[test]
fn docx_is_a_verified_subtype_and_zip_filter_includes_it() {
    assert_eq!(found(DOCX, &["zip"])[0].kind, "docx");
    assert!(found(ZIP, &["docx"]).is_empty());
    for b in [
        include_bytes!("fixtures/archives/names-only.zip").as_slice(),
        include_bytes!("fixtures/archives/external-main.zip").as_slice(),
        include_bytes!("fixtures/archives/doctype.zip").as_slice(),
    ] {
        assert_eq!(found(b, &["zip"])[0].kind, "zip");
        assert!(found(b, &["docx"]).is_empty());
    }
}
#[test]
fn jpeg_alias_is_supported_by_registry_and_scan_filter() {
    let bytes = include_bytes!("fixtures/original.jpg");
    assert_eq!(found(bytes, &["jpeg"])[0].kind, "jpg");
}
#[test]
fn archive_end_signature_straddles_chunk_boundary() {
    let end = marker(DD, b"PK\x05\x06");
    let prefix = 1024 * 1024 - 18 - end;
    let mut bytes = vec![0x66; prefix];
    bytes.extend_from_slice(DD);
    let entries = found(&bytes, &["zip"]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].offset, prefix as u64);
}
#[test]
fn adjacent_archives_keep_their_exact_boundaries() {
    let b = [ZIP, RAR4, DOCX_DD, RAR5, ZIP64].concat();
    let e = found(&b, &["zip", "rar"]);
    assert_eq!(e.len(), 5);
    let mut at = 0;
    for (e, payload) in e.iter().zip([ZIP, RAR4, DOCX_DD, RAR5, ZIP64]) {
        assert_eq!(e.offset, at);
        assert_eq!(e.size, payload.len() as u64);
        at += payload.len() as u64;
    }
}
#[test]
fn truncated_archive_end_is_not_reported() {
    for b in [ZIP, DD, ZIP64, DOCX, RAR4, RAR5] {
        assert!(found(&b[..b.len() - 1], &["zip", "rar"]).is_empty());
    }
}
#[test]
fn zip_local_and_central_names_sizes_and_crc_must_agree() {
    for field in [14, 18, 30] {
        let mut b = ZIP.to_vec();
        b[field] ^= 1;
        assert!(found(&b, &["zip"]).is_empty(), "field {field}");
    }
}
#[test]
fn descriptor_values_must_match_authoritative_central_record() {
    for b in [DD, DD64] {
        let mut bad = b.to_vec();
        let at = marker(&bad, b"PK\x07\x08");
        bad[at + 4] ^= 1;
        assert!(found(&bad, &["zip"]).is_empty());
    }
}
#[test]
fn central_local_offsets_cannot_escape_the_selected_archive() {
    let mut b = ZIP.to_vec();
    let at = marker(&b, b"PK\x01\x02");
    set32(&mut b, at + 42, u32::MAX - 1);
    assert!(found(&b, &["zip"]).is_empty());
}
#[test]
fn zip64_counts_and_relative_end_offsets_are_bounded() {
    for field in [32, 48] {
        let mut b = ZIP64.to_vec();
        let at = marker(&b, b"PK\x06\x06");
        b[at + field..at + field + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(found(&b, &["zip"]).is_empty());
    }
}
#[test]
fn zip_multivolume_and_encryption_are_rejected() {
    let mut volume = ZIP.to_vec();
    let at = marker(&volume, b"PK\x05\x06");
    set16(&mut volume, at + 4, 1);
    assert!(found(&volume, &["zip"]).is_empty());
    let mut encrypted = ZIP.to_vec();
    let at = marker(&encrypted, b"PK\x01\x02");
    set16(&mut encrypted, 6, 1);
    set16(&mut encrypted, at + 8, 1);
    assert!(found(&encrypted, &["zip"]).is_empty());
}
#[test]
fn rar_header_checksum_corruption_is_rejected() {
    for b in [RAR4, RAR5] {
        let mut bad = b.to_vec();
        bad[10] ^= 1;
        assert!(found(&bad, &["rar"]).is_empty());
    }
}
#[test]
fn rar_split_and_encryption_flags_rejected_with_valid_header_checksums() {
    for flag in [1, 128] {
        let mut b = RAR4.to_vec();
        set16(&mut b, 10, flag);
        rar4_recrc(&mut b, 7);
        assert!(found(&b, &["rar"]).is_empty());
    }
    for flag in [1, 2] {
        let mut b = RAR5.to_vec();
        b[15] = flag;
        rar5_recrc(&mut b, 8);
        assert!(found(&b, &["rar"]).is_empty());
    }
}
#[test]
fn rar_overlong_vint_and_missing_explicit_end_are_rejected() {
    let mut b = RAR5.to_vec();
    b[12..15].fill(0x80);
    assert!(found(&b, &["rar"]).is_empty());
    assert!(found(&RAR4[..RAR4.len() - 7], &["rar"]).is_empty());
    assert!(found(&RAR5[..RAR5.len() - 8], &["rar"]).is_empty());
}
#[test]
fn bounded_range_does_not_follow_zip_central_pointer_outside_selection() {
    let b = [vec![0x66; 512], ZIP.to_vec()].concat();
    let f = Image::new(&b);
    let mut o = opts(&["zip"]);
    o.start = Some(513);
    o.end = Some(b.len() as u64);
    assert!(scan(&f.0, &o, &AtomicBool::new(false), |_| {})
        .unwrap()
        .entries
        .is_empty());
}
#[test]
fn entry_events_arrive_before_completion_and_survive_cancellation() {
    let mut b = ZIP.to_vec();
    b.resize(2 * 1024 * 1024, 0x66);
    let f = Image::new(&b);
    let cancel = AtomicBool::new(false);
    let mut entries = Vec::new();
    let result = scan_with_events(
        &f.0,
        &opts(&["zip"]),
        None,
        &cancel,
        |_| {},
        |event| {
            if let ScanEvent::Entry(e) = event {
                entries.push(e);
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, ZIP.len() as u64);
}
#[test]
fn diagnostics_count_validated_and_rejected_headers_with_bounded_reasons() {
    let mut b = ZIP.to_vec();
    b.extend_from_slice(b"PK\x05\x06");
    b.extend_from_slice(&[255; 32]);
    let f = Image::new(&b);
    let mut last = None;
    scan_with_events(
        &f.0,
        &opts(&["zip"]),
        None,
        &AtomicBool::new(false),
        |_| {},
        |e| {
            if let ScanEvent::Diagnostics(d) = e {
                last = Some(d);
            }
        },
    )
    .unwrap();
    let d = last.unwrap();
    assert_eq!(d.accepted, 1);
    assert!(d.rejected >= 1);
    assert_eq!(d.candidates, d.accepted + d.rejected);
    assert!(d.reasons.len() <= 32);
}

#[test]
fn embedded_end_magic_inside_stored_payload_does_not_truncate_archive() {
    let bytes = include_bytes!("fixtures/archives/embedded-end.zip");
    let entries = found(bytes, &["zip"]);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, bytes.len() as u64);
}
#[test]
fn docx_metadata_inflation_is_bounded_without_losing_the_zip_container() {
    let bytes = include_bytes!("fixtures/archives/oversized-docx-metadata.zip");
    assert!(found(bytes, &["docx"]).is_empty());
    assert_eq!(found(bytes, &["zip"])[0].kind, "zip");
}
#[test]
fn mutated_archive_records_never_panic_or_escape_small_source_bounds() {
    for original in [ZIP, DD, ZIP64, DOCX, RAR4, RAR5] {
        for seed in 0..64usize {
            let mut bytes = original.to_vec();
            let at = (seed * 137 + 11) % bytes.len();
            let value = if seed % 2 == 0 { 0xff } else { 0 };
            let end = (at + 8).min(bytes.len());
            bytes[at..end].fill(value);
            let f = Image::new(&bytes);
            let result = std::panic::catch_unwind(|| {
                scan(
                    &f.0,
                    &opts(&["zip", "rar"]),
                    &AtomicBool::new(false),
                    |_| {},
                )
            });
            assert!(
                result.is_ok(),
                "seed{seed}, source length{}",
                original.len()
            );
            if let Ok(Ok(r)) = result {
                for entry in r.entries {
                    assert!(entry.offset + entry.size <= bytes.len() as u64);
                }
            }
        }
    }
}
