# Feature matrix

Status describes implementation scope. See [VALIDATION.md](VALIDATION.md) for tested evidence. No row promises recovery after overwrite, TRIM or encryption without legitimate access.

| Filesystem / layout | Detection | Metadata recovery | Names / hierarchy | Raw carving |
| --- | --- | --- | --- | --- |
| FAT12 / FAT16 / FAT32 | Boot-parameter validation | Existing allocation chains, including fragmented files; deleted files require currently free contiguous clusters | Live hierarchy and bounded tentative deleted-directory paths; missing or ambiguous characters remain uncertain | Implemented handlers below |
| exFAT 1.00 | Main boot checksum and bounded geometry validation | Existing allocated streams; intact inactive NoFatChain streams require currently free clusters on clean single-FAT volumes | Unicode names, live hierarchy and bounded contiguous deleted-directory candidates | Implemented handlers only |
| NTFS | Signature only | Unsupported | Unsupported | Implemented handlers only |
| ext2 / ext3 / ext4 | Shared ext signature only; not a full feature classification | Unsupported | Unsupported | Implemented handlers only |
| APFS | Container magic only | Unsupported | Unsupported; no snapshots or encryption support | Accessible raw bytes only |
| HFS+ | Volume magic only | Unsupported | Unsupported | Accessible raw bytes only |
| Btrfs / XFS / ReFS | Signature only | Unsupported | Unsupported | Accessible raw bytes only |
| Unknown / formatted source | No filesystem requirement | Unsupported | Generated names | Implemented handlers only |

Metadata-based recovery can preserve extensions for types that have no dedicated carver; it does not validate their file contents. Existing FAT files follow the current allocation chain, including fragmented files. Deleted FAT files still assume contiguous content and require every assumed cluster to be currently free. Deleted allocation chains and arbitrary fragmented deleted files are not reconstructed.

FAT deleted-directory candidates require a free first cluster, consistent `.` and `..` records and plausible entries. Only that first cluster is inspected; the former directory chain is not guessed. Active-looking children under deleted ancestry remain tentative deleted candidates, with explicit evidence notes. Paths are evidence, not a guarantee of the former complete hierarchy.

exFAT recovery independently follows the public Microsoft specification. It validates the main boot-region checksum, geometry, metadata FAT-chain bounds and current allocation bitmap. Existing streams use allocated FAT chains or NoFatChain extents. Deleted File/Stream/Name sets must pass a checksum after reconstructing their former InUse bits. This is a recovery heuristic, not authoritative filesystem metadata. Deleted files require fully initialized contiguous NoFatChain streams and free content clusters. Deleted directories additionally require preserved contiguous length of at most 256 MiB and all clusters free; their descendants inherit tentative deleted ancestry.

Dirty/media-failure volumes, TexFAT/two-FAT volumes, fragmented deleted files, partly initialized streams and empty-file undelete are unsupported. NameHash is not checked against the up-case table, and the parser does not validate the backup boot region or certify full filesystem consistency. Coalesced file extents are capped at one million. Freed bytes may already have been overwritten. Tests use generated fixtures; these are not qualification of every real device or deletion pattern.

## Scanning and core modes

The desktop has one **Scan** action combining metadata and content searching. Collapsed **Expert options** offers optional format filters and a byte range. The following core mode names remain for APIs and saved sessions; they are not three competing primary UI buttons.

- Quick inspects supported FAT/exFAT metadata. Its byte counter measures processed work, not a claim that every source sector was read.
- Deep performs metadata inspection and sequential signature scanning. It operates on real bytes and stops on read errors.
- Advanced applies a start/end byte range. End offsets are exclusive. Parsers and scan reads stay inside the selected range. v0.1.1 requires raw-device start and end offsets to align to the device logical sector size; use an image for arbitrary byte ranges. Preview, recovery and fingerprint reads may fetch enclosing sectors for hardware alignment, while exposing only requested bytes. For a partial-source range, global partition tables are skipped; FAT/exFAT metadata recovery requires a complete volume beginning at the selected start. It does not repair partition tables or recover filesystem features absent from Quick/Deep.

The MBR parser covers primary partitions and reports unsupported extended/logical chains. GPT validates primary header/table CRCs and bounds. There is no damaged backup-GPT repair, lost-partition forensic search or partition write operation.

## Carving

The versioned [format registry](../crates/recovery-core/src/formats.json) contains eleven categories: `jpg` (alias `jpeg`), `png`, `pdf`, `gif`, `zip`, `docx`, `rar`, `bmp`, `wav`, `mp4`, `mov`. The same catalog supplies format selection and carving signatures. It is bounded JSON, compiled into the application; changing it requires rebuilding. It can select existing validators and offset/magic values, not execute plugins or create an arbitrary new parser. Enabled signatures are compiled into first-byte buckets before scanning. Scanning still covers the selected byte range; this optimization is not a free-space map or source-wide skip algorithm.

Each validator has explicit work limits. Images/PDF are limited to 256 MiB, WAV to 1 GiB, archives to 16 GiB and video to 64 TiB, with the tighter structural limits below. These are defensive bounds, not paid quotas. Compressed pixels, PDF objects and ordinary archive payloads are not fully decoded; structural acceptance cannot prove intact original content.

ZIP requires matching local/central records, bounded payload spans and an end directory. Signed or unsigned data descriptors and ZIP64 metadata are supported. Split/encrypted archives, gaps or overlapping members, absent central records and incomplete end records are rejected. Limits include 100,000 entries, a 64 MiB central directory and 16 MiB aggregate names. Ordinary ZIP checks agreement of recorded CRC fields, not payload CRC by decompression. A ZIP filter also includes DOCX; a DOCX-only filter excludes generic ZIP.

DOCX additionally requires bounded package relationships, the non-macro Word main content type and a matching document root/namespace. Required XML parts must be stored or deflated, have valid CRCs, and remain within 1 MiB each (at most three parts). External main-part targets and DTDs are rejected. This is package classification, not full Office schema/content validation; unsupported metadata leaves a structurally valid archive classified as ZIP.

RAR4/RAR5 require header checksums, finite declared data extents and an explicit end record. Volume continuations and encrypted headers/files are rejected; archive payloads are not decompressed or CRC-verified. Header reads are effectively limited to 1 MiB and record count to 100,000. Unusual legacy RAR4 service/comment layouts may be rejected. See [archive validation](ARCHIVE_RECOVERY_VALIDATION.md) for exact tests and local independent extraction evidence.

MKV, MXF, MTS, audio formats other than WAV, camera RAW, HEIC, TIFF, 7z, SQLite and executable-specific carving are not implemented. PDF preview, audio playback, video playback and general document rendering are not implemented. Text/source code without reliable boundaries requires metadata-based recovery.

## MP4/MOV content recovery (v0.1.3)

Deep and Advanced scans recognize classic self-contained MP4/MOV containers stored in one contiguous range on the source. Movie metadata may precede or follow media data. The parser requires an intact `ftyp`, `moov`, a video track and explicitly sized `mdat` data. It validates box bounds, sample descriptions, local data references, sample counts/sizes, sample-to-chunk mappings and chunk offsets against media-data ranges. Extended-size boxes and 64-bit chunk offsets use checked 64-bit arithmetic; archive/WAV candidate limits do not apply to video.

Video metadata safety limits are 512 MiB per movie box, 100,000 parsed boxes, 4,096 top-level boxes, 64 tracks, 256 media-data atoms, and 64 data references or sample descriptions per track. Per-track table limits are 100 million samples, 20 million chunks and one million runs. Tables use 64 KiB buffers. Chunk ranges must be ordered and nonoverlapping within each track. These are parser work limits, not a paid recovery quota.

These checks do not decode video or audio in the running app. Overwritten media payload can still have consistent container metadata. Arbitrarily fragmented files, `moof`/`mvex` fragmented MP4, missing `moov`, compressed movie metadata, reference movies, encrypted tracks, zero-size media-data boxes and legacy MOV without `ftyp` are unsupported. Recovered video has a generated name; use a local player after saving to a different healthy destination. Test fixtures, exact hashes, local decoding evidence and remaining qualification limits are in [VIDEO_RECOVERY_VALIDATION.md](VIDEO_RECOVERY_VALIDATION.md).

## Operations and limitations

| Capability | State / constraint |
| --- | --- |
| Source safety | Source handles are read-only; no format, repair or partition-write command |
| Raw devices | Discovery and permission reporting; v0.1.1 Windows requests standard UAC elevation. Actual access depends on the OS; not qualified against real damaged media |
| Destination safety | Separate-physical-device checks for raw-source recovery; fail closed if mapping cannot be established. Disk images are treated as existing independent copies |
| Output integrity | New recovery folder, sanitized hierarchy, collision-safe paths, temporary output, read-back SHA-256 and per-file status |
| Source identity | Size, modified time for images, and first/last 64 KiB fingerprint; **not a full-image integrity hash** |
| Progress | v0.1.1 exposes stage, elapsed time, actual counters, measured rate and conditional ETA; indeterminate metadata/preparation stages do not imply a source-wide percentage. Accepted results stream through a separate paged API during scanning; bounded candidate/rejection counters explain carver activity |
| Pause / scan checkpoint resume | Unsupported; cooperative cancellation can wait for a blocked OS read to return |
| Idle sleep protection | v0.1.1 requests it during Windows/macOS jobs and reports success/failure; manual sleep, lid closure, shutdown and unplugging remain possible |
| Saved sessions | Source/options manifest; image session reopening re-runs the scan rather than resuming an interrupted scan |
| Imaging | Sequential full-source image; stop on first error; no bad-sector retries, mapfile resume or ddrescue backend |
| Large result sets | Paged result transfer and virtualized rendering; both backend/frontend catalogs remain in memory, without million-result qualification |
| Forensic certification | None; source locking, hardware write blocking and chain-of-custody certification are outside this release |

## v0.1.2 result presentation and FAT error isolation

Displayed capacity units are decimal KB/MB/GB/TB; raw offsets remain byte counts. Quick Scan completion explicitly describes metadata-only coverage and positive durations below one second are shown as less than one second. Partition inventory and general method assumptions are separate from operation warnings. FAT32 invalid root clusters are reported. A mirrored-table conflict affecting one deleted candidate or one directory continuation does not erase already found independent candidates; skipped records or ranges are reported. Device I/O failures and cancellation remain fatal. That historical update did not add deleted-directory traversal or video-fragment reconstruction. v0.2.0 adds only the bounded directory cases documented above; arbitrary video-fragment reconstruction remains unsupported.

## v0.2.0 result handling

The interface offers location and type trees, name/path/type/deletion-status filtering and sorting. Unknown carved paths stay separate from recorded metadata paths. Fully accepted candidates can be recovered while a scan continues. Cancellation retains accepted results; recovery checks source identity again, and a failed identity check disables recovery. A blocked OS read can still delay cancellation. Candidate diagnostics count detected signatures, accepted/rejected candidates and a bounded set of rejection categories; they do not predict a recovery success rate.

## v0.2.1 directory and discovery corrections

Invalid directory candidates no longer reserve a visited cluster before validation. Deleted FAT special names with an E5 first byte are accepted only with consistent surviving names, attributes, zero special-record sizes, self/parent pointers and a free first cluster; evidence remains tentative. Available output basenames remain unchanged after safe sanitization, with suffixes only for real collisions. Empty directories are not independent results. See [directory recovery details](DIRECTORY_RECOVERY.md).

Device discovery starts automatically, retries while idle and on return to the window, and displays loading/error states separately from scan errors. These lifecycle fixes have synthetic frontend coverage; they do not establish the cause of a particular Windows enumeration delay.
