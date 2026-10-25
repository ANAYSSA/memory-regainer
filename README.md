# Memory Regainer

**Your files. Your memories. Recovered.**

An independent, offline desktop file-recovery application built with Rust, Tauri 2, React and TypeScript. It has no subscriptions, activation, login, advertisements, telemetry, watermark or paid recovery quota.

**Early prerelease:** this is a working foundation with deliberately limited recovery coverage. It is not a substitute for a mature forensic suite or professional handling of physically failing storage. Overwritten bytes, SSD TRIM and inaccessible encrypted data cannot be recovered by this software.

**По-русски:** приложение работает локально, поддерживает русский и английский интерфейс. Начните с копии образа диска. Быстрое сканирование ищет удалённые записи FAT и ограниченный набор записей exFAT; глубокое — семь типов сигнатур. Для exFAT поддерживаются целые непрерывные файлы на чистом томе с одной FAT, если bitmap отмечает их кластеры свободными. Восстановление NTFS, ext4 и APFS по метаданным пока не реализовано. Сохраняйте результат на здоровый отдельный носитель.

[Download EXE / DMG releases](https://github.com/ANAYSSA/memory-regainer/releases) · [Validation evidence](docs/VALIDATION.md)

## Get started

1. Stop writing to the affected storage. Prefer a raw disk image created on healthy storage.
2. Open the image in Memory Regainer, or select an accessible physical device. No automatic privilege elevation is performed.
3. Choose **Quick Scan** for supported FAT/exFAT deleted directory records or **Deep Scan** for the implemented file signatures. **Advanced Scan** adds an explicit byte range.
4. Review evidence, notes and previews. Results are populated from actual source bytes; counts alone do not establish successful recovery.
5. Select files and choose a healthy destination. The app writes into a new recovery folder and verifies copied bytes using SHA-256 read-back.
6. Inspect `recovery-report.json`. A verified copy establishes copy integrity; it does not prove that pre-deletion bytes were intact.

The application is designed for Windows 10/11 x64 and macOS Apple Silicon / Intel. Native installer builds and automated tests are defined in [GitHub Actions](.github/workflows/release.yml). Build completion, interactive installer testing and raw-device qualification are different checks: see [validation](docs/VALIDATION.md) for actual evidence. Linux source support is secondary and has no packaged release in this milestone. Windows ARM64 is not qualified.

Windows requires the Microsoft WebView2 runtime; the installer can obtain it if absent. Recovery itself works offline.

Unsigned installers can trigger Windows SmartScreen or macOS Gatekeeper. macOS builds are not Developer ID signed or notarized. Do not bypass security prompts for a build you have not chosen to trust; compare its SHA-256 checksum and source revision.

## What this version does

| Area | Current scope |
| --- | --- |
| Recovery engine | Real read-only reads, bounded parser buffers, checked byte extents, cancellation |
| Partitions | MBR primary entries; GPT primary header/table CRC checks; 512/4096-byte GPT sectors; protective-MBR handling |
| FAT12/16/32 | Deleted directory-record recovery with explicit confidence and reconstruction notes; see [feature matrix](docs/FEATURES.md) |
| exFAT | Narrow metadata undelete: clean single-FAT revision 1.00 volumes, intact inactive entry sets, Unicode names, fully initialized contiguous NoFatChain files whose clusters are currently free |
| Signature carving | JPEG, PNG, PDF, GIF, ZIP, BMP and WAV boundary handlers, with type-specific limits and structural checks |
| Other filesystems | Several signatures detected; no claim of metadata undelete for NTFS, ext4, APFS, HFS+, Btrfs, XFS or ReFS |
| Desktop | Actual engine progress, cancellation, searchable/filtered virtualized result table, selection and recovery reports; Russian/English and theme controls |
| Preview | Bounded raster image, UTF-8 text and hex previews; no executable or macro execution |
| Imaging | Sequential raw image acquisition, cancellation, first-read-error stop, SHA-256 read-back and acquisition report |
| Sessions | Export scan metadata; reopen an image session by validating its source fingerprint and running the scan again |

Results are delivered after a scan completes; progress counters update during scanning. Interrupted scan checkpoints and pause/resume are not implemented. The result index is held in memory, so extremely large result sets remain a scaling limit despite a virtualized UI. No multi-terabyte throughput or maximum-memory claim is made.

Read the full [feature matrix and limits](docs/FEATURES.md), [safety notes](docs/SAFETY.md) and [implementation progress](PROGRESS.md).

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

`crates/recovery-core` contains the reusable parsers and reconstruction code. `apps/desktop/src-tauri` owns source registration, OS discovery, destination checks, background jobs and native dialogs. `apps/desktop/src` contains the interface. Build, fixture and license tools live in `scripts`.

GPL-3.0-or-later. See [LICENSE](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md), [legal decisions](LEGAL.md), [initial name check](TRADEMARKS.md), [privacy](PRIVACY.md), [security](SECURITY.md) and [contributing](CONTRIBUTING.md). Memory Regainer is not affiliated with commercial recovery software vendors. Recovered user files do not become GPL-licensed merely because this app recovered them.
