# Archive and format-registry validation — v0.2.0

The archive carvers are independent bounded parsers for contiguous source bytes. Tests establish exact recovery on original generated fixtures, not a success rate on damaged media. They do not add arbitrary archive-fragment reconstruction or runtime payload decompression. The app has no dependency on Python, bsdtar or an external recovery executable.

## Implemented boundaries

| Format | Acceptance evidence | Important exclusions |
| --- | --- | --- |
| ZIP | End record, bounded central directory, matching local records, names, recorded CRCs and sizes, nonoverlapping contiguous member extents; signed/unsigned descriptors and ZIP64 fields | Split/encrypted archives, missing central/end records, inconsistent fields, gaps, overlapping members; payload CRC is not independently calculated by the app |
| DOCX | Valid ZIP plus package office-document relationship, non-macro Word main content type, document root/namespace; required stored/deflated XML parts checked for exact length and CRC | External main-part targets, DTDs, unsupported compression or oversized required metadata; no Office schema/rendering validation |
| RAR4 / RAR5 | Header CRCs, finite declared data spans, supported header fields and an explicit final block | Split/continued volumes, encryption, missing ends and some legacy RAR4 service/comment layouts; no general payload decompression or payload CRC verification |

Archive candidates are capped at 16 GiB. ZIP has at most 100,000 entries, a 64 MiB central directory and 16 MiB aggregate names. Required DOCX metadata is capped at 1 MiB per part, at most three parts, with XML depth/node limits and no external requests. Unsupported DOCX metadata leaves a valid ZIP classified as ZIP. RAR has at most 100,000 records and an effective 1 MiB per-header read cap. These bounds may reject valid unusual files.

ZIP is found from its end-directory signature, then its beginning is derived from checked directory offsets. This accommodates descriptor-based writers without searching blindly for a descriptor inside compressed content. A ZIP filter includes validated DOCX; a DOCX-only filter excludes generic ZIP. A complete archive is emitted only after its required end metadata is found. Other format results may arrive earlier.

The [versioned JSON registry](../crates/recovery-core/src/formats.json) is embedded at build time. It supports bounded offset/magic rules and known validator identifiers, with strict validation of IDs, aliases and limits. It is not a runtime plugin mechanism or UI file importer. A regression exercises the maximum allowed 1,024-byte offset plus 64-byte magic across a scan-window boundary while respecting the source reader's 1 MiB buffer cap.

## Executed checks

Twenty archive integration tests cover exact recovery, ZIP32/ZIP64 descriptors, DOCX classification versus names-only/external/DTD impostors, RAR4/5 header corruption and unsupported flags, truncated endings, descriptor/local/central inconsistency, extreme offsets, embedded false end signatures, adjacent archives, scan ranges, cross-window endings, streaming results/cancellation and bounded diagnostics. Deterministic mutation tests exercise 384 changed archives and assert safe rejection or source-bounded extents.

Nine positive fixtures recover **2,955 bytes exactly**. Every recovered file's SHA-256 matches its original. Python's standard-library `zipfile.testzip()` independently decodes and CRC-checks the seven ZIP/DOCX fixtures. macOS `bsdtar` independently extracts the stored member from both RAR fixtures and matches its original bytes. This verifies these stored RAR cases, not all RAR compression methods. Machine-readable hashes and tool information are in [archive-recovery-evidence.json](archive-recovery-evidence.json).

Fixtures and their reproducible generator are in [tests/fixtures/archives](../crates/recovery-core/tests/fixtures/archives). They contain original synthetic content, under the project license. ZIP/DOCX are produced with Python's standard ZIP writer, plus explicit documented ZIP64 framing; RAR fixtures encode documented stored-member framing. CI needs only the checked-in small files and Rust tests. Generator and independent validation commands are local development tools:

```sh
python3 crates/recovery-core/tests/fixtures/archives/generate.py
cargo test -p recovery-core --test archives --locked
# Optional: save the actual recovered bytes for independent inspection.
MEMORY_REGAINER_ARCHIVE_EXPORT_DIR=/absolute/scratch/recovered \
  cargo test -p recovery-core --test archives \
  classic_streaming_unsigned_zip64_docx_and_rar_roundtrip_exactly --locked
bsdtar -xOf /absolute/scratch/recovered/stored-rar4.rar memory.txt
bsdtar -xOf /absolute/scratch/recovered/stored-rar5.rar memory.txt
```

The final local integrated checks reported 125 Rust tests and 54 frontend tests passing. Native Windows/macOS outcomes are recorded in [VALIDATION.md](VALIDATION.md) when executed; no physical-USB qualification is inferred from these fixtures.

## Local dispatch benchmark

A 128 MiB regular image contains the repeating bytes `11 22 33 44 66 77 88 99` in hexadecimal. These contain none of the enabled signature starting bytes. The same file was scanned three times by release-mode builds of the old v0.1.3 core and the current v0.2.0 core. Both completed the full 134,217,728-byte range with zero results. The SHA-256 and individual runs are recorded in the JSON evidence.

| Build | Three elapsed times | Median |
| --- | --- | --- |
| Previous dispatcher | 266 / 262 / 262 ms | 262 ms |
| Registry/first-byte dispatcher | 219 / 202 / 199 ms | 202 ms |

This final local run reduced median elapsed time by about 23%, or about 1.30 times the scan throughput. An earlier run on the same fixture differed, illustrating timing sensitivity. The test measures warm local no-signature dispatch overhead; it does not measure damaged-device reads, USB hardware, metadata traversal or archive-validation cost, and cannot explain a particular ten-minute device scan. Full-range scanning still reads the source, while actual read counters and candidate diagnostics expose what the running job has done.

## Primary format references

ZIP field layouts, descriptor rules and ZIP64 extension records follow [PKWARE's APPNOTE](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT). The parser uses central metadata as the authority for member boundaries and verifies its consistency against local records.

RAR5 framing, variable-length integers, header checksums and record flags follow [RARLab's technical note](https://www.rarlab.com/technote.htm). Legacy RAR4 field definitions were checked against RARLab's official [UnRAR source distribution](https://www.rarlab.com/rar_add.htm). No UnRAR implementation, proprietary code or binaries are copied or linked into the app.

DOCX package relationships and main-document structure are based on Microsoft's [Open XML package documentation](https://learn.microsoft.com/en-us/office/open-xml/general/how-to-create-a-package). This implementation performs limited classification, not complete document validation.

CGSecurity's [PhotoRec workflow](https://www.cgsecurity.org/wiki/PhotoRec_Step_By_Step) and [custom-signature guidance](https://www.cgsecurity.org/wiki/Add_your_own_extension_to_PhotoRec) informed the separation of signature configuration, validated boundaries and visible ongoing results. Memory Regainer does not bundle PhotoRec/TestDisk or claim equivalent recovery coverage.
