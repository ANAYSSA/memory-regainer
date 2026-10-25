# Memory Regainer v0.1.1

v0.1.1 focuses on Windows raw-device access, visible progress and protection from automatic idle sleep during recovery work. It follows a report of a USB scan appearing stalled and returning OS error 1 after sleep. That device failure has not been reproduced; this release does not claim to establish its exact cause.

## Changes

- Windows requests administrator access through the standard UAC prompt at application launch. The whole application runs elevated, including image-only work; no access-control bypass is used.
- Source opening, fingerprints and destination preparation run in the background, with explicit preparation and verification stages. Underlying OS errors retain operation details.
- Progress distinguishes stages with known byte totals from indeterminate metadata/device work. The UI shows elapsed time and measured activity; ETA appears only with sufficient recent same-stage progress and disappears when it cannot be supported.
- Active scans, recovery and imaging request idle system sleep protection on Windows/macOS and release it afterward. The UI reports whether the request succeeded. The display may dim; manual sleep, lid closure, shutdown, unplugging and battery loss can still interrupt work.
- A shared raw-device reader uses device-specific length/sector queries, aligned reads and a bounded 512 KiB data cache. Advanced raw scans require sector-aligned boundaries; image scans retain arbitrary byte ranges. Preview and recovery expose only requested content bytes.
- Project-owned documentation and release descriptions are in English. The application retains English and Russian UI options.

## Validation status

The latest local run passed 51 Rust and 38 frontend tests (89 total); strict workspace Clippy passed. A native macOS read-only disposable device probe passed, and a separate power probe confirmed that the idle-sleep assertion appeared while active and disappeared on completion. Post-review formatting, Clippy and Rust tests passed; native installer builds remain pending; exact results belong in [VALIDATION.md](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.1/docs/VALIDATION.md). Real Windows USB sleep/wake, disconnect/reconnect and damaged-device qualification remain outstanding.

Cancellation is cooperative; a blocked OS read may delay stopping. There is no resumable scan checkpoint or automatic recovery after a device disappears. An elapsed timer shows that the UI is responsive, not that the disk is delivering new bytes.

## Recovery scope and downloads

The recovery scope remains image-based recovery, FAT metadata undelete, narrow exFAT metadata undelete and seven bounded carvers: JPEG, PNG, PDF, GIF, ZIP, BMP and WAV. exFAT requires clean single-FAT revision 1.00 volumes and intact fully initialized contiguous deleted streams with currently free clusters. NTFS/APFS/ext4 metadata recovery, generic fragmented-file reconstruction and MP4/MOV carving are not implemented. See the [feature matrix](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.1/docs/FEATURES.md).

Native packages target Windows x64 EXE, macOS Apple Silicon DMG and macOS Intel DMG. Windows WebView2 installation may require internet if the runtime is absent; recovery works offline. Installers are unsigned on Windows and are not Apple Developer ID signed or notarized. Release assets include matching source and SHA-256 checksums when publication succeeds.

Use a separate healthy destination and prefer a disk image. Overwritten bytes, TRIM and inaccessible encrypted data cannot be recovered by this application.

## Research and independence

Further bounded static inspection of supplied Recoverit modules found direct power-management and asynchronous-dispatch API references. Those references do not reveal the recovery algorithms or prove their runtime behavior. Official PhotoRec/TestDisk source and GNU ddrescue documentation informed a comparison of device geometry, cooperative cancellation, phased progress and persistent acquisition state. No proprietary code, assets or binaries were incorporated; neither open-source recovery engine is bundled. See [research details](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.1/docs/RECOVERY_IO_RESEARCH.md).

GPL-3.0-or-later, with dependency origins and notices in the source tree. No subscription, activation, telemetry, watermark or paid recovery limit. The documented Linux dependency audit warnings remain relevant; see [AUDIT.md](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.1/docs/AUDIT.md).
