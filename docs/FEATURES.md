# Feature matrix

Status describes implementation scope. See [VALIDATION.md](VALIDATION.md) for tested evidence. No row promises recovery after overwrite, TRIM or encryption without legitimate access.

| Filesystem / layout | Detection | Metadata undelete | Names / hierarchy | Raw carving |
| --- | --- | --- | --- | --- |
| FAT12 / FAT16 / FAT32 | Boot-parameter validation | Free-cluster contiguous candidates only, normally low confidence | Deleted-record evidence where available; missing/ambiguous name characters and paths must be treated as uncertain | Implemented handlers below |
| exFAT | Signature only | Unsupported | Unsupported | Implemented handlers only |
| NTFS | Signature only | Unsupported | Unsupported | Implemented handlers only |
| ext2 / ext3 / ext4 | Shared ext signature only; not a full feature classification | Unsupported | Unsupported | Implemented handlers only |
| APFS | Container magic only | Unsupported | Unsupported; no snapshots or encryption support | Accessible raw bytes only |
| HFS+ | Volume magic only | Unsupported | Unsupported | Accessible raw bytes only |
| Btrfs / XFS / ReFS | Signature only | Unsupported | Unsupported | Accessible raw bytes only |
| Unknown / formatted source | No filesystem requirement | Unsupported | Generated names | Implemented handlers only |

Metadata-based recovery can preserve extensions for types that have no dedicated carver. This does not imply a validated format-specific parser. FAT candidates assume contiguous content and require every assumed cluster to be currently free. Records whose clusters are allocated are skipped. Deleted allocation chains and fragmented deleted files are not reconstructed. Surviving live-directory chains are traversed to locate deleted records; deleted directories are not reconstructed. Inspect each result's notes.

## Scan modes

- Quick inspects supported FAT metadata. Its byte counter measures processed work, not a claim that every source sector was read.
- Deep performs metadata inspection and sequential signature scanning. It operates on real bytes and stops on read errors.
- Advanced applies a start/end byte range. End offsets are exclusive. Reads stay inside the selected range. For a partial-source range, global partition tables are skipped; FAT metadata recovery requires a complete FAT volume beginning at the selected start. It does not repair partition tables or recover filesystem features absent from Quick/Deep.

The MBR parser covers primary partitions and reports unsupported extended/logical chains. GPT validates primary header/table CRCs and bounds. There is no damaged backup-GPT repair, lost-partition forensic search or partition write operation.

## Carving

The registry contains seven handlers: `jpg`, `png`, `pdf`, `gif`, `zip`, `bmp`, `wav`. Each has a bounded boundary parser; confidence reflects the evidence that parser actually checks. Image/PDF candidates are limited to 256 MiB; ZIP/WAV candidates to 1 GiB. These are defensive parser bounds, not paid recovery quotas. ZIP64, encrypted ZIP and ZIP data descriptors are not supported. Compressed pixels, archive payloads and PDF objects are not fully decoded. A complete-looking header/footer is not an authenticity or semantic-validity guarantee. ZIP can contain office documents, but the engine does not advertise each contained office format as an independently tested carver. Text and source code without reliable boundaries require metadata-based recovery.

MP4, MOV, MKV, audio formats other than WAV, camera RAW, HEIC, TIFF, RAR, 7z, SQLite and executable-specific carving are not implemented. PDF preview, audio playback, video playback and general document parsing are not implemented.

## Operations and limitations

| Capability | State / constraint |
| --- | --- |
| Source safety | Source handles are read-only; no format, repair or partition-write command |
| Raw devices | Discovery and permission reporting; actual access depends on the OS. Not qualified against real damaged media |
| Destination safety | Separate-physical-device checks for raw-source recovery; fail closed if mapping cannot be established. Disk images are treated as existing independent copies |
| Output integrity | New recovery folder, sanitized names, temporary output, read-back SHA-256 and per-file status |
| Source identity | Size, modified time for images, and first/last 64 KiB fingerprint; **not a full-image integrity hash** |
| Progress | Actual engine counters; results become available at scan completion |
| Pause / scan checkpoint resume | Unsupported; cancellation is available |
| Saved sessions | Source/options manifest; image session reopening re-runs the scan rather than resuming an interrupted scan |
| Imaging | Sequential full-source image; stop on first error; no bad-sector retries, mapfile resume or ddrescue backend |
| Large result sets | Virtualized rendering, but in-memory backend result index and JSON snapshots; no million-result memory qualification |
| Forensic certification | None; source locking, hardware write blocking and chain-of-custody certification are outside this release |
