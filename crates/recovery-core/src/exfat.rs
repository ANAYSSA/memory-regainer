//! Conservative exFAT 1.00 metadata undelete, implemented from the public Microsoft
//! specification: https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification
//! No proprietary recovery implementation is used. Inactive records are undefined
//! by the filesystem specification: their former active checksum is an additional
//! recovery heuristic, not a guarantee. Only clean single-FAT volumes and intact,
//! fully initialized streams are supported. Deleted streams require NoFatChain and
//! currently free clusters; surviving directory ancestry remains tentative.
use crate::{check_cancel, le16, le32, le64, Entry, Error, Extent, Partition, Result, Source};
use std::{
    collections::{HashSet, VecDeque},
    sync::atomic::AtomicBool,
};

pub(crate) const METHOD_NOTE: &str = "exFAT metadata scanning supports existing allocated streams, including FAT chains, on clean single-FAT revision 1.00 volumes. Deleted files and directories require intact contiguous streams, reconstructed entry checksums and currently free clusters; deleted ancestry is tentative. TexFAT, fragmented deleted streams and partial initialization are unsupported. Content bytes are not decoded or proven unchanged.";

const MAX_DIRECTORY: u64 = 256 * 1024 * 1024;
const PAGE: u64 = 64 * 1024;
fn invalid(message: &str) -> Error {
    Error::Invalid(format!("exFAT: {message}"))
}

struct Geometry {
    base: u64,
    fat: u64,
    heap: u64,
    cluster_bytes: u64,
    clusters: u32,
    root: u32,
}
impl Geometry {
    fn parse(source: &mut Source, part: &Partition, cancel: &AtomicBool) -> Result<Self> {
        let header = source.bytes(part.offset, 512)?;
        if &header[3..11] != b"EXFAT   "
            || header[..3] != [0xeb, 0x76, 0x90]
            || header[510..512] != [0x55, 0xaa]
            || header[11..64].iter().any(|n| *n != 0)
        {
            return Err(invalid("invalid boot sector identity"));
        }
        let sector_shift = header[108];
        let cluster_shift = header[109];
        if !(9..=12).contains(&sector_shift) || cluster_shift > 25 - sector_shift {
            return Err(invalid("invalid sector or cluster shift"));
        }
        let sector = 1u64 << sector_shift;
        let cluster_bytes = sector << cluster_shift;
        if part.size < 24 * sector {
            return Err(invalid("truncated boot region"));
        }
        let boot = source.bytes(part.offset, (11 * sector) as usize)?;
        let sum = boot
            .iter()
            .enumerate()
            .filter(|(i, _)| ![106, 107, 112].contains(i))
            .fold(0u32, |sum, (_, v)| {
                sum.rotate_right(1).wrapping_add(u32::from(*v))
            });
        let checksums = source.bytes(part.offset + 11 * sector, sector as usize)?;
        if checksums
            .as_chunks::<4>()
            .0
            .iter()
            .any(|b| u32::from_le_bytes(*b) != sum)
        {
            return Err(invalid(
                "main boot checksum mismatch; metadata recovery requires a valid main boot region",
            ));
        }
        for i in 1..=8 {
            if le32(&boot, ((i + 1) * sector - 4) as usize) != 0xaa55_0000 {
                return Err(invalid("invalid extended boot signature"));
            }
        }
        if le16(&header, 104) != 0x0100 {
            return Err(invalid("only revision 1.00 is implemented"));
        }
        if header[110] != 1 || le16(&header, 106) & 1 != 0 {
            return Err(invalid(
                "TexFAT/two-FAT metadata recovery is not implemented",
            ));
        }
        if le16(&header, 106) & 6 != 0 {
            return Err(invalid("dirty or media-failure volume: allocation state is uncertain; use an image and Deep Scan"));
        }
        if header[112] > 100 && header[112] != 0xff {
            return Err(invalid("invalid PercentInUse field"));
        }
        let volume = le64(&header, 72);
        let fat_sector = u64::from(le32(&header, 80));
        let fat_length = u64::from(le32(&header, 84));
        let heap_sector = u64::from(le32(&header, 88));
        let clusters = le32(&header, 92);
        let root = le32(&header, 96);
        let volume_bytes = volume
            .checked_mul(sector)
            .ok_or_else(|| invalid("volume length overflow"))?;
        let heap_end = heap_sector
            .checked_add(u64::from(clusters) << cluster_shift)
            .ok_or_else(|| invalid("cluster heap overflow"))?;
        if volume_bytes < 1024 * 1024
            || volume_bytes > part.size
            || fat_sector < 24
            || fat_length == 0
            || fat_sector
                .checked_add(fat_length)
                .is_none_or(|n| n > heap_sector)
            || heap_end > volume
            || clusters == 0
            || clusters > 0xffff_fff5
            || ((volume - heap_sector) >> cluster_shift).min(0xffff_fff5) != u64::from(clusters)
            || (u64::from(clusters) + 2) * 4 > fat_length * sector
            || root < 2
            || root > clusters + 1
        {
            return Err(invalid("boot geometry exceeds the declared volume or FAT"));
        }
        let fat = part.offset + fat_sector * sector;
        let heap = part.offset + heap_sector * sector;
        check_cancel(cancel)?;
        let reserved = source.bytes(fat, 8)?;
        if le32(&reserved, 0) & 0xffff_ff00 != 0xffff_ff00 || le32(&reserved, 4) != u32::MAX {
            return Err(invalid("invalid reserved FAT entries"));
        }
        Ok(Self {
            base: part.offset,
            fat,
            heap,
            cluster_bytes,
            clusters,
            root,
        })
    }
    fn offset(&self, cluster: u32) -> Result<u64> {
        if cluster < 2 || cluster > self.clusters + 1 {
            return Err(invalid("cluster outside heap"));
        }
        Ok(self.heap + u64::from(cluster - 2) * self.cluster_bytes)
    }
    fn next(&self, source: &mut Source, cluster: u32) -> Result<u32> {
        self.offset(cluster)?;
        Ok(le32(
            &source.bytes(self.fat + u64::from(cluster) * 4, 4)?,
            0,
        ))
    }
    fn chain(
        &self,
        source: &mut Source,
        first: u32,
        length: Option<u64>,
        contiguous: bool,
        limit: u64,
        cancel: &AtomicBool,
    ) -> Result<Chain> {
        self.offset(first)?;
        let expected = length.map(|n| n.div_ceil(self.cluster_bytes));
        if length.is_some_and(|n| n == 0 || n > limit) {
            return Err(invalid("invalid metadata stream length"));
        }
        if contiguous {
            let count = expected.ok_or_else(|| invalid("contiguous stream lacks length"))?;
            if u64::from(first) + count > u64::from(self.clusters) + 2 {
                return Err(invalid("contiguous metadata stream exceeds heap"));
            }
            let clusters = (0..count).map(|n| first + n as u32).collect();
            return Ok(Chain {
                clusters,
                length: length.expect("known length"),
            });
        }
        let mut clusters = Vec::new();
        let mut seen = HashSet::new();
        let mut current = first;
        loop {
            check_cancel(cancel)?;
            self.offset(current)?;
            if !seen.insert(current) {
                return Err(invalid("metadata FAT chain contains a cycle"));
            }
            if clusters.len() as u64 >= limit.div_ceil(self.cluster_bytes)
                || expected.is_some_and(|n| clusters.len() as u64 >= n)
            {
                return Err(invalid("metadata FAT chain exceeds stream length"));
            }
            clusters.push(current);
            let next = self.next(source, current)?;
            if next == u32::MAX {
                break;
            }
            current = next;
        }
        if expected.is_some_and(|n| n != clusters.len() as u64) {
            return Err(invalid("metadata FAT chain is shorter than its stream"));
        }
        let length = length.unwrap_or(clusters.len() as u64 * self.cluster_bytes);
        Ok(Chain { clusters, length })
    }
}
struct Chain {
    clusters: Vec<u32>,
    length: u64,
}
impl Chain {
    fn read(
        &self,
        source: &mut Source,
        geo: &Geometry,
        offset: u64,
        bytes: &mut [u8],
    ) -> Result<()> {
        if offset
            .checked_add(bytes.len() as u64)
            .is_none_or(|n| n > self.length)
        {
            return Err(invalid("metadata read exceeds stream"));
        }
        let mut copied = 0;
        while copied < bytes.len() {
            let pos = offset + copied as u64;
            let cluster = self.clusters[(pos / geo.cluster_bytes) as usize];
            let inside = pos % geo.cluster_bytes;
            let count = (geo.cluster_bytes - inside).min((bytes.len() - copied) as u64) as usize;
            source.read(
                geo.offset(cluster)? + inside,
                &mut bytes[copied..copied + count],
            )?;
            copied += count;
        }
        Ok(())
    }
}
struct Bitmap {
    chain: Chain,
    cached_start: u64,
    cache: Vec<u8>,
}
impl Bitmap {
    fn allocated(
        &mut self,
        source: &mut Source,
        geo: &Geometry,
        cluster: u32,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        check_cancel(cancel)?;
        geo.offset(cluster)?;
        let bit = u64::from(cluster - 2);
        let byte = bit / 8;
        if self.cache.is_empty()
            || byte < self.cached_start
            || byte >= self.cached_start + self.cache.len() as u64
        {
            self.cached_start = byte / PAGE * PAGE;
            self.cache.resize(
                (self.chain.length - self.cached_start).min(PAGE) as usize,
                0,
            );
            self.chain
                .read(source, geo, self.cached_start, &mut self.cache)?;
        }
        Ok(self.cache[(byte - self.cached_start) as usize] & (1 << (bit % 8)) != 0)
    }
    fn verify_metadata(
        &mut self,
        source: &mut Source,
        geo: &Geometry,
        chain: &Chain,
        cancel: &AtomicBool,
    ) -> Result<()> {
        for cluster in &chain.clusters {
            if !self.allocated(source, geo, *cluster, cancel)? {
                return Err(invalid(
                    "allocation bitmap marks a live metadata cluster as free",
                ));
            }
        }
        Ok(())
    }
}

struct FileSet {
    name: String,
    first: u32,
    length: u64,
    valid: u64,
    contiguous: bool,
    directory: bool,
    deleted: bool,
}
fn file_set(records: &[[u8; 32]]) -> Result<FileSet> {
    if records.len() < 3 || records.len() > 19 {
        return Err(invalid("unsupported entry-set shape"));
    }
    let deleted = records[0][0] == 0x05;
    let expected_primary = if deleted { 0x05 } else { 0x85 };
    let expected_stream = if deleted { 0x40 } else { 0xc0 };
    let expected_name = if deleted { 0x41 } else { 0xc1 };
    if records[0][0] != expected_primary
        || usize::from(records[0][1]) + 1 != records.len()
        || records[1][0] != expected_stream
        || records[2..]
            .iter()
            .any(|r| r[0] != expected_name || r[1] != 0)
    {
        return Err(invalid("mixed or unsupported entry-set records"));
    }
    let mut checksum = 0u16;
    for (ri, r) in records.iter().enumerate() {
        for (i, b) in r.iter().enumerate() {
            if ri == 0 && [2, 3].contains(&i) {
                continue;
            }
            // Normal deletion clears InUse; accept only a checksum matching the
            // reconstructed formerly-active set. Never alter bytes on the source.
            let byte = if deleted && i == 0 { *b | 0x80 } else { *b };
            checksum = checksum.rotate_right(1).wrapping_add(u16::from(byte));
        }
    }
    if checksum != le16(&records[0], 2) {
        return Err(invalid("entry-set checksum mismatch"));
    }
    let stream = &records[1];
    let names = usize::from(stream[3]);
    if names == 0 || names.div_ceil(15) != records.len() - 2 || ![1, 3].contains(&stream[1]) {
        return Err(invalid("invalid name length or stream flags"));
    }
    let mut utf16 = Vec::with_capacity(names);
    for record in &records[2..] {
        for value in record[2..].as_chunks::<2>().0 {
            if utf16.len() < names {
                utf16.push(u16::from_le_bytes(*value));
            }
        }
    }
    let name = String::from_utf16(&utf16).map_err(|_| invalid("invalid UTF-16 filename"))?;
    if name == "." || name == ".." || name.chars().any(|c| c < ' ' || "\"*/:<>?\\|".contains(c)) {
        return Err(invalid("invalid filename characters"));
    }
    let length = le64(stream, 24);
    let valid = le64(stream, 8);
    let first = le32(stream, 20);
    let directory = le16(&records[0], 4) & 0x10 != 0;
    if valid > length
        || (first == 0 && length != 0)
        || (stream[1] & 2 != 0 && (length == 0 || first < 2))
    {
        return Err(invalid("invalid file allocation fields"));
    }
    Ok(FileSet {
        name,
        first,
        length,
        valid,
        contiguous: stream[1] & 2 != 0,
        directory,
        deleted,
    })
}

fn root_bitmap(
    source: &mut Source,
    geo: &Geometry,
    root: &Chain,
    cancel: &AtomicBool,
) -> Result<Bitmap> {
    let mut offset = 0;
    let mut selected = None;
    let mut ended = false;
    while offset < root.length && !ended {
        check_cancel(cancel)?;
        let mut block = vec![0u8; (root.length - offset).min(PAGE) as usize];
        root.read(source, geo, offset, &mut block)?;
        for r in block.as_chunks::<32>().0 {
            if r[0] == 0 {
                ended = true;
                break;
            }
            if r[0] != 0x81 {
                continue;
            }
            if selected.is_some() || r[1] & 1 != 0 {
                return Err(invalid(
                    "missing, duplicate or unsupported allocation bitmap",
                ));
            }
            let length = le64(r, 24);
            if length != u64::from(geo.clusters).div_ceil(8) {
                return Err(invalid(
                    "allocation bitmap length does not match cluster count",
                ));
            }
            selected = Some((le32(r, 20), length));
        }
        offset += block.len() as u64;
    }
    let (first, length) =
        selected.ok_or_else(|| invalid("no active allocation bitmap in root directory"))?;
    let chain = geo.chain(
        source,
        first,
        Some(length),
        false,
        u64::from(geo.clusters).div_ceil(8),
        cancel,
    )?;
    let mut bitmap = Bitmap {
        chain,
        cached_start: 0,
        cache: vec![],
    };
    // Check the bitmap's own chain without cloning its potentially long index.
    for i in 0..bitmap.chain.clusters.len() {
        let cluster = bitmap.chain.clusters[i];
        if !bitmap.allocated(source, geo, cluster, cancel)? {
            return Err(invalid("allocation bitmap marks itself free"));
        }
    }
    bitmap.verify_metadata(source, geo, root, cancel)?;
    Ok(bitmap)
}

fn file_extents(
    source: &mut Source,
    geo: &Geometry,
    bitmap: &mut Bitmap,
    file: &FileSet,
    deleted: bool,
    cancel: &AtomicBool,
) -> Result<Vec<Extent>> {
    if file.valid != file.length {
        return Err(invalid("partly initialized streams are unsupported"));
    }
    if file.length == 0 {
        if file.first != 0 {
            return Err(invalid("zero-length stream has a cluster"));
        }
        return Ok(vec![]);
    }
    let count = file.length.div_ceil(geo.cluster_bytes);
    if count > u64::from(geo.clusters) || (deleted && !file.contiguous) {
        return Err(invalid(
            "stream exceeds heap or deleted allocation chain is unavailable",
        ));
    }
    if file.contiguous && u64::from(file.first) + count > u64::from(geo.clusters) + 2 {
        return Err(invalid("contiguous stream exceeds heap"));
    }
    let mut extents: Vec<Extent> = Vec::new();
    let mut cluster = file.first;
    let mut remaining = file.length;
    let mut anchor = cluster;
    let mut power = 1u64;
    let mut distance = 0u64;
    for step in 0..count {
        check_cancel(cancel)?;
        let offset = geo.offset(cluster)?;
        if bitmap.allocated(source, geo, cluster, cancel)? == deleted {
            return Err(invalid(if deleted {
                "deleted stream is currently allocated"
            } else {
                "existing stream has a free allocation-bitmap cluster"
            }));
        }
        let length = remaining.min(geo.cluster_bytes);
        if let Some(previous) = extents.last_mut().filter(|e| e.offset + e.length == offset) {
            previous.length += length;
        } else {
            if extents.len() >= 1_000_000 {
                return Err(invalid(
                    "stream exceeds the one-million-extent safety limit",
                ));
            }
            extents.push(Extent { offset, length });
        }
        remaining -= length;
        if file.contiguous {
            cluster += 1;
        } else {
            let next = geo.next(source, cluster)?;
            if step + 1 == count {
                if next != u32::MAX {
                    return Err(invalid(
                        "file FAT chain exceeds its stream length or contains a cycle",
                    ));
                }
            } else {
                geo.offset(next)?;
                distance += 1;
                if next == anchor {
                    return Err(invalid("file FAT chain contains a cycle"));
                }
                if distance == power {
                    anchor = next;
                    power *= 2;
                    distance = 0;
                }
                cluster = next;
            }
        }
    }
    Ok(extents)
}

struct Directory {
    first: u32,
    length: Option<u64>,
    contiguous: bool,
    path: String,
    deleted: bool,
}

pub(crate) fn scan_deleted(
    source: &mut Source,
    part: &Partition,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64, usize),
    on_entry: &mut impl FnMut(&Entry),
) -> Result<(Vec<Entry>, Vec<String>)> {
    let geo = Geometry::parse(source, part, cancel)?;
    let root = geo.chain(source, geo.root, None, false, MAX_DIRECTORY, cancel)?;
    let mut bitmap = root_bitmap(source, &geo, &root, cancel)?;
    let mut queue = VecDeque::from([Directory {
        first: geo.root,
        length: None,
        contiguous: false,
        path: String::new(),
        deleted: false,
    }]);
    let mut visited = HashSet::new();
    let mut entries = Vec::new();
    let mut notices = Vec::new();
    let mut rejected = 0u64;
    let mut malformed = 0u64;
    while let Some(Directory {
        first,
        length,
        contiguous,
        path,
        deleted: inherited_deleted,
    }) = queue.pop_front()
    {
        check_cancel(cancel)?;
        let directory = format!("/{path}");
        if visited.contains(&first) {
            notices.push(format!("Directory {directory} skipped: its first cluster was already visited (alias or cycle)."));
            continue;
        }
        let chain = match geo.chain(source, first, length, contiguous, MAX_DIRECTORY, cancel) {
            Ok(chain) => chain,
            Err(Error::Invalid(s)) => {
                notices.push(format!("Directory {directory} skipped: {s}"));
                continue;
            }
            Err(e) => return Err(e),
        };
        let mut allocation_valid = true;
        for cluster in &chain.clusters {
            if bitmap.allocated(source, &geo, *cluster, cancel)? == inherited_deleted {
                allocation_valid = false;
                break;
            }
        }
        if !allocation_valid {
            notices.push(format!(
                "Directory {directory} skipped: allocation bitmap {}.",
                if inherited_deleted {
                    "marks a deleted directory cluster currently allocated"
                } else {
                    "marks a live metadata cluster as free"
                }
            ));
            continue;
        }
        // Rejected aliases with invalid lengths/chains/bitmap state must not
        // prevent a later independent, valid reference to the same first cluster.
        visited.insert(first);
        if inherited_deleted {
            notices.push(format!("Deleted directory {directory}: intact contiguous length and free clusters were verified; surviving records and original ancestry remain tentative."));
        }
        let mut offset = 0;
        let mut pending = Vec::<[u8; 32]>::new();
        let mut entry_offset = 0;
        let mut expected = 0usize;
        let mut ended = false;
        while offset < chain.length && !ended {
            check_cancel(cancel)?;
            let mut block = vec![0u8; (chain.length - offset).min(PAGE) as usize];
            chain.read(source, &geo, offset, &mut block)?;
            for (index, r) in block.as_chunks::<32>().0.iter().enumerate() {
                check_cancel(cancel)?;
                if r[0] == 0 {
                    ended = true;
                    if !pending.is_empty() {
                        malformed += 1;
                    }
                    break;
                }
                if pending.is_empty() {
                    if r[0] != 0x05 && r[0] != 0x85 {
                        continue;
                    }
                    if !(2..=18).contains(&r[1]) {
                        malformed += 1;
                        continue;
                    }
                    expected = usize::from(r[1]) + 1;
                    let logical = offset + (index * 32) as u64;
                    entry_offset = geo
                        .offset(chain.clusters[(logical / geo.cluster_bytes) as usize])?
                        + logical % geo.cluster_bytes;
                    pending.push(*r);
                    continue;
                }
                pending.push(*r);
                if pending.len() != expected {
                    continue;
                }
                let candidate = file_set(&pending);
                pending.clear();
                let file = match candidate {
                    Ok(file) => file,
                    Err(_) => {
                        malformed += 1;
                        continue;
                    }
                };
                let deleted = file.deleted || inherited_deleted;
                if file.directory {
                    if file.length > 0
                        && file.length <= MAX_DIRECTORY
                        && file.valid == file.length
                        && file.length.is_multiple_of(geo.cluster_bytes)
                        && (!deleted || file.contiguous)
                    {
                        let next = Directory {
                            first: file.first,
                            length: Some(file.length),
                            contiguous: file.contiguous,
                            path: format!("{path}{}/", file.name),
                            deleted,
                        };
                        if deleted {
                            queue.push_back(next);
                        } else {
                            queue.push_front(next);
                        }
                    } else {
                        notices.push(format!("Directory {directory}{} skipped: invalid/unsupported length, partial initialization, or deleted FAT-chain allocation.", file.name));
                    }
                    continue;
                }
                let extents = match file_extents(source, &geo, &mut bitmap, &file, deleted, cancel)
                {
                    Ok(extents) => extents,
                    Err(Error::Invalid(reason)) => {
                        notices.push(format!("exFAT record {directory}{} at byte {entry_offset} skipped: {reason}. Other independent records were retained.", file.name));
                        rejected += 1;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let kind = file
                    .name
                    .rsplit_once('.')
                    .map_or("unknown", |(_, kind)| kind)
                    .to_ascii_lowercase();
                let mut notes = vec![if file.deleted {
                    format!("Inactive exFAT entry set in {directory}; checksum matches when former InUse bits are reconstructed.")
                } else {
                    format!("Active exFAT entry set in {directory}; entry checksum and declared stream allocation were validated.")
                }];
                if deleted {
                    notes.push("NoFatChain stream with valid length equal to data length; the current allocation bitmap marks all content clusters free. Inactive records are not authoritative metadata; freed clusters may have been overwritten.".into());
                } else {
                    notes.push("All content clusters are currently allocated; the complete FAT chain or NoFatChain range matches the declared length. Content is not decoded; a changing mounted filesystem can invalidate this snapshot.".into());
                }
                if inherited_deleted {
                    notes.push("The parent directory is deleted; surviving child status and original ancestry are inferred and tentative.".into());
                }
                notes.push(
                    "NameHash is not independently checked against the up-case table.".into(),
                );
                let entry = Entry {
                    id: format!("exfat-{:x}-{entry_offset:x}", geo.base),
                    relative_path: Some(format!("{path}{}", file.name)),
                    partition: Some(part.name.clone()),
                    deletion_status: if deleted { "deleted" } else { "existing" }.into(),
                    name: file.name,
                    kind,
                    size: file.length,
                    offset: extents.first().map_or(entry_offset, |e| e.offset),
                    confidence: if deleted { "low" } else { "high" }.into(),
                    method: if deleted {
                        "exfat-undelete"
                    } else {
                        "exfat-metadata"
                    }
                    .into(),
                    notes,
                    extents,
                };
                on_entry(&entry);
                entries.push(entry);
            }
            progress(block.len() as u64, entries.len());
            check_cancel(cancel)?;
            offset += block.len() as u64;
        }
        if !ended && !pending.is_empty() {
            malformed += 1;
        }
    }
    if rejected > 0 {
        notices.push(format!("{rejected} exFAT streams skipped: unsupported, partly initialized, outside the heap, invalid allocation chains, or allocation-bitmap conflicts (deleted content currently allocated or live content free)."));
    }
    if malformed > 0 {
        notices.push(format!("{malformed} exFAT entry sets skipped because their shape, checksum, or fields were invalid/unsupported."));
    }
    Ok((entries, notices))
}
