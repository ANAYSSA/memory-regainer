use crate::{
    check_cancel, le16, le32, partition::crc32_update, Entry, Error, Extent, Result, Source,
};
use std::sync::atomic::AtomicBool;

const CHUNK: usize = 1024 * 1024 - 16;
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

pub(crate) fn scan(
    source: &mut Source,
    start: u64,
    end: u64,
    types: &[String],
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, usize),
) -> Result<Vec<Entry>> {
    let mut found = Vec::new();
    let mut offset = start;
    let mut skip_until = start;
    while offset < end {
        check_cancel(cancel)?;
        let len = (end - offset).min((CHUNK + 16) as u64) as usize;
        let block = source.bytes(offset, len)?;
        let process = (end - offset).min(CHUNK as u64) as usize;
        let mut i = 0;
        while i < process {
            if i % 4096 == 0 {
                check_cancel(cancel)?;
            }
            let absolute = offset + i as u64;
            if absolute < skip_until {
                i += (skip_until - absolute).min((process - i) as u64) as usize;
                continue;
            }
            let b = &block[i..];
            let kind = if b.starts_with(b"\x89PNG\r\n\x1a\n") {
                "png"
            } else if b.starts_with(b"\xff\xd8\xff") {
                "jpg"
            } else if b.starts_with(b"%PDF-") {
                "pdf"
            } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
                "gif"
            } else if b.starts_with(b"PK\x03\x04") {
                "zip"
            } else if b.starts_with(b"BM") {
                "bmp"
            } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE" {
                "wav"
            } else {
                i += 1;
                continue;
            };
            if !types.is_empty() && !types.iter().any(|t| t.eq_ignore_ascii_case(kind)) {
                i += 1;
                continue;
            }
            let validated = match kind {
                "png" => png(source, absolute, end, cancel),
                "jpg" => jpeg(source, absolute, end, cancel),
                "pdf" => pdf(source, absolute, end, cancel),
                "gif" => gif(source, absolute, end, cancel),
                "zip" => zip(source, absolute, end, cancel),
                "bmp" => bmp(source, absolute, end),
                "wav" => wav(source, absolute, end, cancel),
                _ => unreachable!(),
            };
            match validated {
                Ok((size, evidence)) => {
                    found.push(Entry { id:format!("carve-{absolute:x}-{size:x}"),name:format!("recovered_{absolute:016x}.{kind}"),kind:kind.into(),size,offset:absolute,confidence:"medium".into(),method:"signature-carving".into(),notes:vec![evidence.into(),"Contiguous signature recovery; original name, deletion status and folder are unknown. Recognisable structure does not guarantee intact content.".into()],extents:vec![Extent {offset:absolute,length:size}] });
                    skip_until = absolute + size;
                }
                Err(Error::Invalid(_)) => {}
                Err(e) => return Err(e),
            }
            i += 1;
        }
        offset += process as u64;
        progress(offset - start, found.len());
    }
    Ok(found)
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
fn zip(
    source: &mut Source,
    start: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<(u64, &'static str)> {
    let end = end.min(start.saturating_add(MAX_CONTAINER));
    let mut pos = start;
    let mut files = 0u32;
    // Follow actual local record sizes, so arbitrary PK sequences inside data
    // cannot be mistaken for directory/end records. Data descriptors are not yet supported.
    loop {
        check_cancel(cancel)?;
        let h = bounded_read(source, pos, 4, end)?;
        if h == b"PK\x01\x02" {
            break;
        }
        if h != b"PK\x03\x04" {
            return Err(invalid("ZIP missing local header"));
        }
        let h = bounded_read(source, pos, 30, end)?;
        if le16(&h, 6) & 9 != 0 {
            return Err(invalid(
                "ZIP encrypted/descriptor streams require unsupported metadata",
            ));
        }
        let compressed = u64::from(le32(&h, 18));
        let extra = u64::from(le16(&h, 26)) + u64::from(le16(&h, 28));
        if compressed == u64::from(u32::MAX) {
            return Err(invalid("ZIP64 unsupported"));
        }
        pos = pos
            .checked_add(30 + extra + compressed)
            .ok_or_else(|| invalid("ZIP overflow"))?;
        if pos > end {
            return Err(invalid("ZIP payload exceeds range"));
        }
        files += 1;
    }
    let directory = pos;
    let mut directory_files = 0u32;
    loop {
        check_cancel(cancel)?;
        let h = bounded_read(source, pos, 4, end)?;
        if h == b"PK\x05\x06" {
            break;
        }
        if h != b"PK\x01\x02" {
            return Err(invalid("ZIP central directory invalid"));
        }
        let h = bounded_read(source, pos, 46, end)?;
        let local = u64::from(le32(&h, 42));
        if local >= directory - start
            || bounded_read(source, start + local, 4, end)? != b"PK\x03\x04"
        {
            return Err(invalid("ZIP local-header pointer invalid"));
        }
        if le16(&h, 34) != 0 {
            return Err(invalid("ZIP multi-disk unsupported"));
        }
        pos = pos
            .checked_add(
                46 + u64::from(le16(&h, 28)) + u64::from(le16(&h, 30)) + u64::from(le16(&h, 32)),
            )
            .ok_or_else(|| invalid("ZIP directory overflow"))?;
        if pos > end {
            return Err(invalid("ZIP directory exceeds range"));
        }
        directory_files += 1;
    }
    let e = bounded_read(source, pos, 22, end)?;
    if le16(&e, 4) != 0
        || le16(&e, 6) != 0
        || u32::from(le16(&e, 8)) != files
        || u32::from(le16(&e, 10)) != files
        || directory_files != files
        || u64::from(le32(&e, 12)) != pos - directory
        || u64::from(le32(&e, 16)) != directory - start
    {
        return Err(invalid("ZIP end record does not match directory"));
    }
    let finish = pos + 22 + u64::from(le16(&e, 20));
    if finish > end {
        return Err(invalid("ZIP comment truncated"));
    }
    Ok((finish-start,"ZIP local/central directory boundaries, offsets and counts validated; payload CRC/decompression not checked. ZIP64, encryption and data-descriptor streams are not supported."))
}
