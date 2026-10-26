use crate::{check_cancel, le16, le32, Entry, Error, Extent, Partition, Result, Source};
use std::{
    collections::{HashSet, VecDeque},
    sync::atomic::AtomicBool,
};

pub(crate) const METHOD_NOTE: &str = "FAT metadata scanning includes existing files with validated allocation chains and deleted records with currently free contiguous clusters. Deleted directory names and ancestry are tentative; only the confirmed first cluster is searched when its allocation chain is cleared. Orphan metadata and fragmented deleted files are not reconstructed. Content bytes are not decoded or proven unchanged.";

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

fn record_cluster(fat: &Fat, record: &[u8]) -> u32 {
    if fat.bits == 32 {
        ((u32::from(le16(record, 20)) << 16) | u32::from(le16(record, 26))) & 0x0fff_ffff
    } else {
        u32::from(le16(record, 26))
    }
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

fn live_extents(
    fat: &Fat,
    source: &mut Source,
    start: u32,
    size: u64,
    cancel: &AtomicBool,
) -> Result<Vec<Extent>> {
    let count = size.div_ceil(fat.cluster_bytes);
    if count > u64::from(fat.clusters) {
        return Err(Error::Invalid("File length exceeds cluster heap".into()));
    }
    let mut extents: Vec<Extent> = Vec::new();
    let mut cluster = start;
    let mut anchor = start;
    let mut power = 1u64;
    let mut distance = 0u64;
    let mut remaining = size;
    for step in 0..count {
        check_cancel(cancel)?;
        let offset = fat.cluster_offset(cluster)?;
        let length = remaining.min(fat.cluster_bytes);
        if let Some(previous) = extents.last_mut().filter(|e| e.offset + e.length == offset) {
            previous.length += length;
        } else {
            // Bound memory even for adversarial one-cluster fragments.
            if extents.len() >= 1_000_000 {
                return Err(Error::Invalid(
                    "File exceeds the one-million-extent safety limit".into(),
                ));
            }
            extents.push(Extent { offset, length });
        }
        remaining -= length;
        let next = fat.next(source, cluster)?;
        if step + 1 == count {
            if !fat.is_end(next) {
                return Err(Error::Invalid(
                    "File FAT chain exceeds its declared length or contains a loop".into(),
                ));
            }
        } else {
            fat.cluster_offset(next)?;
            distance += 1;
            if next == anchor {
                return Err(Error::Invalid("File FAT chain contains a loop".into()));
            }
            if distance == power {
                anchor = next;
                power *= 2;
                distance = 0;
            }
            cluster = next;
        }
    }
    Ok(extents)
}

fn deleted_directory(fat: &Fat, source: &mut Source, first: u32, parent: u32) -> Result<()> {
    if fat.next(source, first)? != 0 {
        return Err(Error::Invalid(
            "first directory cluster is currently allocated".into(),
        ));
    }
    let records = source.bytes(fat.cluster_offset(first)?, fat.cluster_bytes as usize)?;
    let dot = &records[..32];
    let dotdot = &records[32..64];
    let parent_cluster = record_cluster(fat, dotdot);
    if &dot[..11] != b".          "
        || &dotdot[..11] != b"..         "
        || dot[11] != 0x10
        || dotdot[11] != 0x10
        || le32(dot, 28) != 0
        || le32(dotdot, 28) != 0
        || record_cluster(fat, dot) != first
        || !(parent_cluster == parent || (parent == fat.root_cluster && parent_cluster == 0))
    {
        return Err(Error::Invalid(
            "deleted directory lacks consistent . and .. records".into(),
        ));
    }
    for record in records[64..].as_chunks::<32>().0 {
        if record[0] == 0 {
            break;
        }
        if record[11] == 0x0f {
            if record[12] != 0 || le16(record, 26) != 0 {
                return Err(Error::Invalid(
                    "deleted directory contains implausible long-name records".into(),
                ));
            }
        } else if record[11] & 0xc0 != 0 || !safe_name(&short_name(record)) {
            return Err(Error::Invalid(
                "deleted directory contains implausible directory records".into(),
            ));
        }
    }
    Ok(())
}

struct Directory {
    first: u32,
    path: String,
    deleted: bool,
    parent: u32,
}

pub(crate) fn scan_deleted(
    source: &mut Source,
    part: &Partition,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64, usize),
    on_entry: &mut impl FnMut(&Entry),
) -> Result<(Vec<Entry>, Vec<String>)> {
    let fat = Fat::parse(source, part)?;
    let mut found = Vec::new();
    let mut notices = Vec::new();
    let mut queue = VecDeque::from([Directory {
        first: fat.root_cluster,
        path: String::new(),
        deleted: false,
        parent: 0,
    }]);
    let mut visited_dirs = HashSet::new();
    let mut rejected = 0usize;
    while let Some(Directory {
        first,
        path,
        deleted: inherited_deleted,
        parent,
    }) = queue.pop_front()
    {
        check_cancel(cancel)?;
        let directory = format!("/{path}");
        if !visited_dirs.insert(first) {
            notices.push(format!("Directory {directory} skipped: its first cluster was already visited (alias or cycle)."));
            continue;
        }
        if inherited_deleted {
            match deleted_directory(&fat, source, first, parent) {
                Ok(()) => notices.push(format!("Deleted directory {directory}: allocation chain is cleared; only its confirmed first cluster is searched. Its path and surviving child records are tentative.")),
                Err(Error::Invalid(reason)) => {
                    notices.push(format!("Deleted directory {directory} skipped: {reason}."));
                    continue;
                }
                Err(error) => return Err(error),
            }
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
                    check_cancel(cancel)?;
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
                    let record_deleted = r[0] == 0xe5;
                    let deleted = record_deleted || inherited_deleted;
                    let name = long.clone().unwrap_or_else(|| short_name(r));
                    if r[11] & 0xc0 != 0 || !safe_name(&name) {
                        rejected += 1;
                        continue;
                    }
                    let start = record_cluster(&fat, r);
                    if r[11] & 0x10 != 0 {
                        if start >= 2 && start < fat.clusters + 2 && le32(r, 28) == 0 {
                            let next = Directory {
                                first: start,
                                path: format!("{path}{name}/"),
                                deleted,
                                parent: first,
                            };
                            // Prefer authoritative live ancestry before tentative deleted aliases.
                            if deleted {
                                queue.push_back(next);
                            } else {
                                queue.push_front(next);
                            }
                        } else {
                            notices.push(format!("Directory {directory}{name} skipped: invalid first cluster or directory size."));
                        }
                        continue;
                    }
                    let size = u64::from(le32(r, 28));
                    let record_offset = offset + pos + (n * 32) as u64;
                    let mut notes = vec![format!(
                        "{} FAT{} record in {directory}",
                        if deleted { "Deleted" } else { "Existing" },
                        fat.bits
                    )];
                    if record_deleted {
                        if long.is_some() {
                            notes.push("Deleted long-name records have lost sequence numbers; adjacent name reconstruction is tentative.".into());
                        } else {
                            notes.push("The first short-name character was erased; '_' replaces it. Non-ASCII short names may be uncertain.".into());
                        }
                    }
                    if inherited_deleted {
                        notes.push("This record survives inside a deleted directory; deletion status and original ancestry are inferred and tentative.".into());
                    }
                    let uncertain_short_name =
                        long.is_none() && r[..11].iter().any(|b| *b >= 0x80 && *b != 0xe5);
                    if uncertain_short_name {
                        notes.push("The OEM code page for this short name is unknown; non-ASCII filename characters and the resulting path may be uncertain.".into());
                    }
                    let allocation = if size == 0 {
                        Ok(Vec::new())
                    } else if deleted {
                        let needed = size.div_ceil(fat.cluster_bytes);
                        if start < 2 || u64::from(start) + needed > u64::from(fat.clusters) + 2 {
                            rejected += 1;
                            continue;
                        }
                        let mut result = Ok(vec![Extent {
                            offset: fat.cluster_offset(start)?,
                            length: size,
                        }]);
                        for step in 0..needed {
                            check_cancel(cancel)?;
                            match fat.next(source, start + step as u32) {
                                Ok(0) => {}
                                Ok(_) => {
                                    result = Err(Error::Invalid(
                                        "content clusters are currently allocated".into(),
                                    ));
                                    break;
                                }
                                Err(error) => {
                                    result = Err(error);
                                    break;
                                }
                            }
                        }
                        result
                    } else {
                        live_extents(&fat, source, start, size, cancel)
                    };
                    let extents = match allocation {
                        Ok(extents) => extents,
                        Err(Error::Invalid(reason)) => {
                            notices.push(format!("Partial FAT scan: {} record {directory}{name} at byte {record_offset} was skipped: {reason}. Other independent records were retained.", if deleted { "deleted" } else { "existing" }));
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if size == 0 {
                        notes.push("Zero-length directory record; no content bytes exist.".into());
                    } else if deleted {
                        notes.push("All assumed clusters are currently free. Deletion erased the chain: contiguous allocation is an assumption; fragmented or overwritten content may be wrong.".into());
                    } else {
                        notes.push("The complete current FAT chain matches the declared file length. Content is not decoded; a changing mounted filesystem can invalidate this snapshot.".into());
                    }
                    let kind = name
                        .rsplit_once('.')
                        .map_or("unknown", |(_, ext)| ext)
                        .to_ascii_lowercase();
                    let entry = Entry {
                        id: format!("fat-{:x}-{record_offset:x}", fat.base),
                        relative_path: Some(format!("{path}{name}")),
                        partition: Some(part.name.clone()),
                        deletion_status: if deleted { "deleted" } else { "existing" }.into(),
                        name,
                        kind,
                        size,
                        offset: extents.first().map_or(record_offset, |e| e.offset),
                        confidence: if deleted {
                            "low"
                        } else if uncertain_short_name {
                            "medium"
                        } else {
                            "high"
                        }
                        .into(),
                        method: if deleted {
                            "fat-undelete"
                        } else {
                            "fat-metadata"
                        }
                        .into(),
                        notes,
                        extents,
                    };
                    on_entry(&entry);
                    found.push(entry);
                }
                progress(len as u64, found.len());
                pos += len as u64;
                if finished {
                    break;
                }
            }
            if first == 0 || finished || inherited_deleted {
                break;
            }
            let next = match fat.next(source, cluster) {
                Ok(next) => next,
                Err(Error::Invalid(reason)) => {
                    notices.push(format!("Partial FAT scan: directory {directory} stopped at cluster {cluster}: {reason}. Remaining directory records were skipped; previously found records were retained."));
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
        notices.push(format!("{rejected} FAT records were skipped because their ranges, names or allocation chains were invalid, or deleted content clusters are currently allocated. Fragmented deleted FAT chains are not reconstructed."));
    }
    Ok((found, notices))
}
