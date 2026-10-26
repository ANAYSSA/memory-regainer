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
// Public Microsoft FAT v1.03: FAT type comes from cluster count, while the
// boot label/type strings are informational copies. The volume-label directory
// record is identified by ATTR_VOLUME_ID (0x08) in the root directory.
fn verify_fat32_root_label(
    reader: &mut SourceReader,
    boot: &[u8; 512],
) -> Result<String, Box<dyn std::error::Error>> {
    let word = |at| u16::from_le_bytes(boot[at..at + 2].try_into().unwrap());
    let dword = |at| u32::from_le_bytes(boot[at..at + 4].try_into().unwrap());
    let sector = u64::from(word(11));
    let per_cluster = u64::from(boot[13]);
    let reserved = u64::from(word(14));
    let tables = u64::from(boot[16]);
    let fat_sectors = u64::from(dword(36));
    let sectors = u64::from(dword(32));
    let overhead = reserved + tables * fat_sectors;
    if boot[510..] != [0x55, 0xaa]
        || ![512, 1024, 2048, 4096].contains(&sector)
        || !per_cluster.is_power_of_two()
        || per_cluster > 128
        || reserved == 0
        || !(1..=2).contains(&tables)
        || fat_sectors == 0
        || sectors <= overhead
        || sectors * sector > reader.len()
        || word(17) != 0
        || word(22) != 0
        || word(19) != 0
        || word(42) != 0
    {
        return Err("Scratch FAT32 boot geometry is invalid".into());
    }
    let clusters = (sectors - overhead) / per_cluster;
    if !(65_525..=0x0fff_fff5).contains(&clusters) || (clusters + 2) * 4 > fat_sectors * sector {
        return Err("Scratch cluster count/table capacity is not FAT32".into());
    }
    let heap = overhead * sector;
    let cluster_bytes = sector * per_cluster;
    let cluster = u64::from(dword(44) & 0x0fff_ffff);
    if cluster < 2 || cluster >= clusters + 2 {
        return Err("Scratch root cluster is invalid".into());
    }
    // This fresh fixture creates its label before any files. Inspect exactly
    // the first root cluster; the application scanner validates full chains.
    let mut bytes = vec![0u8; cluster_bytes as usize];
    reader.read_at(heap + (cluster - 2) * cluster_bytes, &mut bytes)?;
    for record in bytes.as_chunks::<32>().0 {
        if record[0] == 0 {
            break;
        }
        if record[0] != 0xe5 && record[11] == 0x08 {
            let label = String::from_utf8_lossy(&record[..11]).trim_end().to_owned();
            if &record[..11] != b"MRPROBE    " {
                return Err(
                    format!("Scratch root volume label differs from MRPROBE: {label:?}").into(),
                );
            }
            return Ok(label);
        }
    }
    Err("Scratch first root cluster has no MRPROBE volume-label record".into())
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
    eprintln!(
        "Scratch boot label={:?}, informational filesystem type={:?}",
        String::from_utf8_lossy(&boot[71..82]),
        String::from_utf8_lossy(&boot[82..90])
    );
    let root_label = verify_fat32_root_label(&mut reader, &boot)?;
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
            &serde_json::json!({"raw_volume_verified":true,"path":args[1],"volume_bytes":size,"boot_label":String::from_utf8_lossy(&boot[71..82]),"root_volume_label":root_label,"boot_filesystem_type":String::from_utf8_lossy(&boot[82..90]),"scan_results":evidence})
        )?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    #[test]
    fn root_volume_label_is_verified_when_boot_copy_is_no_name() {
        let path = std::env::temp_dir().join(format!("mr-volume-label-{}.img", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut boot = [0u8; 512];
        let clusters = 65_526u32;
        let fats = ((clusters + 2) * 4).div_ceil(512);
        let sectors = 32 + fats + clusters;
        boot[11..13].copy_from_slice(&512u16.to_le_bytes());
        boot[13] = 1;
        boot[14..16].copy_from_slice(&32u16.to_le_bytes());
        boot[16] = 1;
        boot[32..36].copy_from_slice(&sectors.to_le_bytes());
        boot[36..40].copy_from_slice(&fats.to_le_bytes());
        boot[44..48].copy_from_slice(&2u32.to_le_bytes());
        boot[71..82].copy_from_slice(b"NO NAME    ");
        boot[82..90].copy_from_slice(b"FAT32   ");
        boot[510..].copy_from_slice(&[0x55, 0xaa]);
        file.set_len(u64::from(sectors) * 512).unwrap();
        file.write_all(&boot).unwrap();
        let heap = u64::from(32 + fats) * 512;
        file.seek(SeekFrom::Start(heap)).unwrap();
        let mut label = [0u8; 32];
        label[..11].copy_from_slice(b"MRPROBE    ");
        label[11] = 0x08;
        file.write_all(&label).unwrap();
        drop(file);
        let mut reader = SourceReader::open(&path, None).unwrap();
        verify_fat32_root_label(&mut reader, &boot).unwrap();
        drop(reader);
        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(heap + 11)).unwrap();
        file.write_all(&[0x20]).unwrap();
        drop(file);
        let mut reader = SourceReader::open(&path, None).unwrap();
        assert!(verify_fat32_root_label(&mut reader, &boot).is_err());
        drop(reader);
        fs::remove_file(path).unwrap();
    }
}
