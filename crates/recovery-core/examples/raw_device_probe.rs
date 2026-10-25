//! CI-only probe for an explicitly supplied, newly created all-zero scratch disk.
//! This executable does not discover devices and never opens a source for writing.
use recovery_core::{scan, ScanOptions, SourceReader};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: raw_device_probe EXPLICIT_SCRATCH_RAW_PATH EXPECTED_BYTES (all-zero disposable VHD only)".into());
    }
    let path = Path::new(&args[1]);
    let size = args[2].parse::<u64>()?;
    if !(1024 * 1024..=64 * 1024 * 1024).contains(&size) {
        return Err("Probe requires a 1–64MiB disposable scratch device".into());
    }
    let legacy_metadata = match std::fs::File::open(path).and_then(|file| file.metadata()) {
        Ok(metadata) => serde_json::json!({"ok":true,"length":metadata.len()}),
        Err(error) => {
            serde_json::json!({"ok":false,"os_error":error.raw_os_error(),"message":error.to_string()})
        }
    };
    let native = SourceReader::open(path, None)?;
    if native.len() != size {
        return Err("Native raw geometry does not match scratch capacity".into());
    }
    let mut reader = SourceReader::open(path, Some(size))?;
    if !reader.is_raw() || reader.len() != size {
        return Err("Probe target is not a raw device with the expected scratch capacity".into());
    }
    for (offset, length) in [(1, 511), (4093, 19), (65535, 257), (size - 123, 123)] {
        let mut bytes = vec![0xa5; length];
        reader.read_at(offset, &mut bytes)?;
        if bytes.iter().any(|b| *b != 0) {
            return Err(format!(
                "Scratch device is not all-zero at byte {offset}; stopping without scanning"
            )
            .into());
        }
    }
    let mut modes = Vec::new();
    for mode in ["quick", "deep"] {
        let mut stages = Vec::new();
        let mut last_bytes = 0;
        let options = ScanOptions {
            mode: mode.into(),
            start: None,
            end: Some(size),
            types: vec![],
        };
        let result = scan(path, &options, &AtomicBool::new(false), |progress| {
            assert!(progress.bytes_read >= last_bytes);
            last_bytes = progress.bytes_read;
            if stages.last() != Some(&progress.stage) {
                stages.push(progress.stage.clone());
            }
            if progress.stage == "metadata" {
                assert!(progress.indeterminate);
                assert_eq!(progress.total, 0);
            }
            if progress.stage == "carving" {
                assert!(!progress.indeterminate);
                assert_eq!(progress.total, size);
                assert!(progress.scanned <= size);
            }
        })?;
        if !result.entries.is_empty() {
            return Err("Unexpected candidate in disposable all-zero device".into());
        }
        if stages.first().map(String::as_str) != Some("opening-source")
            || stages.last().map(String::as_str) != Some("finished")
        {
            return Err("Missing progress stage transition".into());
        }
        modes.push(serde_json::json!({"mode":mode,"stages":stages,"actual_read_bytes":last_bytes,"files_found":result.entries.len()}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"raw_device_verified":true,"legacy_file_metadata_diagnostic":legacy_metadata,"size":reader.len(),"transfer_sector_size":reader.sector_size(),"unaligned_range_read_operations":reader.read_operations(),"unaligned_range_bytes_read":reader.bytes_read(),"scan_results":modes})
        )?
    );
    Ok(())
}
