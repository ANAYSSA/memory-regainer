//! Bounded, independent archive framing from PKWARE APPNOTE and RARLab format
//! documentation. No extraction paths or archive programs are invoked.
use crate::{check_cancel, le16, le32, partition::crc32_update, Error, Result, Source};
use flate2::read::DeflateDecoder;
use quick_xml::{events::Event, name::ResolveResult, NsReader};
use std::{collections::BTreeMap, io::Read, sync::atomic::AtomicBool};
const MAX_ARCHIVE: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DIRECTORY: u64 = 64 * 1024 * 1024;
const MAX_FILES: u64 = 100_000;
const MAX_XML: u64 = 1024 * 1024;
fn bad(s: &str) -> Error {
    Error::Invalid(format!("Archive: {s}"))
}
fn u64le(b: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(b[n..n + 8].try_into().expect("bounded field"))
}
fn crc(b: &[u8]) -> u32 {
    !crc32_update(u32::MAX, b)
}
fn read(
    s: &mut Source,
    at: u64,
    n: usize,
    lower: u64,
    upper: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    check_cancel(cancel)?;
    if at < lower || at.checked_add(n as u64).is_none_or(|v| v > upper) {
        return Err(bad("Truncated or overflowing record"));
    }
    s.bytes(at, n)
}
pub(crate) struct Candidate {
    pub offset: u64,
    pub size: u64,
    pub kind: &'static str,
    pub evidence: String,
}
#[derive(Clone)]
struct Member {
    name: Vec<u8>,
    data: u64,
    compressed: u64,
    uncompressed: u64,
    crc: u32,
    method: u16,
}
fn extra64(extra: &[u8], values: &mut [u64], needs: &[bool]) -> Result<()> {
    let mut at = 0;
    let mut found = false;
    while at < extra.len() {
        if extra.len() - at < 4 {
            return Err(bad("Truncated ZIP extra field"));
        }
        let id = le16(extra, at);
        let len = usize::from(le16(extra, at + 2));
        at += 4;
        let end = at
            .checked_add(len)
            .filter(|v| *v <= extra.len())
            .ok_or_else(|| bad("ZIP extra field bounds"))?;
        if id == 1 {
            if found {
                return Err(bad("Duplicate ZIP64 extra field"));
            }
            found = true;
            let mut p = at;
            for (i, needed) in needs.iter().enumerate() {
                if *needed {
                    let width = if i == 3 { 4 } else { 8 };
                    if p + width > end {
                        return Err(bad("Missing ZIP64 size or offset"));
                    }
                    values[i] = if width == 8 {
                        u64le(extra, p)
                    } else {
                        u64::from(le32(extra, p))
                    };
                    p += width;
                }
            }
        }
        at = end;
    }
    if needs.iter().any(|n| *n) && !found {
        return Err(bad("Missing ZIP64 extra field"));
    }
    Ok(())
}
fn descriptor(
    s: &mut Source,
    at: u64,
    m: &Member,
    wide: bool,
    end: u64,
    cancel: &AtomicBool,
) -> Result<u64> {
    // CRC may equal the optional signature. Test both layouts against the
    // authoritative central record instead of assuming the first four bytes.
    for signature in [true, false] {
        let size = if wide { 20 } else { 12 } + if signature { 4 } else { 0 };
        if at.checked_add(size).is_none_or(|v| v > end) {
            continue;
        }
        let b = read(s, at, size as usize, 0, end, cancel)?;
        if signature && &b[..4] != b"PK\x07\x08" {
            continue;
        }
        let p = if signature { 4 } else { 0 };
        let compressed = if wide {
            u64le(&b, p + 4)
        } else {
            u64::from(le32(&b, p + 4))
        };
        let uncompressed = if wide {
            u64le(&b, p + 12)
        } else {
            u64::from(le32(&b, p + 8))
        };
        if le32(&b, p) == m.crc && compressed == m.compressed && uncompressed == m.uncompressed {
            return Ok(at + size);
        }
    }
    Err(bad("ZIP data descriptor disagrees with central directory"))
}
/// EOCD-led lookup avoids rescanning up to gigabytes for every PK local header.
/// Descriptors (including signatureless ones) are bounded using central sizes.
pub(crate) fn zip(
    s: &mut Source,
    eocd: u64,
    lower: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<Candidate> {
    let e = read(s, eocd, 22, lower, end, cancel)?;
    if &e[..4] != b"PK\x05\x06" || le16(&e, 4) != 0 || le16(&e, 6) != 0 {
        return Err(bad("ZIP multivolume or invalid end record"));
    }
    let finish = eocd
        .checked_add(22 + u64::from(le16(&e, 20)))
        .filter(|v| *v <= end)
        .ok_or_else(|| bad("ZIP comment truncated"))?;
    let mut count = u64::from(le16(&e, 10));
    let mut cdsize = u64::from(le32(&e, 12));
    let mut relative = u64::from(le32(&e, 16));
    let mut cdend = eocd;
    let zip64 = le16(&e, 8) == u16::MAX
        || count == u64::from(u16::MAX)
        || cdsize == u64::from(u32::MAX)
        || relative == u64::from(u32::MAX);
    let base;
    if zip64 {
        let locator = eocd
            .checked_sub(20)
            .filter(|v| *v >= lower)
            .ok_or_else(|| bad("Missing ZIP64 locator"))?;
        let l = read(s, locator, 20, lower, end, cancel)?;
        if &l[..4] != b"PK\x06\x07" || le32(&l, 4) != 0 || le32(&l, 16) != 1 {
            return Err(bad("ZIP64 multivolume or locator invalid"));
        }
        // Optional ZIP64 extensible data is bounded to64KiB, not unbounded search.
        let begin = locator.saturating_sub(65536).max(lower);
        let bytes = read(s, begin, (locator - begin) as usize, lower, end, cancel)?;
        let mut record = None;
        for i in (0..bytes.len().saturating_sub(55)).rev() {
            if &bytes[i..i + 4] == b"PK\x06\x06" {
                let n = u64le(&bytes, i + 4);
                if n >= 44
                    && n.checked_add(12)
                        .and_then(|size| (begin + i as u64).checked_add(size))
                        == Some(locator)
                {
                    record = Some(begin + i as u64);
                    break;
                }
            }
        }
        let at = record.ok_or_else(|| bad("Missing finite ZIP64 end record"))?;
        let h = read(s, at, 56, lower, end, cancel)?;
        if le32(&h, 16) != 0 || le32(&h, 20) != 0 || u64le(&h, 24) != u64le(&h, 32) {
            return Err(bad("ZIP64 disk/count mismatch"));
        }
        count = u64le(&h, 32);
        cdsize = u64le(&h, 40);
        relative = u64le(&h, 48);
        cdend = at;
        base = at
            .checked_sub(u64le(&l, 8))
            .ok_or_else(|| bad("ZIP64 relative end offset invalid"))?;
        if le16(&e, 8) != u16::MAX && u64::from(le16(&e, 8)) != count
            || le16(&e, 10) != u16::MAX && u64::from(le16(&e, 10)) != count
            || le32(&e, 12) != u32::MAX && u64::from(le32(&e, 12)) != cdsize
            || le32(&e, 16) != u32::MAX && u64::from(le32(&e, 16)) != relative
        {
            return Err(bad("ZIP64 and legacy end records disagree"));
        }
    } else {
        if le16(&e, 8) != le16(&e, 10) {
            return Err(bad("ZIP disk/count mismatch"));
        }
        base = eocd
            .checked_sub(cdsize)
            .and_then(|p| p.checked_sub(relative))
            .ok_or_else(|| bad("ZIP directory offset underflow"))?;
    }
    let directory = base
        .checked_add(relative)
        .ok_or_else(|| bad("ZIP directory offset overflow"))?;
    if base < lower
        || base >= finish
        || finish - base > MAX_ARCHIVE
        || count == 0
        || count > MAX_FILES
        || cdsize > MAX_DIRECTORY
        || directory.checked_add(cdsize) != Some(cdend)
    {
        return Err(bad("ZIP archive/directory bounds or resource limit"));
    }
    let mut at = directory;
    let mut spans = Vec::new();
    let mut members = Vec::new();
    let mut name_bytes = 0usize;
    for _ in 0..count {
        let h = read(s, at, 46, directory, cdend, cancel)?;
        if &h[..4] != b"PK\x01\x02" {
            return Err(bad("ZIP central header invalid"));
        }
        let flags = le16(&h, 8);
        let method = le16(&h, 10);
        if flags & (1 | 64 | 8192) != 0 {
            return Err(bad("Encrypted ZIP unsupported"));
        }
        let name_len = usize::from(le16(&h, 28));
        let extra_len = usize::from(le16(&h, 30));
        let comment = usize::from(le16(&h, 32));
        name_bytes += name_len;
        if name_len == 0 || name_bytes > 16 * 1024 * 1024 {
            return Err(bad("ZIP filename metadata limit"));
        }
        let variable = read(
            s,
            at + 46,
            name_len + extra_len + comment,
            directory,
            cdend,
            cancel,
        )?;
        let name = variable[..name_len].to_vec();
        let needs = [
            le32(&h, 24) == u32::MAX,
            le32(&h, 20) == u32::MAX,
            le32(&h, 42) == u32::MAX,
            le16(&h, 34) == u16::MAX,
        ];
        let mut values = [
            u64::from(le32(&h, 24)),
            u64::from(le32(&h, 20)),
            u64::from(le32(&h, 42)),
            u64::from(le16(&h, 34)),
        ];
        extra64(
            &variable[name_len..name_len + extra_len],
            &mut values,
            &needs,
        )?;
        if values[3] != 0 {
            return Err(bad("ZIP split archive unsupported"));
        }
        let local = base
            .checked_add(values[2])
            .filter(|v| *v < directory)
            .ok_or_else(|| bad("ZIP local offset outside archive"))?;
        let l = read(s, local, 30, base, directory, cancel)?;
        if &l[..4] != b"PK\x03\x04" || le16(&l, 6) != flags || le16(&l, 8) != method {
            return Err(bad("ZIP local/central header mismatch"));
        }
        let ln = usize::from(le16(&l, 26));
        let lx = usize::from(le16(&l, 28));
        let extra = read(s, local + 30, ln + lx, base, directory, cancel)?;
        if extra[..ln] != name {
            return Err(bad("ZIP local/central filename mismatch"));
        }
        let local_wide = le32(&l, 18) == u32::MAX || le32(&l, 22) == u32::MAX;
        let mut lv = [u64::from(le32(&l, 22)), u64::from(le32(&l, 18))];
        extra64(
            &extra[ln..],
            &mut lv,
            &[le32(&l, 22) == u32::MAX, le32(&l, 18) == u32::MAX],
        )?;
        if flags & 8 == 0 {
            if lv != values[..2] || le32(&l, 14) != le32(&h, 16) {
                return Err(bad("ZIP local/central size or CRC mismatch"));
            }
        } else if (lv[0] != 0 && lv[0] != values[0])
            || (lv[1] != 0 && lv[1] != values[1])
            || (le32(&l, 14) != 0 && le32(&l, 14) != le32(&h, 16))
        {
            return Err(bad("ZIP descriptor local metadata mismatch"));
        }
        let data = local + 30 + (ln + lx) as u64;
        let data_end = data
            .checked_add(values[1])
            .filter(|v| *v <= directory)
            .ok_or_else(|| bad("ZIP data extends into directory"))?;
        let m = Member {
            name,
            data,
            compressed: values[1],
            uncompressed: values[0],
            crc: le32(&h, 16),
            method,
        };
        if method == 0 && m.compressed != m.uncompressed {
            return Err(bad("Stored ZIP sizes disagree"));
        }
        let local_end = if flags & 8 != 0 {
            descriptor(
                s,
                data_end,
                &m,
                local_wide || needs[0] || needs[1],
                directory,
                cancel,
            )?
        } else {
            data_end
        };
        spans.push((local, local_end));
        members.push(m);
        at += 46 + (name_len + extra_len + comment) as u64;
    }
    if at != cdend {
        return Err(bad("ZIP directory count/length mismatch"));
    }
    spans.sort_unstable();
    let mut next = base;
    for (start, finish) in spans {
        if start != next || finish < start {
            return Err(bad("ZIP local records overlap or contain unsupported gaps"));
        }
        next = finish;
    }
    if next != directory {
        return Err(bad("ZIP local data does not meet central directory"));
    }
    let kind = match is_docx(s, &members, base, directory, cancel) {
        Ok(true) => "docx",
        Ok(false) | Err(Error::Invalid(_)) => "zip",
        Err(e) => return Err(e),
    };
    Ok(Candidate {offset:base,size:finish-base,kind,evidence:format!("{}: central/local names, sizes, offsets, counts and descriptor CRC fields agree; contiguous {} records and finite end validated. ZIP64 supported; encryption/split archives unsupported. General payloads are not decompressed or CRC-verified.{}",kind.to_ascii_uppercase(),count,if kind=="docx" {" DOCX bounded metadata CRC, content type, internal main-document relationship and XML root validated."}else{""})})
}
fn metadata(
    s: &mut Source,
    m: &Member,
    lower: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    if m.compressed > MAX_XML || m.uncompressed > MAX_XML || ![0, 8].contains(&m.method) {
        return Err(bad("DOCX metadata compression/size limit"));
    }
    let bytes = read(s, m.data, m.compressed as usize, lower, end, cancel)?;
    let out = if m.method == 0 {
        bytes
    } else {
        let mut decoder = DeflateDecoder::new(bytes.as_slice());
        let mut out = Vec::new();
        let mut buffer = [0; 16384];
        loop {
            check_cancel(cancel)?;
            let n = decoder
                .read(&mut buffer)
                .map_err(|_| bad("DOCX metadata deflate invalid"))?;
            if n == 0 {
                break;
            }
            if out.len() + n > MAX_XML as usize {
                return Err(bad("DOCX metadata inflation limit"));
            }
            out.extend_from_slice(&buffer[..n]);
        }
        if decoder.total_in() != m.compressed {
            return Err(bad("Trailing DOCX compressed metadata"));
        }
        out
    };
    if out.len() as u64 != m.uncompressed || crc(&out) != m.crc {
        return Err(bad("DOCX metadata CRC/size mismatch"));
    }
    Ok(out)
}
struct XmlNode {
    ns: Vec<u8>,
    name: Vec<u8>,
    attrs: BTreeMap<String, String>,
    depth: usize,
}
fn xml(bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<XmlNode>> {
    let text = std::str::from_utf8(bytes).map_err(|_| bad("Unsupported DOCX metadata encoding"))?;
    let mut r = NsReader::from_str(text);
    r.config_mut().expand_empty_elements = true;
    let mut nodes = Vec::new();
    let mut depth = 0usize;
    let mut roots = 0;
    let mut version = quick_xml::XmlVersion::Implicit1_0;
    loop {
        check_cancel(cancel)?;
        let (ns, event) = r
            .read_resolved_event()
            .map_err(|_| bad("DOCX XML malformed"))?;
        match event {
            Event::Start(e) | Event::Empty(e) => {
                if depth == 0 {
                    roots += 1;
                }
                if roots > 1 || depth > 64 || nodes.len() >= 10000 {
                    return Err(bad("DOCX XML nesting/node limit"));
                }
                let namespace = match ns {
                    ResolveResult::Bound(n) => n.as_ref().as_bytes().to_vec(),
                    _ => return Err(bad("DOCX XML namespace missing")),
                };
                let mut attrs = BTreeMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| bad("DOCX XML duplicate/malformed attribute"))?;
                    let key = a.key.as_ref().to_string();
                    let value = a
                        .normalized_value(version)
                        .map_err(|_| bad("DOCX attribute entity"))?
                        .into_owned();
                    attrs.insert(key, value);
                }
                nodes.push(XmlNode {
                    ns: namespace,
                    name: e.local_name().as_ref().as_bytes().to_vec(),
                    attrs,
                    depth,
                });
                depth += 1;
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| bad("DOCX XML end mismatch"))?;
            }
            Event::Decl(decl) => {
                version = decl.xml_version().map_err(|_| bad("DOCX XML version"))?;
            }
            Event::DocType(_) => return Err(bad("DOCX DTD unsupported")),
            Event::Text(text) if depth == 0 && !text.as_ref().trim().is_empty() => {
                return Err(bad("Text outside DOCX XML root"))
            }
            Event::GeneralRef(reference) => {
                if depth == 0
                    || (!matches!(reference.as_ref(), "amp" | "lt" | "gt" | "quot" | "apos")
                        && reference
                            .resolve_char_ref()
                            .map_err(|_| bad("Invalid DOCX XML character reference"))?
                            .is_none())
                {
                    return Err(bad("Undeclared DOCX XML entity"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || roots != 1 {
        return Err(bad("DOCX XML root invalid"));
    }
    Ok(nodes)
}
fn is_docx(
    s: &mut Source,
    ms: &[Member],
    lower: u64,
    end: u64,
    cancel: &AtomicBool,
) -> Result<bool> {
    let find = |name: &[u8]| -> Result<&Member> {
        let mut i = ms.iter().filter(|m| m.name == name);
        let first = i.next().ok_or_else(|| bad("DOCX metadata absent"))?;
        if i.next().is_some() {
            return Err(bad("DOCX duplicate part"));
        }
        Ok(first)
    };
    let rels = xml(
        &metadata(s, find(b"_rels/.rels")?, lower, end, cancel)?,
        cancel,
    )?;
    let relns = b"http://schemas.openxmlformats.org/package/2006/relationships";
    if rels[0].name != b"Relationships" || rels[0].ns != relns {
        return Ok(false);
    }
    let mut targets = Vec::new();
    for n in &rels {
        if n.depth == 1 && n.name == b"Relationship" && n.ns == relns
            && n.attrs.get("Type").is_some_and(|t| [
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument",
            ].contains(&t.as_str())) {
            if n.attrs.get("TargetMode").is_some_and(|v| v != "Internal") { return Ok(false); }
            if let Some(t) = n.attrs.get("Target") { targets.push(t.trim_start_matches('/').to_string()); }
        }
    }
    if targets.len() != 1 {
        return Ok(false);
    }
    let target = &targets[0];
    if target.is_empty()
        || target.contains(['\\', ':', '%', '?', '#'])
        || target
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Ok(false);
    }
    let types = xml(
        &metadata(s, find(b"[Content_Types].xml")?, lower, end, cancel)?,
        cancel,
    )?;
    let typens = b"http://schemas.openxmlformats.org/package/2006/content-types";
    if types[0].name != b"Types" || types[0].ns != typens {
        return Ok(false);
    }
    let part = format!("/{target}");
    let matched = types
        .iter()
        .filter(|n| {
            n.depth == 1
                && n.name == b"Override"
                && n.ns == typens
                && n.attrs.get("PartName") == Some(&part)
        })
        .collect::<Vec<_>>();
    if matched.len() != 1
        || matched[0].attrs.get("ContentType").is_none_or(|v| {
            v != "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
        })
    {
        return Ok(false);
    }
    let main = xml(
        &metadata(s, find(target.as_bytes())?, lower, end, cancel)?,
        cancel,
    )?;
    Ok(main[0].name == b"document"
        && [
            b"http://schemas.openxmlformats.org/wordprocessingml/2006/main".as_slice(),
            b"http://purl.oclc.org/ooxml/wordprocessingml/main",
        ]
        .contains(&main[0].ns.as_slice()))
}
fn vint(bytes: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let b = *bytes
            .get(*at)
            .ok_or_else(|| bad("Truncated RAR variable integer"))?;
        *at += 1;
        if shift == 63 && b > 1 {
            return Err(bad("RAR variable integer overflow"));
        }
        value |= u64::from(b & 127) << shift;
        if b & 128 == 0 {
            return Ok(value);
        }
    }
    Err(bad("RAR variable integer too long"))
}
fn rar_extra(bytes: &[u8], file: bool) -> Result<()> {
    let mut at = 0;
    while at < bytes.len() {
        let size = vint(bytes, &mut at)?;
        let end = at
            .checked_add(usize::try_from(size).map_err(|_| bad("RAR extra size overflow"))?)
            .filter(|v| *v <= bytes.len())
            .ok_or_else(|| bad("RAR extra area bounds"))?;
        let kind = vint(&bytes[..end], &mut at)?;
        if file && kind == 1 {
            return Err(bad("Encrypted RAR file unsupported"));
        }
        at = end;
    }
    Ok(())
}
pub(crate) fn rar(s: &mut Source, start: u64, end: u64, cancel: &AtomicBool) -> Result<Candidate> {
    let end = end.min(start.saturating_add(MAX_ARCHIVE));
    let magic = read(s, start, 7, start, end, cancel)?;
    if magic == b"Rar!\x1a\x07\0" {
        rar4(s, start, end, cancel)
    } else if magic == b"Rar!\x1a\x07\x01" && read(s, start + 7, 1, start, end, cancel)? == [0] {
        rar5(s, start, end, cancel)
    } else {
        Err(bad("RAR marker invalid"))
    }
}
fn rar_result(start: u64, end: u64, version: u8, files: u64) -> Candidate {
    Candidate {offset:start,size:end-start,kind:"rar",evidence:format!("RAR {version}: {files} file records, header CRCs, finite header/data bounds and explicit archive end validated. Split/encrypted archives unsupported; archive contents are not decompressed or payload-verified.")}
}
fn rar4(s: &mut Source, start: u64, end: u64, cancel: &AtomicBool) -> Result<Candidate> {
    let mut at = start + 7;
    let mut main = false;
    let mut files = 0;
    for _ in 0..MAX_FILES {
        let h = read(s, at, 7, start, end, cancel)?;
        let kind = h[2];
        let flags = le16(&h, 3);
        let size = usize::from(le16(&h, 5));
        if size < 7 {
            return Err(bad("RAR4 header shorter than fixed fields"));
        }
        let h = read(s, at, size, start, end, cancel)?;
        if crc(&h[2..]) as u16 != le16(&h, 0) {
            return Err(bad("RAR4 header CRC mismatch"));
        }
        if !main && kind != 0x73 {
            return Err(bad("RAR4 main header absent"));
        }
        let mut data = if flags & 0x8000 != 0 {
            if size < 11 {
                return Err(bad("RAR4 data length absent"));
            }
            u64::from(le32(&h, 7))
        } else {
            0
        };
        match kind {
            0x73 => {
                if main || size < 13 || flags & (1 | 2 | 128 | 256) != 0 || data != 0 {
                    return Err(bad(
                        "RAR4 split/encrypted/legacy-comment main header unsupported",
                    ));
                }
                main = true;
            }
            0x74 | 0x7a => {
                if size < 32 || flags & 0x8000 == 0 || flags & (1 | 2 | 4 | 8 | 1024) != 0 {
                    return Err(bad("RAR4 split/encrypted/legacy-comment file unsupported"));
                }
                let name = usize::from(le16(&h, 26));
                let fixed = if flags & 256 != 0 { 40 } else { 32 };
                if size < fixed
                    || name == 0
                    || name > size - fixed
                    || !(0x30..=0x35).contains(&h[25])
                {
                    return Err(bad("RAR4 filename or compression fields invalid"));
                }
                if fixed == 40 {
                    data |= u64::from(le32(&h, 32)) << 32;
                }
                if kind == 0x74 {
                    files += 1;
                }
            }
            0x7b => {
                if flags & (1 | 8) != 0 || data != 0 || files == 0 {
                    return Err(bad("RAR4 continued volume or empty archive"));
                }
                let expected =
                    7 + if flags & 2 != 0 { 4 } else { 0 } + if flags & 4 != 0 { 7 } else { 0 };
                if size != expected {
                    return Err(bad("RAR4 end header length mismatch"));
                }
                return Ok(rar_result(start, at + size as u64, 4, files));
            }
            // Older comment/security/owner records have special CRC semantics.
            // Reject instead of guessing their boundaries or accepting bad CRCs.
            _ => return Err(bad("Unsupported RAR4 service record")),
        }
        at = at
            .checked_add(size as u64)
            .and_then(|v| v.checked_add(data))
            .filter(|v| *v <= end)
            .ok_or_else(|| bad("RAR4 data exceeds scan bounds"))?;
    }
    Err(bad("RAR4 record limit exceeded"))
}
fn rar5(s: &mut Source, start: u64, end: u64, cancel: &AtomicBool) -> Result<Candidate> {
    let mut at = start + 8;
    let mut main = false;
    let mut files = 0;
    for _ in 0..MAX_FILES {
        // RAR5's size vint is at most3bytes; read only bytes actually needed.
        let checksum = read(s, at, 4, start, end, cancel)?;
        let mut size_bytes = Vec::new();
        for i in 0..3 {
            let b = read(s, at + 4 + i, 1, start, end, cancel)?[0];
            size_bytes.push(b);
            if b & 128 == 0 {
                break;
            }
        }
        let mut cursor = 0;
        let size = vint(&size_bytes, &mut cursor)?;
        if size == 0 || size > 2 * 1024 * 1024 {
            return Err(bad("RAR5 header size limit"));
        }
        let payload = at + 4 + size_bytes.len() as u64;
        let h = read(s, payload, size as usize, start, end, cancel)?;
        let mut crc_value = crc32_update(u32::MAX, &size_bytes);
        crc_value = crc32_update(crc_value, &h);
        if !crc_value != le32(&checksum, 0) {
            return Err(bad("RAR5 header CRC mismatch"));
        }
        let mut p = 0;
        let kind = vint(&h, &mut p)?;
        let flags = vint(&h, &mut p)?;
        if flags & 24 != 0 {
            return Err(bad("Split RAR5 data unsupported"));
        }
        let extra = if flags & 1 != 0 { vint(&h, &mut p)? } else { 0 };
        let data = if flags & 2 != 0 { vint(&h, &mut p)? } else { 0 };
        let fixed_end = h
            .len()
            .checked_sub(usize::try_from(extra).map_err(|_| bad("RAR5 extra overflow"))?)
            .filter(|v| *v >= p)
            .ok_or_else(|| bad("RAR5 extra overlaps fixed header"))?;
        let fixed = &h[..fixed_end];
        if !main && kind != 1 {
            return Err(bad("RAR5 main header absent or encrypted"));
        }
        match kind {
            1 => {
                let archive_flags = vint(fixed, &mut p)?;
                if main || archive_flags & 3 != 0 || data != 0 {
                    return Err(bad("RAR5 volume/main header unsupported"));
                }
                main = true;
            }
            2 | 3 => {
                let f = vint(fixed, &mut p)?;
                let _unpacked = vint(fixed, &mut p)?;
                let _attributes = vint(fixed, &mut p)?;
                if f & 2 != 0 {
                    p = p
                        .checked_add(4)
                        .filter(|v| *v <= fixed_end)
                        .ok_or_else(|| bad("RAR5 time truncated"))?;
                }
                if f & 4 != 0 {
                    p = p
                        .checked_add(4)
                        .filter(|v| *v <= fixed_end)
                        .ok_or_else(|| bad("RAR5 file CRC truncated"))?;
                }
                let compression = vint(fixed, &mut p)?;
                let host = vint(fixed, &mut p)?;
                let name = vint(fixed, &mut p)?;
                if compression & 63 > 1
                    || (compression >> 7) & 7 > 5
                    || host > 1
                    || name == 0
                    || name > 65535
                {
                    return Err(bad("RAR5 file fields unsupported"));
                }
                let name_end = p
                    .checked_add(name as usize)
                    .filter(|v| *v <= fixed_end)
                    .ok_or_else(|| bad("RAR5 filename truncated"))?;
                std::str::from_utf8(&h[p..name_end])
                    .map_err(|_| bad("RAR5 filename is not UTF-8"))?;
                p = name_end;
                if kind == 2 {
                    files += 1;
                }
            }
            5 => {
                let next = vint(fixed, &mut p)?;
                if next != 0 || data != 0 || files == 0 || p != fixed_end {
                    return Err(bad("RAR5 continued/invalid archive end"));
                }
                rar_extra(&h[fixed_end..], false)?;
                return Ok(rar_result(start, payload + size, 5, files));
            }
            4 => return Err(bad("Encrypted RAR5 headers unsupported")),
            _ => return Err(bad("Unknown RAR5 header type")),
        }
        if p != fixed_end {
            return Err(bad("RAR5 fixed header length mismatch"));
        }
        rar_extra(&h[fixed_end..], kind == 2 || kind == 3)?;
        at = payload
            .checked_add(size)
            .and_then(|v| v.checked_add(data))
            .filter(|v| *v <= end)
            .ok_or_else(|| bad("RAR5 data exceeds scan bounds"))?;
    }
    Err(bad("RAR5 record limit exceeded"))
}
