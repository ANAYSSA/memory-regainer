use crate::{check_cancel, le16, le32, Entry, Error, Extent, Partition, Result, Source};
use std::{
    collections::{HashSet, VecDeque},
    sync::atomic::AtomicBool,
};

pub(crate) const METHOD_NOTE: &str = "FAT undelete examines deleted file records in live directories and assumes contiguous, currently free clusters. Deleted directories and orphan metadata are not searched; original allocation chains are unavailable after normal deletion. Verify recovered contents.";

struct Fat {
    base: u64,
    fat_start: u64,
    fat_size: u64,
    fat_count: u8,
    data: u64,
    root: u64,
    root_bytes: u64,
    cluster_bytes: u64,
    clusters: u32,
    bits: u8,
    root_cluster: u32,
    active: Option<u8>,
}
impl Fat {
    fn parse(source: &mut Source, part: &Partition) -> Result<Self> {
        let b = source.bytes(part.offset, 512)?;
        let sector = le16(&b, 11) as u64;
        let spc = b[13] as u64;
        let reserved = le16(&b, 14) as u64;
        let count = b[16];
        let roots = le16(&b, 17) as u64;
        let fat_sectors = if le16(&b, 22) != 0 {
            le16(&b, 22) as u64
        } else {
            le32(&b, 36) as u64
        };
        let total = if le16(&b, 19) != 0 {
            le16(&b, 19) as u64
        } else {
            le32(&b, 32) as u64
        };
        if ![512, 1024, 2048, 4096].contains(&sector)
            || !spc.is_power_of_two()
            || spc > 128
            || reserved == 0
            || !(1..=2).contains(&count)
            || fat_sectors == 0
        {
            return Err(Error::Invalid("Invalid FAT BPB".into()));
        }
        let root_sectors = (roots * 32).div_ceil(sector);
        let overhead = reserved + u64::from(count) * fat_sectors + root_sectors;
        if total <= overhead || total * sector > part.size {
            return Err(Error::Invalid("FAT volume length exceeds partition".into()));
        }
        let clusters = ((total - overhead) / spc) as u32;
        let bits = if clusters < 4085 {
            12
        } else if clusters < 65525 {
            16
        } else {
            32
        };
        let fat_size = fat_sectors * sector;
        if (u64::from(clusters) + 2) * u64::from(bits) > fat_size * 8 {
            return Err(Error::Invalid(
                "FAT is too small for the declared cluster count".into(),
            ));
        }
        if bits == 32 && (roots != 0 || le16(&b, 22) != 0 || le16(&b, 42) != 0) {
            return Err(Error::Invalid(
                "Invalid FAT32 BPB or unsupported revision".into(),
            ));
        }
        let active = if bits == 32 && le16(&b, 40) & 0x80 != 0 {
            Some((le16(&b, 40) & 15) as u8)
        } else {
            None
        };
        if active.is_some_and(|n| n >= count) {
            return Err(Error::Invalid("FAT32 active table is out of range".into()));
        }
        let root_cluster = if bits == 32 {
            le32(&b, 44) & 0x0fff_ffff
        } else {
            0
        };
        if bits == 32 && !(2..clusters + 2).contains(&root_cluster) {
            return Err(Error::Invalid("FAT32 root cluster is out of range".into()));
        }
        let fat_start = part.offset + reserved * sector;
        let root = fat_start + u64::from(count) * fat_size;
        Ok(Self {
            base: part.offset,
            fat_start,
            fat_size,
            fat_count: count,
            data: root + root_sectors * sector,
            root,
            root_bytes: roots * 32,
            cluster_bytes: sector * spc,
            clusters,
            bits,
            root_cluster,
            active,
        })
    }
    fn cluster_offset(&self, cluster: u32) -> Result<u64> {
        if cluster < 2 || cluster >= self.clusters + 2 {
            return Err(Error::Invalid("FAT cluster is out of range".into()));
        }
        Ok(self.data + u64::from(cluster - 2) * self.cluster_bytes)
    }
    fn table_entry(&self, source: &mut Source, cluster: u32, table: u8) -> Result<u32> {
        if cluster >= self.clusters + 2 {
            return Err(Error::Invalid("FAT cluster index exceeds table".into()));
        }
        let off = match self.bits {
            12 => u64::from(cluster) * 3 / 2,
            16 => u64::from(cluster) * 2,
            _ => u64::from(cluster) * 4,
        };
        let count = if self.bits == 32 { 4 } else { 2 };
        if off + count as u64 > self.fat_size {
            return Err(Error::Invalid("FAT entry exceeds table".into()));
        }
        let b = source.bytes(
            self.fat_start + u64::from(table) * self.fat_size + off,
            count,
        )?;
        Ok(match self.bits {
            12 => {
                let word = le16(&b, 0) as u32;
                if cluster & 1 == 0 {
                    word & 0xfff
                } else {
                    word >> 4
                }
            }
            16 => le16(&b, 0) as u32,
            _ => le32(&b, 0) & 0x0fff_ffff,
        })
    }
    fn next(&self, source: &mut Source, cluster: u32) -> Result<u32> {
        let table = self.active.unwrap_or(0);
        let value = self.table_entry(source, cluster, table)?;
        if self.active.is_none() {
            for other in 1..self.fat_count {
                if self.table_entry(source, cluster, other)? != value {
                    return Err(Error::Invalid("Mirrored FAT tables disagree".into()));
                }
            }
        }
        Ok(value)
    }
    fn is_end(&self, cluster: u32) -> bool {
        cluster
            >= match self.bits {
                12 => 0xff8,
                16 => 0xfff8,
                _ => 0x0fff_fff8,
            }
    }
}

fn short_name(record: &[u8]) -> String {
    let trim = |b: &[u8]| String::from_utf8_lossy(b).trim_end().to_string();
    let mut name = record[..8].to_vec();
    if name[0] == 0xe5 {
        name[0] = b'_';
    } else if name[0] == 5 {
        name[0] = 0xe5;
    }
    let base = trim(&name);
    let ext = trim(&record[8..11]);
    if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    }
}
fn checksum(short: &[u8]) -> u8 {
    short[..11]
        .iter()
        .fold(0u8, |sum, &byte| sum.rotate_right(1).wrapping_add(byte))
}
fn lfn_name(pending: &[[u8; 32]], short: &[u8]) -> Option<String> {
    if pending.is_empty() || pending.len() > 20 {
        return None;
    }
    let check = pending[0][13];
    if pending
        .iter()
        .any(|r| r[13] != check || r[12] != 0 || le16(r, 26) != 0)
    {
        return None;
    }
    let deleted = short[0] == 0xe5;
    if deleted {
        // Deletion destroys the first short-name byte and LFN ordinals. Require
        // a possible original checksum, but do not claim the lost order is proven.
        if pending.iter().any(|r| r[0] != 0xe5) {
            return None;
        }
        let mut candidate = short[..11].to_vec();
        if !(1..=255u16).any(|n| {
            candidate[0] = n as u8;
            checksum(&candidate) == check
        }) {
            return None;
        }
    } else {
        if checksum(short) != check || pending[0][0] & 0x40 == 0 {
            return None;
        }
        for (index, p) in pending.iter().enumerate() {
            if usize::from(p[0] & 0x1f) != pending.len() - index {
                return None;
            }
        }
    }
    let mut units = Vec::new();
    for p in pending.iter().rev() {
        for i in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
            units.push(le16(p, i));
        }
    }
    if let Some(end) = units.iter().position(|c| *c == 0 || *c == 0xffff) {
        units.truncate(end);
    }
    let name = String::from_utf16(&units).ok()?;
    if name.is_empty()
        || name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return None;
    }
    Some(name)
}

pub(crate) fn scan_deleted(
    source: &mut Source,
    part: &Partition,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64, usize),
) -> Result<(Vec<Entry>, Vec<String>)> {
    let fat = Fat::parse(source, part)?;
    let mut found = Vec::new();
    let mut notices = Vec::new();
    let mut queue = VecDeque::from([(fat.root_cluster, String::from("/"))]);
    let mut visited_dirs = HashSet::new();
    let mut rejected = 0usize;
    while let Some((first, directory)) = queue.pop_front() {
        check_cancel(cancel)?;
        if !visited_dirs.insert(first) {
            continue;
        }
        let mut chain = HashSet::new();
        let mut cluster = first;
        let mut lfn = Vec::<[u8; 32]>::new();
        let mut finished = false;
        loop {
            check_cancel(cancel)?;
            let (offset, length) = if first == 0 {
                (fat.root, fat.root_bytes)
            } else {
                if !chain.insert(cluster) {
                    notices.push(format!(
                        "Directory {directory} has a FAT loop; remaining records were skipped."
                    ));
                    break;
                }
                (fat.cluster_offset(cluster)?, fat.cluster_bytes)
            };
            let mut pos = 0;
            while pos < length {
                check_cancel(cancel)?;
                let len = (length - pos).min(64 * 1024) as usize;
                let block = source.bytes(offset + pos, len)?;
                for (n, r) in block.as_chunks::<32>().0.iter().enumerate() {
                    if r[0] == 0 {
                        finished = true;
                        break;
                    }
                    if r[11] == 0x0f {
                        if lfn.len() == 20 {
                            lfn.clear();
                        }
                        lfn.push(*r);
                        continue;
                    }
                    let long = lfn_name(&lfn, r);
                    lfn.clear();
                    if r[11] & 0x08 != 0 || r[0] == b'.' {
                        continue;
                    }
                    let deleted = r[0] == 0xe5;
                    let name = long.clone().unwrap_or_else(|| short_name(r));
                    let start = if fat.bits == 32 {
                        ((u32::from(le16(r, 20)) << 16) | u32::from(le16(r, 26))) & 0x0fff_ffff
                    } else {
                        u32::from(le16(r, 26))
                    };
                    if r[11] & 0x10 != 0 {
                        if !deleted && start >= 2 && start < fat.clusters + 2 {
                            queue.push_back((start, format!("{directory}{name}/")));
                        }
                        continue;
                    }
                    if !deleted {
                        continue;
                    }
                    let size = u64::from(le32(r, 28));
                    let record_offset = offset + pos + (n * 32) as u64;
                    let mut notes = vec![format!("Deleted FAT{} record in {directory}", fat.bits)];
                    if long.is_some() {
                        notes.push("Deleted long-name records have lost sequence numbers; adjacent name reconstruction is tentative.".into());
                    } else {
                        notes.push("The first short-name character was erased; '_' replaces it. Non-ASCII short names may be uncertain.".into());
                    }
                    let file_offset;
                    let extents;
                    if size == 0 {
                        file_offset = record_offset;
                        extents = vec![];
                        notes.push(
                            "Deleted zero-length directory record; no content bytes exist.".into(),
                        );
                    } else {
                        let needed = size.div_ceil(fat.cluster_bytes);
                        if start < 2 || u64::from(start) + needed > u64::from(fat.clusters) + 2 {
                            rejected += 1;
                            continue;
                        }
                        let mut free = true;
                        let mut invalid_metadata = false;
                        for step in 0..needed {
                            check_cancel(cancel)?;
                            match fat.next(source, start + step as u32) {
                                Ok(0) => {}
                                Ok(_) => {
                                    free = false;
                                    break;
                                }
                                Err(Error::Invalid(reason)) => {
                                    notices.push(format!(
                                        "Partial FAT scan: deleted record {directory}{name} at byte {record_offset} was skipped: {reason}. Other independent records were retained."
                                    ));
                                    invalid_metadata = true;
                                    break;
                                }
                                Err(error) => return Err(error),
                            }
                        }
                        if invalid_metadata {
                            continue;
                        }
                        if !free {
                            rejected += 1;
                            continue;
                        }
                        file_offset = fat.cluster_offset(start)?;
                        extents = vec![Extent {
                            offset: file_offset,
                            length: size,
                        }];
                        notes.push("All assumed clusters are currently free. Deletion erased the chain: contiguous allocation is an assumption; fragmented or overwritten content may be wrong.".into());
                    }
                    let kind = name
                        .rsplit_once('.')
                        .map_or("unknown", |(_, ext)| ext)
                        .to_ascii_lowercase();
                    found.push(Entry {
                        id: format!("fat-{:x}-{record_offset:x}", fat.base),
                        name,
                        kind,
                        size,
                        offset: file_offset,
                        confidence: if size == 0 { "high" } else { "low" }.into(),
                        method: "fat-undelete".into(),
                        notes,
                        extents,
                    });
                }
                progress(len as u64, found.len());
                pos += len as u64;
                if finished {
                    break;
                }
            }
            if first == 0 || finished {
                break;
            }
            let next = match fat.next(source, cluster) {
                Ok(next) => next,
                Err(Error::Invalid(reason)) => {
                    notices.push(format!(
                        "Partial FAT scan: directory {directory} stopped at cluster {cluster}: {reason}. Remaining directory records were skipped; previously found records were retained."
                    ));
                    break;
                }
                Err(error) => return Err(error),
            };
            if fat.is_end(next) {
                break;
            }
            if next < 2 || next >= fat.clusters + 2 {
                notices.push(format!(
                    "Directory {directory} has a broken FAT chain; remaining records were skipped."
                ));
                break;
            }
            cluster = next;
        }
    }
    if rejected > 0 {
        notices.push(format!("{rejected} deleted FAT records were skipped because their ranges were invalid or clusters are currently allocated. Fragmented deleted FAT chains are not reconstructed."));
    }
    Ok((found, notices))
}
