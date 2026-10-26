# Implementation progress

Updated: 26 October 2026. This record distinguishes shipped work, current changes and unqualified behavior. The application does not implement the full original specification.

## Released baseline: v0.1.0

[v0.1.0](https://github.com/ANAYSSA/memory-regainer/releases/tag/v0.1.0) was published from `2ea7d8a8683db758bc1d6eea5c1108908ff05089`. Windows x64 and macOS ARM64/Intel native builds and automated tests passed; published artifact hashes were verified. The release contains:

- An independent Rust core and Tauri/React desktop shell, with English/Russian UI, real scan jobs, previews, filters and virtualized results.
- Read-only image/device sources, checked extents, MBR/GPT discovery, FAT free-contiguous undelete, narrow exFAT undelete and seven bounded signature handlers.
- Recovery to new output folders, destination checks, read-back SHA-256 and per-file reports; sequential image acquisition and saved image sessions that rerun a scan.
- 32 core tests, 4 backend tests and 18 frontend tests. Native macOS GUI recovery, DMG launch, image creation and session reopening were exercised. Full evidence is in [VALIDATION.md](docs/VALIDATION.md).
- GPL-3.0-or-later licensing, 694 dependency records, license notices, privacy/security guidance and bounded architecture research. The documented five RustSec warnings concern Linux-only target dependencies; see [AUDIT.md](docs/AUDIT.md).

Task-owned v0.1.0 temporary images, verification downloads, build outputs and test caches were cleaned after publication. Supplied Recoverit files and unrelated user files were preserved. New v0.1.1 work generates separate temporary build/research data.

## Released update: v0.1.1

A Windows USB scan was reported as apparently stalled, followed by sleep and OS error 1. The old regular-file metadata call reproduced OS error 1 on a read-only Windows scratch VHD, and the new reader passed on the same device. The exact physical USB/sleep sequence has not been reproduced. The current work addresses concrete reader, responsiveness, progress and sleep-management weaknesses without claiming that this report proves their causal connection.

- [x] Move source opening/fingerprinting and destination preparation into background jobs so the UI can report their stage immediately.
- [x] Preserve original device errors with operation context instead of reducing them to a generic permissions message.
- [x] Add Windows `requireAdministrator` application manifest using standard UAC approval.
- [x] Add operation-scoped idle sleep requests on Windows/macOS with status reporting and release on job exit.
- [x] Add stage, elapsed-time and determinate/indeterminate progress fields; implement stage-aware frontend rate/ETA handling.
- [x] Complete shared raw-device reader geometry/alignment/cache changes and focused core tests.
- [x] Translate project-owned documentation into English and publish the English v0.1.0 release description. Preserve third-party notices verbatim and immutable historical source tags.
- [x] Extend static Recoverit import research and inspect primary PhotoRec/TestDisk source, GNU ddrescue documentation and Microsoft API contracts. No proprietary code or binaries added; see [RECOVERY_IO_RESEARCH.md](docs/RECOVERY_IO_RESEARCH.md).
- [x] Run final integrated Rust/frontend checks (89 tests), three native installer preflight builds, Windows VHD/manifest checks and macOS raw/power probes; record actual results in [VALIDATION.md](docs/VALIDATION.md).
- [x] Publish [v0.1.1](https://github.com/ANAYSSA/memory-regainer/releases/tag/v0.1.1) Windows/macOS installers with matching source/checksums, verify uploaded assets and clean task-owned temporary files.

The checked boxes above indicate implementation or documentation work, not Windows USB hardware qualification. The release's exact executed validation is recorded separately.

## Known limits

- No NTFS, ext4 or APFS metadata undelete. exFAT requires clean single-FAT revision 1.00 volumes; deleted streams require intact fully initialized contiguous content, with only bounded deleted-directory reconstruction.
- No generic fragmented-file reconstruction, video repair, encryption bypass, bootable environment or partition modification.
- No pause/resume checkpoints. Saved image sessions check a limited fingerprint and rerun the scan.
- Imaging stops on the first read error; no retry map or resumable ddrescue integration.
- Accepted results stream during scanning, but backend and frontend catalogs remain in memory. No multi-terabyte or million-result resource benchmark.
- Cancellation is cooperative and may wait for a blocked OS read. Idle protection does not override manual sleep, lid closure, shutdown, power loss or device removal.
- No Developer ID/Windows signing certificates or Apple notarization. Windows UAC is a permission request, not a publisher signature.
- Raw-device geometry, destination mapping, disconnects and sleep/wake need qualification on disposable Windows/macOS hardware.

## Next milestones

1. Qualify raw readers, UAC, destination safety, cancellation and sleep/disconnect behavior on disposable physical devices.
2. Add persistent result indexing and qualify very large result sets; paged transfer and live delivery are implemented.
3. Expand exFAT interoperability and NameHash/up-case validation, then add independently validated NTFS MFT/runlist and ext4 extent recovery.
4. Add source-bound resumable acquisition/checkpoint state and evaluate an explicitly packaged optional ddrescue integration.
5. Expand parser corruption/property/fuzz coverage and automated desktop recovery checks.
6. Add signed/notarized builds when credentials are available; investigate a narrow privileged helper and APFS separately.

## Released update: v0.1.2

Implemented decimal MB/GB display, clear subsecond timing and metadata-only Quick completion, neutral partition/method information, and focused FAT error isolation. Local integrated tests passed (56 Rust + 38 frontend). The extended Recoverit report records selected actual instruction/call paths, including SD profile detection and video reconstruction/repair. No new video-recovery implementation is claimed. The tagged native builds, publication and downloaded-asset verification passed; details are in [VALIDATION.md](docs/VALIDATION.md). Research scratch was removed and user originals preserved. The user then requested actual MP4/MOV recovery, now being implemented for v0.1.3.

## Released update: v0.1.3 video recovery

Implemented independent contiguous MP4/MOV carving for Deep/Advanced, with container/sample-table and AVC/HEVC configuration framing checks, 64-bit geometry and explicit unsupported cases. Deep is the initial UI mode; MP4/MOV filters and video guidance are available. Six real synthetic clips recover byte-for-byte and decode successfully. Local verification passed 83 Rust and 40 frontend tests, strict Clippy, formatting, build/lint and dependency inventory. All three tagged native builds, source verification, publication and downloaded-asset verification passed. The published source archive matched all 1,260 tagged files. Task-owned temporary/build files were cleaned; original user files and project source were preserved. See [v0.1.3 release evidence](docs/release-v0.1.3.json).

## v0.2.0

- [x] Stream accepted scan results through a separate paged API; retain candidates after cancellation and independently manage recovery jobs.
- [x] Add location/type trees, path/status filters, sorting and safe hierarchy-preserving output.
- [x] Recover existing FAT/exFAT files and add bounded tentative deleted-directory traversal, without guessing arbitrary deleted allocation chains.
- [x] Add a versioned data-only format registry, ZIP descriptors/ZIP64, bounded DOCX classification, RAR4/RAR5 framing checks and candidate diagnostics.
- [x] Test maximum signature spans across scan windows and compare old/new release-mode scanning on the same local synthetic image.
- [x] Pass 125 local Rust tests and 54 frontend tests, strict Clippy, formatting, build/lint and dependency checks. Independently extract all nine positive recovered archive fixtures; validate the generated FAT32 image with `fsck_msdos -n`.
- [x] Exercise native macOS live results, folder selection, concurrent recovery, cancellation retention and previews; compare recovered photos with source bytes in [GUI evidence](docs/gui-streaming-v0.2.0.json).
- [ ] Complete native Windows/macOS CI and record mounted-volume regression outcomes.
- [ ] Publish tagged installers, matching source/checksums and verify downloaded assets.
- [ ] Clean task-owned scratch/build outputs after publication.

These changes improve observable progress and tested format coverage. They do not establish a physical USB throughput guarantee, arbitrary video repair or a recovery success rate. See [archive validation](docs/ARCHIVE_RECOVERY_VALIDATION.md) for bounds and the local dispatch benchmark.

## Current update: v0.2.1

- [x] Validate FAT/exFAT directory candidates before reserving visited clusters, so stale references do not suppress valid ancestry.
- [x] Recognize narrowly corroborated E5-erased FAT special names; keep allocation, parent-pointer and bounded-read checks.
- [x] Preserve available basenames and metadata hierarchy; rename only actual output collisions.
- [x] Automatically enumerate drives on startup and retry/focus transitions, with independent loading/error state.
- [x] Add nested Unicode deletion, stale-alias, destination-name and source-discovery regressions.
- [ ] Directly qualify the user's damaged physical USB; current tools are attached to the Mac, not the paired Windows machine.

See [directory recovery details](docs/DIRECTORY_RECOVERY.md) and [executed validation](docs/VALIDATION.md).
