//! Independent classic MP4/QuickTime carving from Apple's public QTFF atom,
//! sample-table and data-reference documentation, and the MP4 Registration
//! Authority brand registry. No proprietary recovery implementation is used.
//! https://developer.apple.com/documentation/quicktime-file-format/sample_table_atom
//! https://developer.apple.com/documentation/quicktime-file-format/chunk_offset_atom
//! https://mp4ra.org/registered-types/brands
//!
//! Resource limits apply to metadata/work, not media byte length. Table payloads
//! are streamed through 64 KiB buffers. Offsets and extended atom sizes are u64.
use crate::{check_cancel, Error, Result, Source};
use std::sync::atomic::AtomicBool;

pub(crate) const METHOD_NOTE: &str = "MP4/MOV carving supports contiguous classic files with ftyp, intact moov sample tables and explicit mdat lengths, including 64-bit offsets. It does not reconstruct fragmented storage, fragmented MP4, missing moov, encrypted media or older MOV without ftyp. Sample bytes are not decoded; stale or overwritten content can pass structural checks.";
const MAX_METADATA: u64 = 512 * 1024 * 1024;
const MAX_BOXES: usize = 100_000;
const MAX_TRACKS: usize = 64;
const MAX_MDAT: usize = 256;
const MAX_SAMPLES: u32 = 100_000_000;
const MAX_CHUNKS: u32 = 20_000_000;
const MAX_RUNS: u32 = 1_000_000;
const PAGE: usize = 64 * 1024;

fn invalid(message: &str) -> Error {
    Error::Invalid(format!("MP4/MOV: {message}"))
}
fn be16(b: &[u8], p: usize) -> u16 {
    u16::from_be_bytes([b[p], b[p + 1]])
}
fn be32(b: &[u8], p: usize) -> u32 {
    u32::from_be_bytes(b[p..p + 4].try_into().expect("bounded field"))
}
fn be64(b: &[u8], p: usize) -> u64 {
    u64::from_be_bytes(b[p..p + 8].try_into().expect("bounded field"))
}
#[derive(Clone, Copy)]
struct Atom {
    kind: [u8; 4],
    payload: u64,
    end: u64,
}
impl Atom {
    fn len(self) -> u64 {
        self.end - self.payload
    }
}
struct Parser<'a, 's> {
    source: &'a mut Source<'s>,
    cancel: &'a AtomicBool,
    start: u64,
    end: u64,
    boxes: usize,
}
impl Parser<'_, '_> {
    fn read(&mut self, at: u64, len: usize, bound: u64) -> Result<Vec<u8>> {
        check_cancel(self.cancel)?;
        if at < self.start
            || at
                .checked_add(len as u64)
                .is_none_or(|n| n > bound || n > self.end)
        {
            return Err(invalid("Truncated or overflowing atom/table"));
        }
        let bytes = self.source.bytes(at, len)?;
        check_cancel(self.cancel)?;
        Ok(bytes)
    }
    fn atom(&mut self, at: u64, bound: u64) -> Result<Atom> {
        self.boxes += 1;
        if self.boxes > MAX_BOXES {
            return Err(invalid("Metadata atom count exceeds the safety limit"));
        }
        let h = self.read(at, 8, bound)?;
        let short = be32(&h, 0);
        let (size, header) = match short {
            0 => {
                return Err(invalid(
                    "Unbounded size-zero atoms cannot establish a carved file end",
                ))
            }
            1 => (be64(&self.read(at + 8, 8, bound)?, 0), 16),
            n => (u64::from(n), 8),
        };
        if size < header {
            return Err(invalid("Atom size is smaller than its header"));
        }
        let end = at
            .checked_add(size)
            .filter(|n| *n <= bound)
            .ok_or_else(|| invalid("Atom extends beyond the available bytes"))?;
        Ok(Atom {
            kind: h[4..8].try_into().expect("four-byte type"),
            payload: at + header,
            end,
        })
    }
    fn children(&mut self, parent: Atom) -> Result<Vec<Atom>> {
        let mut out = Vec::new();
        let mut at = parent.payload;
        while at < parent.end {
            let atom = self.atom(at, parent.end)?;
            at = atom.end;
            out.push(atom);
        }
        Ok(out)
    }
    fn full(&mut self, atom: Atom, size: usize, versions: &[u8]) -> Result<Vec<u8>> {
        let b = self.read(atom.payload, size, atom.end)?;
        if !versions.contains(&b[0]) || b[1..4] != [0, 0, 0] {
            return Err(invalid("Unsupported full-atom version or flags"));
        }
        Ok(b)
    }
}
fn one(atoms: &[Atom], kind: &[u8; 4]) -> Result<Atom> {
    optional(atoms, kind)?.ok_or_else(|| invalid("A required media atom is missing"))
}
fn optional(atoms: &[Atom], kind: &[u8; 4]) -> Result<Option<Atom>> {
    let mut matches = atoms.iter().filter(|a| &a.kind == kind);
    let first = matches.next().copied();
    if matches.next().is_some() {
        return Err(invalid("Duplicate singleton media atom"));
    }
    Ok(first)
}
fn iso_brand(brand: &[u8]) -> bool {
    matches!(
        brand,
        b"isom"
            | b"iso2"
            | b"iso3"
            | b"iso4"
            | b"iso5"
            | b"iso6"
            | b"iso7"
            | b"iso8"
            | b"iso9"
            | b"mp41"
            | b"mp42"
            | b"avc1"
            | b"M4V "
            | b"M4VH"
            | b"M4VP"
            | b"MSNV"
    )
}
fn file_kind(p: &mut Parser<'_, '_>) -> Result<(&'static str, Atom)> {
    let ftyp = p.atom(p.start, p.end)?;
    if ftyp.kind != *b"ftyp" || ftyp.len() < 8 || ftyp.len() > 4096 || !ftyp.len().is_multiple_of(4)
    {
        return Err(invalid("Invalid file-type atom"));
    }
    let b = p.read(ftyp.payload, ftyp.len() as usize, ftyp.end)?;
    let kind = if &b[..4] == b"qt  " {
        "mov"
    } else if iso_brand(&b[..4])
        || b[8..]
            .as_chunks::<4>()
            .0
            .iter()
            .any(|brand| iso_brand(brand))
    {
        "mp4"
    } else {
        return Err(invalid("Unrecognised MP4/MOV brand"));
    };
    Ok((kind, ftyp))
}
/// Only the bounded ftyp is read before the scanner applies the format filter.
pub(crate) fn classify(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<&'static str> {
    file_kind(&mut Parser {
        source,
        cancel,
        start,
        end,
        boxes: 0,
    })
    .map(|v| v.0)
}

/// Sequential bounded table cursor. Cancellation is checked on each refill;
/// callers also check between chunks even when all table pages remain cached.
struct Stream {
    next: u64,
    end: u64,
    buffer: Vec<u8>,
    index: usize,
}
impl Stream {
    fn new(next: u64, end: u64) -> Self {
        Self {
            next,
            end,
            buffer: Vec::new(),
            index: 0,
        }
    }
    fn byte(&mut self, p: &mut Parser<'_, '_>) -> Result<u8> {
        if self.index == self.buffer.len() {
            if self.next >= self.end {
                return Err(invalid("Sample table ran out of entries"));
            }
            let count = (self.end - self.next).min(PAGE as u64) as usize;
            self.buffer = p.read(self.next, count, self.end)?;
            self.next += count as u64;
            self.index = 0;
        }
        let byte = self.buffer[self.index];
        self.index += 1;
        Ok(byte)
    }
    fn u32(&mut self, p: &mut Parser<'_, '_>) -> Result<u32> {
        let mut bytes = [0; 4];
        for b in &mut bytes {
            *b = self.byte(p)?;
        }
        Ok(u32::from_be_bytes(bytes))
    }
    fn u64(&mut self, p: &mut Parser<'_, '_>) -> Result<u64> {
        let mut bytes = [0; 8];
        for b in &mut bytes {
            *b = self.byte(p)?;
        }
        Ok(u64::from_be_bytes(bytes))
    }
}
#[derive(Clone, Copy)]
struct Sizes {
    table: Atom,
    count: u32,
    fixed: u32,
    bits: u8,
}
#[derive(Clone, Copy)]
struct Track {
    video: bool,
    sizes: Sizes,
    chunks: Atom,
    chunk_count: u32,
    runs: Atom,
    run_count: u32,
    descriptions: u32,
}
fn counted(p: &mut Parser<'_, '_>, atom: Atom, width: u64, limit: u32) -> Result<u32> {
    let b = p.full(atom, 8, &[0])?;
    let count = be32(&b, 4);
    if count > limit || atom.len() != 8 + u64::from(count) * width {
        return Err(invalid(
            "Table count/length is invalid or exceeds the safety limit",
        ));
    }
    Ok(count)
}
fn sizes(p: &mut Parser<'_, '_>, atoms: &[Atom]) -> Result<Sizes> {
    match (optional(atoms, b"stsz")?, optional(atoms, b"stz2")?) {
        (Some(table), None) => {
            let b = p.full(table, 12, &[0])?;
            let fixed = be32(&b, 4);
            let count = be32(&b, 8);
            let expected = 12 + if fixed == 0 { u64::from(count) * 4 } else { 0 };
            if count > MAX_SAMPLES || table.len() != expected {
                return Err(invalid("Invalid sample-size table"));
            }
            Ok(Sizes {
                table,
                count,
                fixed,
                bits: 32,
            })
        }
        (None, Some(table)) => {
            let b = p.full(table, 12, &[0])?;
            let bits = b[7];
            let count = be32(&b, 8);
            if b[4..7] != [0, 0, 0]
                || ![4, 8, 16].contains(&bits)
                || count > MAX_SAMPLES
                || table.len() != 12 + (u64::from(count) * u64::from(bits)).div_ceil(8)
            {
                return Err(invalid("Invalid compact sample-size table"));
            }
            Ok(Sizes {
                table,
                count,
                fixed: 0,
                bits,
            })
        }
        _ => Err(invalid("Exactly one sample-size table is required")),
    }
}
fn timing(p: &mut Parser<'_, '_>, atom: Atom, samples: u32, composition: bool) -> Result<()> {
    let header = p.full(atom, 8, if composition { &[0, 1] } else { &[0] })?;
    let count = be32(&header, 4);
    if count > MAX_RUNS || atom.len() != 8 + u64::from(count) * 8 {
        return Err(invalid("Invalid sample timing table"));
    }
    let mut stream = Stream::new(atom.payload + 8, atom.end);
    let mut total = 0u64;
    let mut duration = 0u64;
    for _ in 0..count {
        let n = stream.u32(p)?;
        let delta = stream.u32(p)?;
        if n == 0 {
            return Err(invalid("Empty sample timing run"));
        }
        total += u64::from(n);
        if total > u64::from(samples) {
            return Err(invalid("Timing count exceeds sample count"));
        }
        if !composition {
            duration = duration
                .checked_add(u64::from(n) * u64::from(delta))
                .ok_or_else(|| invalid("Timing duration overflow"))?;
        }
    }
    if total != u64::from(samples) || (!composition && samples > 0 && duration == 0) {
        return Err(invalid("Sample timing and size counts disagree"));
    }
    Ok(())
}
fn nal_lengths(p: &mut Parser<'_, '_>, mut at: u64, count: u32, end: u64) -> Result<u64> {
    for _ in 0..count {
        let size = u64::from(be16(&p.read(at, 2, end)?, 0));
        at += 2;
        if size == 0 {
            return Err(invalid("Empty decoder-configuration NAL unit"));
        }
        at = at
            .checked_add(size)
            .filter(|n| *n <= end)
            .ok_or_else(|| invalid("Decoder-configuration NAL unit exceeds its atom"))?;
    }
    Ok(at)
}
fn visual_extensions(p: &mut Parser<'_, '_>, entry: Atom) -> Result<()> {
    let mut at = entry.payload + 78;
    let mut extensions = Vec::new();
    while at < entry.end {
        // QTFF explicitly permits precisely four terminating zero bytes.
        if entry.end - at == 4 && p.read(at, 4, entry.end)? == [0; 4] {
            break;
        }
        let atom = p.atom(at, entry.end)?;
        if atom.kind == *b"sinf" {
            return Err(invalid(
                "Protected visual sample descriptions are unsupported",
            ));
        }
        at = atom.end;
        extensions.push(atom);
    }
    if matches!(&entry.kind, b"avc1" | b"avc3") {
        let config = one(&extensions, b"avcC")?;
        let h = p.read(config.payload, 6, config.end)?;
        if h[0] != 1 || h[4] & 0xfc != 0xfc || h[4] & 3 == 2 || h[5] & 0xe0 != 0xe0 {
            return Err(invalid("Invalid AVC decoder-configuration header"));
        }
        let sps = u32::from(h[5] & 31);
        let mut at = nal_lengths(p, config.payload + 6, sps, config.end)?;
        let pps = u32::from(p.read(at, 1, config.end)?[0]);
        at = nal_lengths(p, at + 1, pps, config.end)?;
        if entry.kind == *b"avc1" && (sps == 0 || pps == 0) {
            return Err(invalid("AVC configuration has no parameter sets"));
        }
        if at < config.end {
            let extra = p.read(at, 4, config.end)?;
            if extra[0] & 0xfc != 0xfc || extra[1] & 0xf8 != 0xf8 || extra[2] & 0xf8 != 0xf8 {
                return Err(invalid("Invalid AVC configuration extension"));
            }
            at = nal_lengths(p, at + 4, u32::from(extra[3]), config.end)?;
        }
        if at != config.end {
            return Err(invalid("Trailing malformed AVC configuration bytes"));
        }
    } else if matches!(&entry.kind, b"hvc1" | b"hev1") {
        let config = one(&extensions, b"hvcC")?;
        let h = p.read(config.payload, 23, config.end)?;
        if h[0] != 1 || (entry.kind == *b"hvc1" && h[22] == 0) {
            return Err(invalid("Invalid HEVC decoder-configuration header"));
        }
        let mut at = config.payload + 23;
        for _ in 0..h[22] {
            let array = p.read(at, 3, config.end)?;
            at = nal_lengths(p, at + 3, u32::from(be16(&array, 1)), config.end)?;
        }
        if at != config.end {
            return Err(invalid("Trailing malformed HEVC configuration bytes"));
        }
    }
    Ok(())
}
fn descriptions(p: &mut Parser<'_, '_>, stsd: Atom, dref: Atom, video: bool) -> Result<u32> {
    let d = p.full(dref, 8, &[0])?;
    let references = be32(&d, 4);
    if references == 0 || references > 64 {
        return Err(invalid("Invalid data-reference count"));
    }
    let mut at = dref.payload + 8;
    for _ in 0..references {
        let entry = p.atom(at, dref.end)?;
        let flags = p.read(entry.payload, 4, entry.end)?;
        if !matches!(&entry.kind, b"url " | b"alis") || be32(&flags, 0) != 1 {
            return Err(invalid("External or unsupported media data references"));
        }
        at = entry.end;
    }
    if at != dref.end {
        return Err(invalid("Data-reference count/length mismatch"));
    }
    let b = p.full(stsd, 8, &[0])?;
    let count = be32(&b, 4);
    if count == 0 || count > 64 {
        return Err(invalid("Invalid sample-description count"));
    }
    at = stsd.payload + 8;
    for _ in 0..count {
        let entry = p.atom(at, stsd.end)?;
        if matches!(&entry.kind, b"encv" | b"enca" | b"drms" | b"drmi") {
            return Err(invalid("Encrypted sample entries are unsupported"));
        }
        let b = p.read(entry.payload, if video { 28 } else { 8 }, entry.end)?;
        let reference = u32::from(be16(&b, 6));
        if reference == 0 || reference > references || b[..6] != [0; 6] {
            return Err(invalid("Invalid sample data-reference index"));
        }
        if video && (entry.len() < 78 || be16(&b, 24) == 0 || be16(&b, 26) == 0) {
            return Err(invalid("Invalid visual sample description"));
        }
        if video {
            visual_extensions(p, entry)?;
        }
        at = entry.end;
    }
    if at != stsd.end {
        return Err(invalid("Sample-description count/length mismatch"));
    }
    Ok(count)
}
fn movie(p: &mut Parser<'_, '_>, moov: Atom) -> Result<Vec<Track>> {
    if moov.len() > MAX_METADATA {
        return Err(invalid("Movie metadata exceeds the safety limit"));
    }
    let atoms = p.children(moov)?;
    if atoms.iter().any(|a| matches!(&a.kind, b"mvex" | b"cmov")) {
        return Err(invalid(
            "Fragmented or compressed movie metadata is unsupported",
        ));
    }
    let mvhd = one(&atoms, b"mvhd")?;
    let b = p.full(mvhd, 24, &[0, 1])?;
    if mvhd.len() < if b[0] == 0 { 100 } else { 112 }
        || be32(&b, if b[0] == 0 { 12 } else { 20 }) == 0
    {
        return Err(invalid("Invalid movie timescale/header"));
    }
    let mut tracks = Vec::new();
    let mut ids = Vec::new();
    for trak in atoms.iter().filter(|a| a.kind == *b"trak") {
        if tracks.len() == MAX_TRACKS {
            return Err(invalid("Track count exceeds the safety limit"));
        }
        let track_atoms = p.children(*trak)?;
        let tkhd = one(&track_atoms, b"tkhd")?;
        let h = p.read(tkhd.payload, 24, tkhd.end)?;
        if h[0] > 1 || tkhd.len() < if h[0] == 0 { 84 } else { 96 } {
            return Err(invalid("Invalid track header"));
        }
        let id = be32(&h, if h[0] == 0 { 12 } else { 20 });
        if id == 0 || ids.contains(&id) {
            return Err(invalid("Invalid or duplicate track ID"));
        }
        ids.push(id);
        let media = p.children(one(&track_atoms, b"mdia")?)?;
        let mdhd = one(&media, b"mdhd")?;
        let h = p.full(mdhd, 24, &[0, 1])?;
        if mdhd.len() < if h[0] == 0 { 24 } else { 36 }
            || be32(&h, if h[0] == 0 { 12 } else { 20 }) == 0
        {
            return Err(invalid("Invalid media timescale/header"));
        }
        let h = p.full(one(&media, b"hdlr")?, 24, &[0])?;
        let video = &h[8..12] == b"vide";
        let minf = p.children(one(&media, b"minf")?)?;
        let dinf = p.children(one(&minf, b"dinf")?)?;
        let table = p.children(one(&minf, b"stbl")?)?;
        if table
            .iter()
            .any(|a| matches!(&a.kind, b"senc" | b"saiz" | b"saio"))
        {
            return Err(invalid(
                "Auxiliary/encrypted sample layouts are unsupported",
            ));
        }
        let descriptions = descriptions(p, one(&table, b"stsd")?, one(&dinf, b"dref")?, video)?;
        let sizes = sizes(p, &table)?;
        timing(p, one(&table, b"stts")?, sizes.count, false)?;
        if let Some(ctts) = optional(&table, b"ctts")? {
            timing(p, ctts, sizes.count, true)?;
        }
        if let Some(stss) = optional(&table, b"stss")? {
            let count = counted(p, stss, 4, MAX_SAMPLES)?;
            let mut stream = Stream::new(stss.payload + 8, stss.end);
            let mut previous = 0;
            for _ in 0..count {
                let n = stream.u32(p)?;
                if n <= previous || n > sizes.count {
                    return Err(invalid("Invalid sync-sample index"));
                }
                previous = n;
            }
        }
        let chunks = match (optional(&table, b"stco")?, optional(&table, b"co64")?) {
            (Some(a), None) | (None, Some(a)) => a,
            _ => return Err(invalid("Exactly one chunk-offset table is required")),
        };
        let chunk_count = counted(
            p,
            chunks,
            if chunks.kind == *b"co64" { 8 } else { 4 },
            MAX_CHUNKS,
        )?;
        let runs = one(&table, b"stsc")?;
        let run_count = counted(p, runs, 12, MAX_RUNS)?;
        if (sizes.count == 0) != (chunk_count == 0) || (chunk_count == 0) != (run_count == 0) {
            return Err(invalid("Empty and nonempty sample tables disagree"));
        }
        tracks.push(Track {
            video,
            sizes,
            chunks,
            chunk_count,
            runs,
            run_count,
            descriptions,
        });
    }
    if !tracks.iter().any(|t| t.video && t.sizes.count > 0) {
        return Err(invalid("No nonempty video track"));
    }
    Ok(tracks)
}
#[derive(Clone, Copy)]
struct Run {
    first: u32,
    samples: u32,
    description: u32,
}
fn run(p: &mut Parser<'_, '_>, stream: &mut Stream, chunks: u32, descriptions: u32) -> Result<Run> {
    let row = Run {
        first: stream.u32(p)?,
        samples: stream.u32(p)?,
        description: stream.u32(p)?,
    };
    if row.first == 0
        || row.first > chunks
        || row.samples == 0
        || row.description == 0
        || row.description > descriptions
    {
        return Err(invalid("Invalid sample-to-chunk row"));
    }
    Ok(row)
}
fn validate_tracks(
    p: &mut Parser<'_, '_>,
    tracks: &[Track],
    mdats: Option<&[Atom]>,
) -> Result<u64> {
    let mut maximum_end = p.start;
    for track in tracks {
        if track.chunk_count == 0 {
            continue;
        }
        let mut offsets = Stream::new(track.chunks.payload + 8, track.chunks.end);
        let mut runs = Stream::new(track.runs.payload + 8, track.runs.end);
        let mut sizes = Stream::new(track.sizes.table.payload + 12, track.sizes.table.end);
        let mut current = run(p, &mut runs, track.chunk_count, track.descriptions)?;
        if current.first != 1 {
            return Err(invalid(
                "Sample-to-chunk mapping does not begin at chunk one",
            ));
        }
        let mut rows_read = 1;
        let mut next = if rows_read < track.run_count {
            rows_read += 1;
            Some(run(p, &mut runs, track.chunk_count, track.descriptions)?)
        } else {
            None
        };
        let mut samples_read = 0u64;
        let mut previous_end = p.start;
        let mut nibble = 0u8;
        let mut track_bytes = 0u64;
        for chunk in 1..=track.chunk_count {
            check_cancel(p.cancel)?;
            if let Some(row) = next {
                if row.first <= current.first {
                    return Err(invalid("Unsorted sample-to-chunk rows"));
                }
                if row.first == chunk {
                    current = row;
                    next = if rows_read < track.run_count {
                        rows_read += 1;
                        Some(run(p, &mut runs, track.chunk_count, track.descriptions)?)
                    } else {
                        None
                    };
                }
            }
            let relative = if track.chunks.kind == *b"co64" {
                offsets.u64(p)?
            } else {
                u64::from(offsets.u32(p)?)
            };
            let start = p
                .start
                .checked_add(relative)
                .ok_or_else(|| invalid("Chunk offset overflow"))?;
            let count = u64::from(current.samples);
            if samples_read + count > u64::from(track.sizes.count) {
                return Err(invalid("Chunk mapping exceeds sample count"));
            }
            let mut bytes = u64::from(track.sizes.fixed) * count;
            if track.sizes.fixed == 0 {
                for i in 0..count {
                    if i % 4096 == 0 {
                        check_cancel(p.cancel)?;
                    }
                    let size = match track.sizes.bits {
                        4 => {
                            if (samples_read + i).is_multiple_of(2) {
                                nibble = sizes.byte(p)?;
                                u32::from(nibble >> 4)
                            } else {
                                u32::from(nibble & 15)
                            }
                        }
                        8 => u32::from(sizes.byte(p)?),
                        16 => (u32::from(sizes.byte(p)?) << 8) | u32::from(sizes.byte(p)?),
                        _ => sizes.u32(p)?,
                    };
                    bytes = bytes
                        .checked_add(u64::from(size))
                        .ok_or_else(|| invalid("Chunk size overflow"))?;
                }
            }
            samples_read += count;
            let end = start
                .checked_add(bytes)
                .filter(|v| *v <= p.end)
                .ok_or_else(|| invalid("Sample data extends beyond the source/range"))?;
            if start < previous_end || bytes == 0 {
                return Err(invalid(
                    "Overlapping, unordered or empty media chunks are unsupported",
                ));
            }
            if let Some(mdats) = mdats {
                if !mdats.iter().any(|m| start >= m.payload && end <= m.end) {
                    return Err(invalid("Sample bytes lie outside media-data atoms"));
                }
            }
            previous_end = end;
            maximum_end = maximum_end.max(end);
            track_bytes = track_bytes
                .checked_add(bytes)
                .ok_or_else(|| invalid("Track byte count overflow"))?;
        }
        if samples_read != u64::from(track.sizes.count)
            || next.is_some()
            || rows_read != track.run_count
            || (track.video && track_bytes == 0)
        {
            return Err(invalid("Incomplete sample/chunk mapping"));
        }
    }
    Ok(maximum_end)
}

pub(crate) struct Candidate {
    pub size: u64,
    pub kind: &'static str,
    pub evidence: String,
}
pub(crate) fn probe(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<Candidate> {
    let mut p = Parser {
        source,
        cancel,
        start,
        end,
        boxes: 0,
    };
    let (kind, ftyp) = file_kind(&mut p)?;
    let mut at = ftyp.end;
    let mut tracks = None;
    let mut required_end = 0;
    let mut mdats = Vec::new();
    // Stop at the first complete finite container covering every referenced
    // sample. Never consume trailing disk bytes or a following file's ftyp.
    for _ in 0..4096 {
        let atom = p.atom(at, end)?;
        match &atom.kind {
            b"moov" if tracks.is_none() => {
                let parsed = movie(&mut p, atom)?;
                required_end = validate_tracks(&mut p, &parsed, None)?;
                tracks = Some(parsed);
            }
            b"mdat" => {
                if atom.len() == 0 || mdats.len() == MAX_MDAT {
                    return Err(invalid("Invalid or excessive media-data atoms"));
                }
                mdats.push(atom);
            }
            b"free" | b"skip" | b"wide" | b"uuid" => {}
            _ => {
                return Err(invalid(
                    "Unexpected top-level atom or following/fragmented file",
                ))
            }
        }
        at = atom.end;
        if !mdats.is_empty() && at >= required_end {
            if let Some(tracks) = &tracks {
                validate_tracks(&mut p, tracks, Some(&mdats))?;
                // Establish accessibility at the finite physical tail as well.
                p.read(at - 1, 1, at)?;
                let videos = tracks.iter().filter(|t| t.video).count();
                return Ok(Candidate { size: at - start, kind, evidence: format!("{} classic container: {videos} video track(s); finite atom lengths, sample descriptions, timing/sample counts and every track's chunk-byte ranges inside mdat validated. Samples are not decoded; stale or overwritten contiguous bytes can still pass. No fragmented-storage reconstruction or missing-metadata repair.", kind.to_ascii_uppercase()) });
            }
        }
    }
    Err(invalid("Top-level atom count exceeds the safety limit"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    #[cfg(unix)]
    use std::{
        fs::File,
        io::{Seek, SeekFrom, Write},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Image(PathBuf);
    impl Image {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "memory-regainer-video-parser-{}-{}.img",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Image {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    #[cfg(unix)]
    fn relocate_co64(bytes: &mut [u8], shift: u64) {
        let mut pos = 0;
        while pos < bytes.len() {
            let len = be32(bytes, pos) as usize;
            let kind: [u8; 4] = bytes[pos + 4..pos + 8].try_into().unwrap();
            assert!(len >= 8 && pos + len <= bytes.len());
            if kind == *b"co64" {
                let count = be32(bytes, pos + 12) as usize;
                for index in 0..count {
                    let at = pos + 16 + index * 8;
                    let offset = be64(bytes, at) + shift;
                    bytes[at..at + 8].copy_from_slice(&offset.to_be_bytes());
                }
            } else if matches!(&kind, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl") {
                relocate_co64(&mut bytes[pos + 8..pos + len], shift);
            }
            pos += len;
        }
    }
    #[test]
    #[cfg(unix)]
    fn sparse_candidate_over_four_gib_validates_extended_mdat_and_large_relative_co64() {
        // This is a synthetic container-geometry test, not a claim of decoding
        // a four-GiB video. The real fixture payload is relocated after a sparse
        // padding area within one explicitly bounded extended-size mdat.
        let original = include_bytes!("../tests/fixtures/video/original-co64-extended.mp4");
        let mut mdat = 0;
        while &original[mdat + 4..mdat + 8] != b"mdat" {
            mdat += be32(original, mdat) as usize;
        }
        assert_eq!(be32(original, mdat), 1);
        let old_size = be64(original, mdat + 8);
        let payload = mdat + 16;
        let movie_at = mdat + old_size as usize;
        let shift = (1u64 << 32) + 4096;
        let mut movie = original[movie_at..].to_vec();
        relocate_co64(&mut movie, shift);
        let image = Image::new();
        let mut file = File::create(&image.0).unwrap();
        file.set_len(original.len() as u64 + shift).unwrap();
        file.write_all(&original[..payload]).unwrap();
        file.seek(SeekFrom::Start(mdat as u64 + 8)).unwrap();
        file.write_all(&(old_size + shift).to_be_bytes()).unwrap();
        file.seek(SeekFrom::Start(payload as u64 + shift)).unwrap();
        file.write_all(&original[payload..movie_at]).unwrap();
        file.write_all(&movie).unwrap();
        drop(file);
        let mut source = Source::open_with_size(&image.0, None).unwrap();
        let length = source.len;
        let candidate = probe(&mut source, 0, length, &AtomicBool::new(false)).unwrap();
        assert_eq!(candidate.kind, "mp4");
        assert_eq!(candidate.size, length);
        assert!(candidate.size > u64::from(u32::MAX));
        assert_eq!(
            source.bytes(payload as u64 + shift, 16).unwrap(),
            original[payload..payload + 16]
        );
    }
    fn timing_image() -> (Image, Atom) {
        let count = 20_000u32;
        let mut bytes = vec![0; 8];
        bytes[4..8].copy_from_slice(&count.to_be_bytes());
        for _ in 0..count {
            bytes.extend_from_slice(&1u32.to_be_bytes());
            bytes.extend_from_slice(&1u32.to_be_bytes());
        }
        let image = Image::new();
        fs::write(&image.0, &bytes).unwrap();
        (
            image,
            Atom {
                kind: *b"stts",
                payload: 0,
                end: bytes.len() as u64,
            },
        )
    }
    #[test]
    fn cancellation_during_streaming_sample_table_is_fatal() {
        let (image, table) = timing_image();
        let cancel = AtomicBool::new(false);
        let reads = std::cell::Cell::new(0);
        let mut source = Source::open_with_size(&image.0, None).unwrap();
        source.observer = Some(Box::new(|bytes, _| {
            if bytes > 0 {
                reads.set(reads.get() + 1);
            }
            if reads.get() == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
            Ok(())
        }));
        let mut parser = Parser {
            source: &mut source,
            cancel: &cancel,
            start: 0,
            end: table.end,
            boxes: 0,
        };
        assert!(matches!(
            timing(&mut parser, table, 20_000, false),
            Err(Error::Cancelled)
        ));
        assert_eq!(reads.get(), 2);
    }
    #[test]
    fn io_failure_during_streaming_sample_table_is_not_a_rejected_candidate() {
        let (image, table) = timing_image();
        let mut reads = 0;
        let mut source = Source::open_with_size(&image.0, None).unwrap();
        source.observer = Some(Box::new(|bytes, _| {
            if bytes > 0 {
                reads += 1;
            }
            if reads == 2 {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "synthetic table read failure",
                )));
            }
            Ok(())
        }));
        let mut parser = Parser {
            source: &mut source,
            cancel: &AtomicBool::new(false),
            start: 0,
            end: table.end,
            boxes: 0,
        };
        assert!(matches!(
            timing(&mut parser, table, 20_000, false),
            Err(Error::Io(_))
        ));
    }
}
