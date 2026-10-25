use crate::{check_cancel, le16, le32, le64, Error, Partition, Result, Source};
use std::sync::atomic::AtomicBool;

pub(crate) fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(u32::MAX, bytes)
}

pub(crate) fn filesystem(source: &mut Source, offset: u64, size: u64) -> Result<String> {
    if size < 512 {
        return Ok("Unknown".into());
    }
    let b = source.bytes(offset, 512)?;
    if &b[3..11] == b"EXFAT   " {
        return Ok("exFAT".into());
    }
    if &b[3..11] == b"NTFS    " {
        return Ok("NTFS (detection only)".into());
    }
    if &b[32..36] == b"NXSB" {
        return Ok("APFS (detection only)".into());
    }
    if b[510..512] == [0x55, 0xaa] {
        let sector = le16(&b, 11) as u64;
        let spc = b[13] as u64;
        let reserved = le16(&b, 14) as u64;
        let fats = b[16] as u64;
        let roots = le16(&b, 17) as u64;
        let total = if le16(&b, 19) != 0 {
            le16(&b, 19) as u64
        } else {
            le32(&b, 32) as u64
        };
        let fatsz = if le16(&b, 22) != 0 {
            le16(&b, 22) as u64
        } else {
            le32(&b, 36) as u64
        };
        if [512, 1024, 2048, 4096].contains(&sector)
            && spc.is_power_of_two()
            && spc <= 128
            && reserved > 0
            && (1..=2).contains(&fats)
            && fatsz > 0
        {
            let overhead = reserved + fats * fatsz + (roots * 32).div_ceil(sector);
            if total > overhead && total.saturating_mul(sector) <= size {
                let clusters = (total - overhead) / spc;
                if clusters < 4085 && le16(&b, 22) != 0 {
                    return Ok("FAT12".into());
                }
                if clusters < 65525 && le16(&b, 22) != 0 {
                    return Ok("FAT16".into());
                }
                if clusters >= 65525 && le16(&b, 22) == 0 && roots == 0 {
                    return Ok("FAT32".into());
                }
            }
        }
    }
    if size >= 2048 {
        let sb = source.bytes(offset + 1024, 1024)?;
        if le16(&sb, 56) == 0xef53 {
            return Ok("ext2/3/4 (detection only)".into());
        }
        if &sb[..2] == b"H+" || &sb[..2] == b"HX" {
            return Ok("HFS+ (detection only)".into());
        }
    }
    if b.starts_with(b"XFSB") {
        return Ok("XFS (detection only)".into());
    }
    if &b[3..11] == b"ReFS\0\0\0\0" {
        return Ok("ReFS (detection only)".into());
    }
    if size >= 65536 + 72 {
        let magic = source.bytes(offset + 65536 + 64, 8)?;
        if magic == b"_BHRfS_M" {
            return Ok("Btrfs (detection only)".into());
        }
    }
    Ok("Unknown".into())
}

pub(crate) fn discover(
    source: &mut Source,
    warnings: &mut Vec<String>,
    cancel: &AtomicBool,
) -> Result<Vec<Partition>> {
    let mut out = Vec::new();
    let whole_fs = filesystem(source, 0, source.len)?;
    // A superfloppy has a boot signature too. Do not interpret its boot code as MBR entries.
    if whole_fs != "Unknown" {
        return Ok(vec![Partition {
            name: "Whole source".into(),
            offset: 0,
            size: source.len,
            scheme: "Volume".into(),
            filesystem: whole_fs,
        }]);
    }
    for sector in [512u64, 4096] {
        if source.len >= sector + 92 && source.bytes(sector, 8)? == b"EFI PART" {
            match gpt(source, sector, cancel) {
                Ok(parts) => return Ok(parts),
                Err(Error::Cancelled) => return Err(Error::Cancelled),
                Err(Error::Io(e)) => return Err(Error::Io(e)),
                Err(e) => {
                    warnings.push(format!("GPT rejected: {e}. Only raw carving is available."))
                }
            }
            break;
        }
    }
    if source.len >= 512 {
        let mbr = source.bytes(0, 512)?;
        if mbr[510..512] == [0x55, 0xaa] {
            for i in 0..4 {
                check_cancel(cancel)?;
                let p = 446 + i * 16;
                let kind = mbr[p + 4];
                if kind == 0 || kind == 0xee {
                    continue;
                }
                if [0x05, 0x0f, 0x85].contains(&kind) {
                    warnings.push("MBR extended/logical partition traversal is not implemented; use Deep Scan for raw signatures.".into());
                    continue;
                }
                let offset = u64::from(le32(&mbr, p + 8)) * 512;
                let size = u64::from(le32(&mbr, p + 12)) * 512;
                if offset < 512
                    || size == 0
                    || offset.checked_add(size).is_none_or(|end| end > source.len)
                {
                    warnings.push(format!(
                        "MBR partition {} is outside source bounds and was ignored.",
                        i + 1
                    ));
                    continue;
                }
                if out.iter().any(|part: &Partition| {
                    offset < part.offset + part.size && part.offset < offset + size
                }) {
                    warnings.push(format!(
                        "MBR partition {} overlaps another partition and was ignored.",
                        i + 1
                    ));
                    continue;
                }
                let fs = filesystem(source, offset, size)?;
                out.push(Partition {
                    name: format!("Partition {} (0x{kind:02x})", i + 1),
                    offset,
                    size,
                    scheme: "MBR / 512-byte sectors".into(),
                    filesystem: fs,
                });
            }
        }
    }
    if out.is_empty() {
        out.push(Partition {
            name: "Whole source".into(),
            offset: 0,
            size: source.len,
            scheme: "Raw".into(),
            filesystem: whole_fs,
        });
    }
    Ok(out)
}

fn gpt(source: &mut Source, sector: u64, cancel: &AtomicBool) -> Result<Vec<Partition>> {
    let mut header = source.bytes(sector, sector as usize)?;
    let size = le32(&header, 12) as usize;
    if size < 92 || size > sector as usize {
        return Err(Error::Invalid("Invalid GPT header length".into()));
    }
    let expected = le32(&header, 16);
    header[16..20].fill(0);
    if crc32(&header[..size]) != expected {
        return Err(Error::Invalid("GPT header checksum mismatch".into()));
    }
    if le64(&header, 24) != 1 {
        return Err(Error::Invalid(
            "Primary GPT current-LBA field is not 1".into(),
        ));
    }
    let first = le64(&header, 40);
    let last = le64(&header, 48);
    let table = le64(&header, 72)
        .checked_mul(sector)
        .ok_or_else(|| Error::Invalid("GPT table address overflow".into()))?;
    let count = le32(&header, 80) as u64;
    let width = le32(&header, 84) as u64;
    if !(128..=4096).contains(&width) || !width.is_multiple_of(8) || count == 0 || count > 16384 {
        return Err(Error::Invalid(
            "GPT entry dimensions exceed defensive parser limits".into(),
        ));
    }
    let table_len = width * count;
    if first > last
        || last >= source.len / sector
        || table.checked_add(table_len).is_none_or(|n| n > source.len)
    {
        return Err(Error::Invalid(
            "GPT table/usable range exceeds source".into(),
        ));
    }
    let mut crc = u32::MAX;
    let mut entries = Vec::new();
    for i in 0..count {
        check_cancel(cancel)?;
        let b = source.bytes(table + i * width, width as usize)?;
        crc = crc32_update(crc, &b);
        if b[..16].iter().all(|v| *v == 0) {
            continue;
        }
        let lo = le64(&b, 32);
        let hi = le64(&b, 40);
        if lo > hi || lo < first || hi > last {
            return Err(Error::Invalid(format!(
                "GPT partition {} has invalid extent",
                i + 1
            )));
        }
        let offset = lo * sector;
        let size = (hi - lo + 1) * sector;
        if entries
            .iter()
            .any(|part: &Partition| offset < part.offset + part.size && part.offset < offset + size)
        {
            return Err(Error::Invalid("Overlapping GPT partitions".into()));
        }
        let units: Vec<u16> = b[56..128]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .take_while(|c| *c != 0)
            .collect();
        let name = String::from_utf16_lossy(&units);
        let fs = filesystem(source, offset, size)?;
        entries.push(Partition {
            name: if name.is_empty() {
                format!("Partition {}", i + 1)
            } else {
                name
            },
            offset,
            size,
            scheme: format!("GPT / {sector}-byte sectors"),
            filesystem: fs,
        });
    }
    if !crc != le32(&header, 88) {
        return Err(Error::Invalid(
            "GPT partition table checksum mismatch".into(),
        ));
    }
    if entries.is_empty() {
        entries.push(Partition {
            name: "Unpartitioned GPT space".into(),
            offset: first * sector,
            size: (last - first + 1) * sector,
            scheme: "GPT".into(),
            filesystem: "Unknown".into(),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_reference() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
