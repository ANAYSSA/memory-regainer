use crate::{
    archives, check_cancel, formats, le16, le32, partition::crc32_update, video, Entry, Error,
    Extent, Result, ScanDiagnostics, Source,
};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const READ_WINDOW: usize = 1024 * 1024;
const MAX_IMAGE: u64 = 256 * 1024 * 1024;
const MAX_DOCUMENT: u64 = 256 * 1024 * 1024;
const MAX_CONTAINER: u64 = 1024 * 1024 * 1024;

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn bounded_read(source: &mut Source, offset: u64, len: usize, end: u64) -> Result<Vec<u8>> {
    if offset.checked_add(len as u64).is_none_or(|n| n > end) {
        return Err(invalid("Truncated candidate"));
    }
    source.bytes(offset, len)
}
fn be32(b: &[u8], p: usize) -> u32 {
    u32::from_be_bytes(b[p..p + 4].try_into().expect("bounded field"))
}

// Separate callbacks keep metadata/results/diagnostics streaming without shared mutable state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scan(
    source: &mut Source,
    start: u64,
    end: u64,
    types: &[String],
    cancel: &AtomicBool,
    progress: impl FnMut(u64, usize),
    on_entry: &mut impl FnMut(&Entry),
    on_diagnostic: &mut impl FnMut(ScanDiagnostics),
) -> Result<Vec<Entry>> {
    let rules = formats::rules(types);
    scan_rules(
        source,
        start,
        end,
        types,
        cancel,
        progress,
        on_entry,
        on_diagnostic,
        &rules,
    )
}
#[allow(clippy::too_many_arguments)]
fn scan_rules(
    source: &mut Source,
    start: u64,
    end: u64,
    types: &[String],
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, usize),
    on_entry: &mut impl FnMut(&Entry),
    on_diagnostic: &mut impl FnMut(ScanDiagnostics),
    rules: &[formats::Rule],
) -> Result<Vec<Entry>> {
    let mut buckets: [Vec<usize>; 256] = std::array::from_fn(|_| Vec::new());
    let overlap = rules
        .iter()
        .map(|r| r.offset + r.magic.len())
        .max()
        .unwrap_or(1);
    let chunk = READ_WINDOW
        .checked_sub(overlap)
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("Signature span exceeds bounded read window"))?;
    for (index, rule) in rules.iter().enumerate() {
        buckets[rule.magic[0] as usize].push(index);
    }
    let mut found = Vec::new();
    let mut offset = start;
    let mut skip_until = start;
    let mut diagnostics = ScanDiagnostics::default();
    let mut last_diagnostic = Instant::now();
    while offset < end {
        check_cancel(cancel)?;
        let len = (end - offset).min(READ_WINDOW as u64) as usize;
        let block = source.bytes(offset, len)?;
        let process = (end - offset).min(chunk as u64) as usize;
        let mut i = 0;
        while i < process {
            if i % 4096 == 0 {
                check_cancel(cancel)?;
            }
            let anchor = offset + i as u64;
            if anchor < skip_until {
                i += (skip_until - anchor).min((process - i) as u64) as usize;
                continue;
            }
            // One indexed lookup per byte; only the few eligible magic values
            // are compared. Offset signatures (ftyp) retain arbitrary alignment.
            for index in &buckets[block[i] as usize] {
                let rule = &rules[*index];
                if !block[i..].starts_with(&rule.magic) {
                    continue;
                }
                let Some(absolute) = anchor
                    .checked_sub(rule.offset as u64)
                    .filter(|v| *v >= start)
                else {
                    continue;
                };
                diagnostics.candidates = diagnostics.candidates.saturating_add(1);
                if last_diagnostic.elapsed() >= Duration::from_millis(100) {
                    on_diagnostic(diagnostics.clone());
                    last_diagnostic = Instant::now();
                }
                let bound = end.min(absolute.saturating_add(rule.max_bytes));
                let validated = match rule.validator.as_str() {
                    "video" => video::classify(source, absolute, bound, cancel)
                        .and_then(|kind| {
                            if !formats::matches_filter(kind, types) {
                                return Err(invalid("Filtered video subtype"));
                            }
                            video::probe(source, absolute, bound, cancel)
                        })
                        .map(|c| archives::Candidate {
                            offset: absolute,
                            size: c.size,
                            kind: c.kind,
                            evidence: c.evidence,
                        }),
                    "zip-eocd" => archives::zip(source, anchor, start, end, cancel),
                    "rar" => archives::rar(source, absolute, bound, cancel),
                    validator => {
                        let (kind, result) = match validator {
                            "jpeg" => ("jpg", jpeg(source, absolute, bound, cancel)),
                            "png" => ("png", png(source, absolute, bound, cancel)),
                            "pdf" => ("pdf", pdf(source, absolute, bound, cancel)),
                            "gif" => ("gif", gif(source, absolute, bound, cancel)),
                            "bmp" => ("bmp", bmp(source, absolute, bound)),
                            "wav" => ("wav", wav(source, absolute, bound, cancel)),
                            _ => unreachable!("validated registry handler"),
                        };
                        result.map(|(size, evidence)| archives::Candidate {
                            offset: absolute,
                            size,
                            kind,
                            evidence: evidence.into(),
                        })
                    }
                };
                match validated {
                    Ok(c) if c.size <= rule.max_bytes && formats::matches_filter(c.kind, types) => {
                        let entry = Entry {
                            id: format!("carve-{:x}-{:x}", c.offset, c.size),
                            name: format!("recovered_{:016x}.{}", c.offset, c.kind),
                            kind: c.kind.into(), size:c.size, offset:c.offset,
                            confidence:"medium".into(), method:"signature-carving".into(),
                            notes:vec![c.evidence, "Contiguous signature recovery; original name, deletion status and folder are unknown. Recognisable structure does not guarantee intact content.".into()],
                            extents:vec![Extent { offset:c.offset,length:c.size }],
                            relative_path:None, partition:None, deletion_status:"unknown".into(),
                        };
                        diagnostics.accepted = diagnostics.accepted.saturating_add(1);
                        skip_until = entry.offset + entry.size;
                        on_entry(&entry);
                        found.push(entry);
                        progress(anchor - start, found.len());
                        break;
                    }
                    Ok(_) => rejection(
                        &mut diagnostics,
                        &rule.validator,
                        "filtered subtype or size limit",
                    ),
                    Err(Error::Invalid(message)) => {
                        let reason = if message.contains("CRC") || message.contains("checksum") {
                            "checksum mismatch"
                        } else if message.contains("unsupported")
                            || message.contains("Unsupported")
                            || message.contains("Unrecognised")
                        {
                            "unsupported layout"
                        } else if message.contains("limit") || message.contains("too large") {
                            "resource limit"
                        } else if message.contains("Truncated")
                            || message.contains("truncated")
                            || message.contains("bounds")
                            || message.contains("beyond")
                            || message.contains("exceeds")
                        {
                            "truncated or out of bounds"
                        } else {
                            "invalid structure"
                        };
                        rejection(&mut diagnostics, &rule.validator, reason);
                    }
                    Err(error) => {
                        on_diagnostic(diagnostics);
                        return Err(error);
                    }
                }
            }
            i += 1;
        }
        offset += process as u64;
        progress(offset - start, found.len());
        if last_diagnostic.elapsed() >= Duration::from_millis(100) {
            on_diagnostic(diagnostics.clone());
            last_diagnostic = Instant::now();
        }
    }
    on_diagnostic(diagnostics);
    Ok(found)
}
fn rejection(diagnostics: &mut ScanDiagnostics, format: &str, reason: &str) {
    diagnostics.rejected = diagnostics.rejected.saturating_add(1);
    let mut key = format!("{format}: {reason}");
    if !diagnostics.reasons.contains_key(&key) && diagnostics.reasons.len() >= 31 {
        key = "other bounded reasons".into();
    }
    let count = diagnostics.reasons.entry(key).or_default();
    *count = count.saturating_add(1);
}

fn png(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let end = end.min(start.saturating_add(MAX_IMAGE));
    let mut pos = start + 8;
    let mut ihdr = false;
    let mut idat = false;
    loop {
        check_cancel(cancel)?;
        let h = bounded_read(source, pos, 8, end)?;
        let length = u64::from(be32(&h, 0));
        if length > 0x7fff_ffff {
            return Err(invalid("PNG chunk too large"));
        }
        let next = pos
            .checked_add(12 + length)
            .ok_or_else(|| invalid("PNG overflow"))?;
        if next > end {
            return Err(invalid("PNG truncated chunk"));
        }
        let kind = &h[4..8];
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(invalid("Invalid PNG chunk type"));
        }
        if !ihdr {
            if kind != b"IHDR" || length != 13 {
                return Err(invalid("PNG has no first IHDR"));
            }
            let fields = bounded_read(source, pos + 8, 13, end)?;
            let width = be32(&fields, 0);
            let height = be32(&fields, 4);
            if width == 0
                || height == 0
                || width > 0x7fff_ffff
                || height > 0x7fff_ffff
                || fields[10] != 0
                || fields[11] != 0
                || fields[12] > 1
            {
                return Err(invalid("Invalid PNG dimensions/method"));
            }
            let valid_depth = match fields[9] {
                0 => [1, 2, 4, 8, 16].contains(&fields[8]),
                2 | 4 | 6 => [8, 16].contains(&fields[8]),
                3 => [1, 2, 4, 8].contains(&fields[8]),
                _ => false,
            };
            if !valid_depth {
                return Err(invalid("PNG invalid bit depth"));
            }
            ihdr = true;
        } else if kind == b"IHDR" {
            return Err(invalid("Duplicate PNG header"));
        }
        let mut crc = crc32_update(u32::MAX, kind);
        let mut consumed = 0;
        while consumed < length {
            check_cancel(cancel)?;
            let count = (length - consumed).min(65536) as usize;
            let bytes = bounded_read(source, pos + 8 + consumed, count, end)?;
            crc = crc32_update(crc, &bytes);
            consumed += count as u64;
        }
        let stored = bounded_read(source, pos + 8 + length, 4, end)?;
        if !crc != be32(&stored, 0) {
            return Err(invalid("PNG chunk CRC mismatch"));
        }
        if kind == b"IDAT" {
            idat = true;
        }
        if kind == b"IEND" {
            if length != 0 || !idat {
                return Err(invalid("PNG invalid IEND or missing image data"));
            }
            return Ok((next-start,"PNG chunk lengths, IHDR fields, all chunk CRCs and IEND validated; compressed pixels are not decoded."));
        }
        pos = next;
    }
}

// A buffered byte cursor avoids one OS read for every JPEG entropy byte.
struct Cursor<'a, 'b> {
    source: &'a mut Source<'b>,
    pos: u64,
    end: u64,
    buffer: Vec<u8>,
    index: usize,
}
impl<'a, 'b> Cursor<'a, 'b> {
    fn new(source: &'a mut Source<'b>, pos: u64, end: u64) -> Self {
        Self {
            source,
            pos,
            end,
            buffer: vec![],
            index: 0,
        }
    }
    fn byte(&mut self) -> Result<u8> {
        if self.pos >= self.end {
            return Err(invalid("Truncated marker stream"));
        }
        if self.index == self.buffer.len() {
            self.buffer = bounded_read(
                self.source,
                self.pos,
                (self.end - self.pos).min(65536) as usize,
                self.end,
            )?;
            self.index = 0;
        }
        let b = self.buffer[self.index];
        self.index += 1;
        self.pos += 1;
        Ok(b)
    }
    fn skip(&mut self, n: u64) -> Result<()> {
        if self.pos.checked_add(n).is_none_or(|p| p > self.end) {
            return Err(invalid("Truncated marker payload"));
        }
        self.pos += n;
        self.buffer.clear();
        self.index = 0;
        Ok(())
    }
}
fn jpeg(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let end = end.min(start.saturating_add(MAX_IMAGE));
    let mut c = Cursor::new(source, start + 2, end);
    let mut frame = false;
    let mut scan = false;
    let mut entropy = false;
    loop {
        check_cancel(cancel)?;
        let marker = if entropy {
            loop {
                if c.pos.is_multiple_of(65536) {
                    check_cancel(cancel)?;
                }
                if c.byte()? != 0xff {
                    continue;
                }
                let mut m = c.byte()?;
                while m == 0xff {
                    if c.pos.is_multiple_of(65536) {
                        check_cancel(cancel)?;
                    }
                    m = c.byte()?;
                }
                if m == 0 || (0xd0..=0xd7).contains(&m) {
                    continue;
                }
                break m;
            }
        } else {
            if c.byte()? != 0xff {
                return Err(invalid("JPEG missing marker"));
            }
            let mut m = c.byte()?;
            while m == 0xff {
                if c.pos.is_multiple_of(65536) {
                    check_cancel(cancel)?;
                }
                m = c.byte()?;
            }
            m
        };
        if marker == 0xd9 {
            if !frame || !scan {
                return Err(invalid("JPEG lacks frame or scan"));
            }
            return Ok((c.pos-start,"JPEG marker lengths, frame, scan and end marker validated; entropy-coded pixels are not decoded."));
        }
        if marker == 0xd8 || marker == 0 || (0xd0..=0xd7).contains(&marker) {
            return Err(invalid("Unexpected JPEG marker"));
        }
        if marker == 0x01 {
            continue;
        }
        let len = u16::from_be_bytes([c.byte()?, c.byte()?]) as u64;
        if len < 2 {
            return Err(invalid("Invalid JPEG segment length"));
        }
        if [
            0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf,
        ]
        .contains(&marker)
        {
            if len < 8 {
                return Err(invalid("Short JPEG frame"));
            }
            let _precision = c.byte()?;
            let h = u16::from_be_bytes([c.byte()?, c.byte()?]);
            let w = u16::from_be_bytes([c.byte()?, c.byte()?]);
            let components = c.byte()?;
            if h == 0 || w == 0 || components == 0 || len != 8 + 3 * u64::from(components) {
                return Err(invalid("Invalid JPEG frame dimensions"));
            }
            c.skip(len - 8)?;
            frame = true;
            entropy = false;
        } else if marker == 0xda {
            if !frame || len < 6 {
                return Err(invalid("Invalid JPEG scan"));
            }
            let components = c.byte()?;
            if components == 0 || len != 6 + 2 * u64::from(components) {
                return Err(invalid("Invalid JPEG scan dimensions"));
            }
            c.skip(len - 3)?;
            scan = true;
            entropy = true;
        } else {
            c.skip(len - 2)?;
            entropy = false;
        }
    }
}

fn pdf(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let end = end.min(start.saturating_add(MAX_DOCUMENT));
    let header = bounded_read(source, start, 8, end)?;
    if !(header[5] == b'1' && header[6] == b'.' && (b'0'..=b'7').contains(&header[7])
        || &header[5..8] == b"2.0")
    {
        return Err(invalid("Invalid PDF version"));
    }
    let mut pos = start + 8;
    while pos < end {
        check_cancel(cancel)?;
        let bytes = bounded_read(source, pos, (end - pos).min(65536) as usize, end)?;
        for (i, w) in bytes.windows(5).enumerate() {
            if w == b"%PDF-" {
                return Err(invalid("Nested PDF header before trailer"));
            }
            if w != b"%%EOF" {
                continue;
            }
            let eof = pos + i as u64;
            let tail_start = eof.saturating_sub(2048).max(start);
            let tail = bounded_read(source, tail_start, (eof - tail_start) as usize, end)?;
            let Some(xref_pos) = tail.windows(9).rposition(|w| w == b"startxref") else {
                return Err(invalid("PDF missing startxref"));
            };
            let raw = String::from_utf8_lossy(&tail[xref_pos + 9..]);
            let digits = raw.trim();
            let relative = digits
                .parse::<u64>()
                .map_err(|_| invalid("Invalid PDF cross-reference offset"))?;
            if relative < 8 || relative >= eof - start {
                return Err(invalid("PDF xref outside candidate"));
            }
            let xref = bounded_read(source, start + relative, 4, end)?;
            if xref != b"xref" && !xref[0].is_ascii_digit() {
                return Err(invalid("PDF xref target invalid"));
            }
            let mut size = eof + 5 - start;
            if start + size < end {
                let b = bounded_read(source, start + size, 1, end)?;
                if b[0] == b'\r' {
                    size += 1;
                }
            }
            if start + size < end {
                let b = bounded_read(source, start + size, 1, end)?;
                if b[0] == b'\n' {
                    size += 1;
                }
            }
            return Ok((size,"PDF header, EOF trailer and bounded startxref target validated; objects and incremental revisions are not fully interpreted."));
        }
        if bytes.len() <= 9 {
            break;
        }
        pos += bytes.len() as u64 - 9;
    }
    Err(invalid("PDF EOF not found within signature size bound"))
}

fn gif_subblocks(source: &mut Source, mut pos: u64, end: u64, cancel: &AtomicBool) -> Result<u64> {
    loop {
        check_cancel(cancel)?;
        let n = bounded_read(source, pos, 1, end)?[0] as u64;
        pos += 1;
        if n == 0 {
            return Ok(pos);
        }
        pos = pos.checked_add(n).ok_or_else(|| invalid("GIF overflow"))?;
        if pos > end {
            return Err(invalid("Truncated GIF subblock"));
        }
    }
}
fn gif(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let end = end.min(start.saturating_add(MAX_IMAGE));
    let h = bounded_read(source, start, 13, end)?;
    if le16(&h, 6) == 0 || le16(&h, 8) == 0 {
        return Err(invalid("GIF empty dimensions"));
    }
    let mut pos = start + 13;
    if h[10] & 0x80 != 0 {
        pos += 3 * (1u64 << ((h[10] & 7) + 1));
    }
    let mut images = 0;
    loop {
        check_cancel(cancel)?;
        let tag = bounded_read(source, pos, 1, end)?[0];
        pos += 1;
        match tag {
            0x3b if images>0=>return Ok((pos-start,"GIF screen/image dimensions, color-table bounds, subblock lengths and trailer validated; LZW pixels are not decoded.")),
            0x21=>{bounded_read(source,pos,1,end)?;pos=gif_subblocks(source,pos+1,end,cancel)?;},
            0x2c=>{let d=bounded_read(source,pos,9,end)?;if le16(&d,4)==0||le16(&d,6)==0 {return Err(invalid("GIF empty image"));}pos+=9;if d[8]&0x80!=0 {pos+=3*(1u64<<((d[8]&7)+1));}let lzw=bounded_read(source,pos,1,end)?[0];if !(2..=8).contains(&lzw) {return Err(invalid("Invalid GIF LZW code size"));}pos=gif_subblocks(source,pos+1,end,cancel)?;images+=1;},
            _=>return Err(invalid("Invalid GIF block")),
        }
    }
}
fn bmp(source: &mut Source, start: u64, end: u64) -> Result<(u64, &'static str)> {
    let h = bounded_read(source, start, 54, end)?;
    let size = u64::from(le32(&h, 2));
    let pixels = u64::from(le32(&h, 10));
    let dib = u64::from(le32(&h, 14));
    if ![40, 52, 56, 108, 124].contains(&dib)
        || !(54..=MAX_IMAGE).contains(&size)
        || pixels < 14 + dib
        || pixels >= size
        || start.checked_add(size).is_none_or(|n| n > end)
        || le16(&h, 26) != 1
    {
        return Err(invalid("Invalid BMP sizes"));
    }
    let width = i32::from_le_bytes(h[18..22].try_into().expect("width"));
    let height = i32::from_le_bytes(h[22..26].try_into().expect("height"));
    let bits = u64::from(le16(&h, 28));
    let compression = le32(&h, 30);
    if width <= 0
        || height == 0
        || ![1, 4, 8, 16, 24, 32].contains(&bits)
        || ![0, 3].contains(&compression)
    {
        return Err(invalid("Unsupported BMP structure"));
    }
    let required = (width as u64 * bits).div_ceil(32) * 4 * u64::from(height.unsigned_abs());
    if required > size - pixels {
        return Err(invalid("BMP pixel data exceeds file size"));
    }
    Ok((
        size,
        "BMP declared size, DIB, dimensions, bit depth and uncompressed pixel bounds validated.",
    ))
}
fn wav(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let h = bounded_read(source, start, 12, end)?;
    if &h[..4] != b"RIFF" || &h[8..12] != b"WAVE" {
        return Err(invalid("Not a WAVE RIFF container"));
    }
    let size = u64::from(le32(&h, 4)) + 8;
    if !(44..=MAX_CONTAINER).contains(&size) || start.checked_add(size).is_none_or(|n| n > end) {
        return Err(invalid("Invalid RIFF size"));
    }
    let end = start + size;
    let mut pos = start + 12;
    let mut fmt = false;
    let mut data = false;
    while pos < end {
        check_cancel(cancel)?;
        let h = bounded_read(source, pos, 8, end)?;
        let length = u64::from(le32(&h, 4));
        let next = pos
            .checked_add(8 + length + (length & 1))
            .ok_or_else(|| invalid("RIFF overflow"))?;
        if next > end {
            return Err(invalid("RIFF chunk outside file"));
        }
        if &h[..4] == b"fmt " {
            if length < 16 {
                return Err(invalid("WAV short format"));
            }
            let f = bounded_read(source, pos + 8, 16, end)?;
            if le16(&f, 2) == 0 || le32(&f, 4) == 0 || le16(&f, 12) == 0 {
                return Err(invalid("WAV invalid format"));
            }
            fmt = true;
        }
        if &h[..4] == b"data" {
            data = true;
        }
        pos = next;
    }
    if !fmt || !data {
        return Err(invalid("WAV missing required chunks"));
    }
    Ok((size,"WAVE RIFF length, chunk boundaries and basic format fields validated; audio samples are not decoded."))
}

#[cfg(test)]
mod registry_window_tests {
    use super::*;
    #[test]
    fn maximum_registry_signature_crosses_window_without_exceeding_read_cap() {
        let mut wave = include_bytes!("../tests/fixtures/original.wav").to_vec();
        wave.resize(2048, 0);
        let size = wave.len() as u32;
        wave[4..8].copy_from_slice(&(size - 8).to_le_bytes());
        wave[40..44].copy_from_slice(&(size - 44).to_le_bytes());
        wave[1024..1088].fill(0xab);
        let overlap = 1024 + 64;
        let prefix = READ_WINDOW - overlap - 1024 - 32;
        let mut image = vec![0u8; prefix];
        image.extend_from_slice(&wave);
        struct Temporary(std::path::PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let file = Temporary(std::env::temp_dir().join(format!(
            "memory-regainer-max-signature-{}.img",
            std::process::id()
        )));
        std::fs::write(&file.0, &image).unwrap();
        let mut source = Source::open_with_size(&file.0, None).unwrap();
        let rule = formats::Rule {
            offset: 1024,
            magic: vec![0xab; 64],
            validator: "wav".into(),
            max_bytes: 1024 * 1024,
        };
        let entries = scan_rules(
            &mut source,
            0,
            image.len() as u64,
            &["wav".into()],
            &AtomicBool::new(false),
            |_, _| {},
            &mut |_| {},
            &mut |_| {},
            &[rule],
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].offset, prefix as u64);
        assert_eq!(entries[0].size, wave.len() as u64);
    }
}
