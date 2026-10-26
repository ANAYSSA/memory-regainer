//! CI-only read-only probe for an explicitly supplied, newly created FAT32 VHD
//! volume. The PowerShell harness resolves and protects that VHD before calling
//! this program. This executable never enumerates disks or writes source bytes.
use recovery_core::{
    recover_entry_with_progress_and_size, scan_with_events, ScanEvent, ScanOptions, SourceReader,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path, sync::atomic::AtomicBool};
#[derive(Deserialize)]
struct Expected {
    relative_path: String,
    source_path: String,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        return Err(
            "Usage: mounted_volume_probe EXPLICIT_SCRATCH_RAW_VOLUME EXPECTED_BYTES MANIFEST_JSON"
                .into(),
        );
    }
    let raw = args[1]
        .strip_prefix(r"\\.\")
        .ok_or("Expected explicit raw drive-letter path")?
        .as_bytes();
    if raw.len() != 2 || !raw[0].is_ascii_alphabetic() || raw[1] != b':' {
        return Err("Expected explicit raw drive letter".into());
    }
    let size = args[2].parse::<u64>()?;
    if !(64 * 1024 * 1024..=256 * 1024 * 1024).contains(&size) {
        return Err("Probe requires a 64–256 MiB disposable VHD volume".into());
    }
    let path = Path::new(&args[1]);
    let expected: Vec<Expected> = serde_json::from_slice(&fs::read(&args[3])?)?;
    if expected.len() != 45 {
        return Err(
            "Probe manifest must contain 42 original clips, 2 photos and one marker".into(),
        );
    }
    let mut reader = SourceReader::open(path, Some(size))?;
    if !reader.is_raw() || reader.len() != size {
        return Err("Raw mounted-volume size mismatch".into());
    }
    let mut boot = [0u8; 512];
    reader.read_at(0, &mut boot)?;
    if &boot[82..90] != b"FAT32   " || &boot[71..82] != b"MRPROBE    " {
        return Err("Volume is not the labelled scratch FAT32 fixture".into());
    }
    // Unaligned raw reads must equal slices of aligned reads, including the tail.
    for offset in [1, 511, 65535, size - 513] {
        let mut unaligned = [0u8; 17];
        reader.read_at(offset, &mut unaligned)?;
        let aligned = offset / 512 * 512;
        let mut reference = [0u8; 1024];
        let count = (size - aligned).min(1024) as usize;
        reader.read_at(aligned, &mut reference[..count])?;
        assert_eq!(
            unaligned,
            reference[(offset - aligned) as usize..(offset - aligned) as usize + 17]
        );
    }
    let mut evidence = Vec::new();
    for mode in ["quick", "deep"] {
        let mut streamed = 0usize;
        let mut last_read = 0;
        let mut stages = Vec::new();
        let result = scan_with_events(
            path,
            &ScanOptions {
                mode: mode.into(),
                ..Default::default()
            },
            Some(size),
            &AtomicBool::new(false),
            |p| {
                assert!(p.bytes_read >= last_read);
                last_read = p.bytes_read;
                if stages.last() != Some(&p.stage) {
                    stages.push(p.stage);
                }
            },
            |event| {
                if matches!(event, ScanEvent::Entry(_)) {
                    streamed += 1;
                }
            },
        )?;
        assert_eq!(streamed, result.entries.len());
        let by_path: BTreeMap<_, _> = result
            .entries
            .iter()
            .filter_map(|e| e.relative_path.as_deref().map(|p| (p, e)))
            .collect();
        let mut files = Vec::new();
        for expected in &expected {
            let entry = by_path
                .get(expected.relative_path.as_str())
                .ok_or_else(|| {
                    format!(
                        "Missing expected metadata path {}: {:?}",
                        expected.relative_path, result.warnings
                    )
                })?;
            assert_eq!(entry.deletion_status, "existing");
            let mut recovered = Vec::new();
            recover_entry_with_progress_and_size(
                path,
                entry,
                &mut recovered,
                &AtomicBool::new(false),
                Some(size),
                |_, _| {},
            )?;
            let original = fs::read(&expected.source_path)?;
            assert_eq!(recovered.len(), original.len());
            let digest = Sha256::digest(&recovered);
            assert_eq!(digest, Sha256::digest(original));
            files.push(serde_json::json!({"path":expected.relative_path,"size":recovered.len(),"sha256":format!("{digest:x}")}));
        }
        evidence.push(serde_json::json!({"mode":mode,"streamed_entries":streamed,"verified_files":files,"bytes_read":last_read,"stages":stages}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"raw_volume_verified":true,"path":args[1],"volume_bytes":size,"scan_results":evidence})
        )?
    );
    Ok(())
}
