# Memory Regainer v0.1.2

This update clarifies Quick Scan results and byte units, preserves valid FAT results when an independent record is damaged, and records a deeper static analysis of Recoverit. It does **not** add Recoverit-equivalent SD-card or fragmented-video recovery.

## Changes

- Sizes now use decimal **KB, MB, GB and TB** (1 GB = 1,000,000,000 bytes). Stored byte offsets and recovered content are unchanged. A capacity previously shown as about 115 GiB will therefore appear as about 124 GB.
- A completed operation taking less than one second displays **Less than 1 s**, rather than 00:00. Completed Quick Scan explicitly says that it searched filesystem metadata, not the entire source for file signatures.
- Detected partitions and general FAT/exFAT method limitations appear in a neutral information section. Actual skipped-data or damaged-structure warnings remain warnings; read failures remain errors.
- FAT32 root-cluster values outside the valid data-cluster range are rejected and reported. A damaged FAT record or directory-chain link no longer discards already found independent candidates. Read errors and cancellation still stop the scan.
- The English research report now includes verified installer identity and selected instruction-level scan, SD-card profile and video-processing call paths from the supplied Windows binaries and an official signed macOS payload. No proprietary binary, source code or asset is bundled.

## Validation

Local checks passed 56 Rust tests (49 core and 7 desktop) and 38 frontend tests: **94 total**. The release workflow runs tests and builds Windows x64 EXE, macOS Apple Silicon DMG and macOS Intel DMG. Final native outcomes and downloaded-asset checks are recorded after publication in [validation](https://github.com/ANAYSSA/memory-regainer/blob/main/docs/VALIDATION.md).

## Scope

A Quick Scan reading only a small amount of metadata is not evidence that the entire device has been searched, nor a claim that every recoverable file was found. The two messages reported in v0.1.1—FAT method assumptions and a FAT32 partition description—were informational, not a device failure.

Content carving remains limited to JPEG, PNG, PDF, GIF, ZIP, BMP and WAV. MP4/MOV/MXF/MTS content recovery and video-fragment reconstruction are **not implemented**. A supported FAT/exFAT deleted record can identify another file type, but this does not establish playable-video recovery. The research is a bounded reverse analysis, not a complete reconstruction of all Recoverit algorithms or proof of matching recovery results. [Read the detailed findings](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.2/docs/RECOVERIT_ARCHITECTURE_REVIEW.md).

Windows continues to request administrator permission at launch. Idle system sleep protection remains active during scans, recovery and imaging; it cannot override manual sleep, lid closure, shutdown or power loss. Installers remain unsigned on Windows and without Apple Developer ID signing or notarization on macOS. Recovery works offline; installing missing WebView2 may require internet.

The release includes matching source, license notices and SHA-256 checksums. GPL-3.0-or-later. See the [feature matrix](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.2/docs/FEATURES.md) for exact supported recovery cases.
