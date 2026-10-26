# Memory Regainer

**Your files. Your memories. Recovered.**

An independent, offline desktop file-recovery application built with Rust, Tauri 2, React and TypeScript. It has no subscriptions, activation, login, advertisements, telemetry, watermark or paid recovery quota.

**Early prerelease:** this is a working foundation with deliberately limited recovery coverage. It is not a substitute for a mature forensic suite or professional handling of physically failing storage. Overwritten bytes, SSD TRIM and inaccessible encrypted data cannot be recovered by this software.

The interface supports English and Russian. The single Scan action checks supported FAT/exFAT metadata, including existing files and cautious deleted-directory candidates, then searches eleven content-format categories including MP4/MOV, ZIP, DOCX and RAR. Metadata undelete for NTFS, ext4 and APFS is not implemented. Start with a disk image and save recovered files to separate healthy storage.

[Download EXE / DMG releases](https://github.com/ANAYSSA/memory-regainer/releases) · [Validation evidence](docs/VALIDATION.md)

## Get started

1. Stop writing to the affected storage. Prefer a raw disk image created on healthy storage.
2. Connected drives appear automatically when the app opens. Open an image or select an accessible device; discovery also retries while idle and when you return to the window. Windows requests administrator access through the standard UAC prompt at application launch. macOS permissions remain controlled by the OS.
3. Click **Scan** to inspect filesystem metadata and search file contents. **Expert options** expands optional format filters and byte ranges. Leave format boxes unchecked for all supported types and the range empty for the entire source.
4. Review results as they arrive, grouped by location or file type. Check deletion status, paths, evidence, notes and previews; counts alone do not establish successful recovery.
5. Select files or a folder's files and choose a healthy destination. The app writes into a new recovery folder and verifies copied bytes using SHA-256 read-back. Available metadata names and folder paths are preserved after safe filename sanitization; only collisions receive a numeric suffix.
6. Inspect `recovery-report.json`. A verified copy establishes copy integrity; it does not prove that pre-deletion bytes were intact.

The application is designed for Windows 10/11 x64 and macOS Apple Silicon / Intel. Native installer builds and automated tests are defined in [GitHub Actions](.github/workflows/release.yml). Build completion, interactive installer testing and raw-device qualification are different checks: see [validation](docs/VALIDATION.md) for actual evidence. Linux source support is secondary and has no packaged release in this milestone. Windows ARM64 is not qualified.

Windows requires the Microsoft WebView2 runtime; the installer can obtain it if absent. Recovery itself works offline. The v0.1.1 Windows application manifest requests administrator access for raw-device reads; this elevates the whole application, including image-only work. No access-control bypass is provided.

Unsigned installers can trigger Windows SmartScreen or macOS Gatekeeper. macOS builds are not Developer ID signed or notarized. Do not bypass security prompts for a build you have not chosen to trust; compare its SHA-256 checksum and source revision.

## What this version does

| Area | Current scope |
| --- | --- |
| Recovery engine | Real read-only reads, bounded parser buffers, checked byte extents, cancellation |
| Partitions | MBR primary entries; GPT primary header/table CRC checks; 512/4096-byte GPT sectors; protective-MBR handling |
| FAT12/16/32 | Existing files from allocation chains, including fragmented files; cautious free-contiguous deleted files and bounded deleted-directory traversal |
| exFAT | Clean single-FAT revision 1.00 volumes: existing files, narrow free-contiguous undelete and bounded contiguous deleted-directory traversal; Unicode names and evidence paths |
| Signature carving | JPEG, PNG, PDF, GIF, ZIP, DOCX, RAR4/RAR5, BMP, WAV, MP4 and MOV, with type-specific structural checks and a versioned data-only format registry |
| MP4/MOV video | Classic self-contained, contiguous files with an intact file-type box, movie metadata and explicitly sized media data; checks video tracks, sample tables and chunk ranges. Fragment reconstruction, missing-metadata repair and runtime codec decoding are unsupported |
| Other filesystems | Several signatures detected; no claim of metadata undelete for NTFS, ext4, APFS, HFS+, Btrfs, XFS or ReFS |
| Desktop | Live paged result delivery, location/type trees, path/status filters, candidate diagnostics, cancellation and recovery reports; Russian/English and theme controls |
| Preview | Bounded raster image, UTF-8 text and hex previews; no executable or macro execution |
| Imaging | Sequential raw image acquisition, cancellation, first-read-error stop, SHA-256 read-back and acquisition report |
| Sessions | Export scan metadata; reopen an image session by validating its source fingerprint and running the scan again |

Validated results arrive during scanning through a separate paged API. They can be selected and recovered while scanning continues; accepted results are retained on cancellation, subject to source revalidation. Paths from metadata are preserved through sanitized, collision-safe output folders. Explicit preparation, metadata, carving, verification and cancellation stages show elapsed time, actual bytes read, measured transfer rates and an estimate only when enough progress samples exist. Metadata traversal and device opening can have no meaningful percentage. Progress describes actual work; it is not a recovery-success percentage. Interrupted scan checkpoints and pause/resume are not implemented. The backend and frontend result indexes remain in memory, so extremely large result sets remain a scaling limit despite paged transfer and a virtualized UI. No multi-terabyte throughput or maximum-memory claim is made.

While a scan, recovery or image acquisition is active, v0.1.1 requests protection from automatic idle system sleep on Windows and macOS and shows whether the request succeeded. The display may dim. Manual sleep, lid closure, shutdown, disconnection and battery loss can still interrupt work. Cancellation is cooperative: a blocked device read may delay completion. There are no resumable scan checkpoints. See [validation](docs/VALIDATION.md) for executed checks; real damaged-device and sleep/wake qualification remains outstanding.

Files found only by content scanning receive generated names and are grouped by type. Their contents do not establish the original filename or folder. When filesystem records survive, metadata scanning can retain names and paths; damaged or overwritten records may prevent this even when the content is recoverable. See [directory recovery evidence and limits](docs/DIRECTORY_RECOVERY.md). Save MP4/MOV files to healthy storage and check them in a local video player. Structural consistency and a matching copy hash cannot prove that freed media bytes were not overwritten. See [video validation](docs/VIDEO_RECOVERY_VALIDATION.md) for the exact tested cases.

ZIP supports bounded central-directory validation, data descriptors and ZIP64. DOCX classification checks bounded package metadata; RAR4/RAR5 check headers, declared bounds and an explicit end record. Archive payloads are not generally decoded by the app. See [archive validation and limits](docs/ARCHIVE_RECOVERY_VALIDATION.md).

Read the full [feature matrix and limits](docs/FEATURES.md), [safety notes](docs/SAFETY.md), [implementation progress](PROGRESS.md) and [recovery architecture research](docs/RECOVERY_IO_RESEARCH.md).

## Build from source

Install a stable Rust toolchain, Node.js 22 or later, and [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/). On Windows use the MSVC toolchain and C++ build tools. macOS requires Apple command-line developer tools. Linux also requires WebKitGTK and related native development libraries.

```sh
git clone https://github.com/ANAYSSA/memory-regainer.git
cd memory-regainer/apps/desktop
npm ci
npm run tauri -- dev
```

Native release builds:

```sh
# macOS (run on a Mac)
npm run tauri -- build --bundles dmg

# Windows (run on Windows)
npm run tauri -- build --bundles nsis
```

The repository-root `target/release/bundle` directory holds a host build. Builds with an explicit target use `target/<target>/release/bundle`. The release workflow builds native Windows x64, macOS ARM64 and macOS Intel installers, retains checksums, and attaches matching source. A manual workflow run uploads build artifacts without publishing a release.

## Test

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cd apps/desktop
npm run build
npm test -- --run
npm run lint
```

Tests use synthetic images and temporary output directories. No test needs a personal physical disk. See [VALIDATION.md](docs/VALIDATION.md) for executed commands and results; the commands above are instructions, not a claim that every platform passed.

## Project and license

`crates/recovery-core` contains the reusable parsers and reconstruction code. Its [formats.json](crates/recovery-core/src/formats.json) is the single versioned signature/filter catalog; changes require rebuilding and can only select implemented validators, not load executable plugins. `apps/desktop/src-tauri` owns source registration, OS discovery, destination checks, background jobs and native dialogs. `apps/desktop/src` contains the interface. Build, fixture and license tools live in `scripts`.

GPL-3.0-or-later. See [LICENSE](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md), [legal decisions](LEGAL.md), [initial name check](TRADEMARKS.md), [privacy](PRIVACY.md), [security](SECURITY.md) and [contributing](CONTRIBUTING.md). Memory Regainer is not affiliated with commercial recovery software vendors. Recovered user files do not become GPL-licensed merely because this app recovered them.
