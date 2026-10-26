# Validation record

This document records executed checks only. Initial implementation date: 25 October 2026. The source is being validated on an Apple Silicon Mac; platform build and test results are recorded below when available.

## v0.1.0 checks

| Check | Result |
| --- | --- |
| License-expression inventory | 694 resolved Rust/npm package records inspected; zero unresolved expressions after review of legacy Cargo slash-separated alternatives |
| `cargo check --workspace` | Passed on Apple Silicon macOS |
| `cargo test --workspace --locked` | Final integrated run passed: 36 Rust tests — 4 desktop, 1 core unit, 20 FAT/carving/partition integration and 11 exFAT integration tests |
| Final `cargo test -p recovery-core` | Passed after independent parser review: 32 total; includes allocation conflicts, Unicode names, deterministic mutations, fragmented live directory chains and entry sets crossing cluster/read-page boundaries |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Frontend tests / typecheck / lint | Final integrated build/typecheck/lint passed; 18 Vitest tests passed |
| Native macOS release app build / launch | Passed; real `.app` launched and inspected through native UI automation |
| Native GUI recovery | Synthetic FAT image selected through native dialog; Quick Scan found 3 files; JPEG preview displayed; 3 files / 979 bytes restored with 0 failures; independent output SHA-256 checks matched originals |
| macOS DMG packaging | Local Apple Silicon DMG built, `hdiutil verify` passed, application launched from read-only mounted DMG |
| Native GUI imaging | 33,827,840-byte synthetic FAT32 image copied; independent SHA-256 equals source: `013516774c5ece8385f3d0b92ce63f661757a428b4ba65fe2a97ad20cea4ef82` |
| Saved-session restart | Exported synthetic image session, restarted the app, reopened session and found the same 3 deleted files by rescanning |
| Independent filesystem checks | `fsck_msdos -n` accepted FAT32 lab (unknown FSInfo free-count warning); read-only mounted exFAT lab passed `fsck_exfat -n` with “volume appears to be OK” |
| Native installer preflight | All 3 native builds passed [run 40202352920](https://github.com/ANAYSSA/memory-regainer/actions/runs/40202352920) before final exFAT/session additions; final tagged installers are rebuilt and retested by the release workflow |
| RustSec audit | `cargo-audit 0.22.2` CI job passed with **5 allowed warnings**, including glib unsoundness; all five are excluded from the three macOS/Windows target graphs. Linux is not packaged. See [AUDIT.md](AUDIT.md) for exact findings and scope. |
| npm audit | Final local full dependency audit: 0 vulnerabilities |
| Physical damaged-media testing | Not performed; no personal drive used for destructive tests |
| Multi-terabyte / million-result benchmark | Not performed |
| Coverage / fuzz campaign | No measured coverage or long-running fuzz campaign yet |

GUI recovery evidence is retained in [gui-recovery-evidence.json](gui-recovery-evidence.json). The image/source files were synthetic; the internal APPLE SSD was only enumerated, and the application reported the actual access denial. No raw read or recovery success is inferred from device discovery.

Original fixture hashes are checked in at [SHA256.json](../crates/recovery-core/tests/fixtures/SHA256.json), with fixture generation code beside them. Tests cover actual deleted FAT directory entries and exact reconstructed bytes, supported carving formats, malformed/truncated structures, partition checksums and bounds, cancellation and rejected allocation cases. This is a targeted suite, not complete coverage of every item in the master specification.

The license inventory is a metadata check and notice collection step, not legal clearance. It includes build-only and other-platform dependencies. RustSec and npm audits depend on the advisory databases available at execution time.

The final exFAT engine is covered by the automated tests and read-only filesystem check. The native interactive smoke tests preceded that addition; the Mac was subsequently locked. Windows/Intel installers were built and automatically tested, not interactively installed in this session. Native build evidence for the tagged revision is available in the release workflow.

## Published release verification

v0.1.0 published successfully from commit `2ea7d8a8683db758bc1d6eea5c1108908ff05089`. [All three native tagged builds](https://github.com/ANAYSSA/memory-regainer/actions/runs/40203903188) and [final source verification](https://github.com/ANAYSSA/memory-regainer/actions/runs/40203900994) passed. All three downloaded installers and matching source archive passed SHA-256 verification; both published DMGs passed `hdiutil verify`. Published checksum filenames were normalized to match GitHub asset names. Structured sizes/hashes are recorded in [release-v0.1.0.json](release-v0.1.0.json).


## v0.1.1 local validation

The latest completed local run reported 51 Rust tests (44 core, 7 desktop) and 38 frontend tests, 89 total. Strict workspace Clippy passed. The dependency inventory still contains 694 packages with zero unresolved license expressions. Formatting, strict workspace Clippy and the complete Rust suite passed again after the source-capacity and transfer-error review corrections. All three native installer preflight builds passed from `4e38a7fad2d8037373fb2d7136b541812314e93d`: [Windows x64 and macOS ARM64/Intel](https://github.com/ANAYSSA/memory-regainer/actions/runs/40207498239). Each ran 51 Rust and 38 frontend tests. [Source verification and RustSec audit](https://github.com/ANAYSSA/memory-regainer/actions/runs/40207497528) also passed. The tagged release rebuilds and retests the final source; uploaded artifact verification is recorded after publication. These local results must not be read as Windows runtime or installer qualification.

A native Apple Silicon macOS reader probe used a newly generated, disposable 8 MiB zero-filled image attached read-only and without mounting. The probe accessed `/dev/rdisk6` as a raw device, measured 512-byte sectors, passed Quick/Deep scans, and verified unaligned content reads. Quick read 131,072 bytes; Deep read 8,388,608 bytes. The probe found zero files, as expected for this fixture. It observed the expected opening/partition/metadata/carving/completion stages, exited successfully, and the device was detached. This validates the tested macOS raw-device path; it is not a physical USB, damaged-media or recovery-quality test.

A separate native macOS probe acquired the application's `PreventUserIdleSystemSleep` assertion. `pmset` showed the named assertion while the process was active and its absence after completion; the probe exited successfully. Backend tests exercise job cleanup on success, failure, cancellation and a caught panic. The assertion probe did not put the Mac to sleep, close its lid or simulate power loss. Structured probe evidence is retained in [native-probes-v0.1.1.json](native-probes-v0.1.1.json).

A Windows x64 runner created a new fixed 32 MiB VHD and attached it read-only. Native geometry, unaligned content reads and Quick/Deep scans passed. A diagnostic call to the former regular-file metadata API returned `Incorrect function. (os error 1)` on that same raw handle, while the new reader passed. The VHD was detached and removed. Windows SDK `mt.exe` extracted the built application manifest and confirmed `requireAdministrator` with `uiAccess=false`. The native sleep-guard unit test passed on Windows. These are automated checks, not interactive UAC-consent or physical USB qualification. See [preflight run](https://github.com/ANAYSSA/memory-regainer/actions/runs/40207498239).

Outstanding platform checks include interactive UAC behavior, the reported physical USB fault, forced or manual sleep/wake, disconnect/reconnect and blocked-driver cancellation. The user-reported error was not independently reproduced. Published v0.1.0 evidence above remains historical; v0.1.1 asset verification will be recorded separately after publication.

## v0.1.1 published asset verification

v0.1.1 was published from `f0f153cc191269132e23e45cba34e356ea4ecf01`. [All tagged native builds and publication](https://github.com/ANAYSSA/memory-regainer/actions/runs/40208121042) passed, as did [source verification and audit](https://github.com/ANAYSSA/memory-regainer/actions/runs/40208119159). Each native build ran 51 Rust and 38 frontend tests. The Windows VHD regression and extracted administrator manifest checks passed again. All three downloaded installers and the matching source archive passed their published SHA-256 checks; both DMGs passed `hdiutil verify`. The source archive contains version 0.1.1 and the administrator manifest. Recorded asset sizes and hashes are in [release-v0.1.1.json](release-v0.1.1.json).

Task-owned local build/dependency directories, generated schemas, probe images/binaries, research scratch and downloaded verification copies were removed after checks. Source, retained validation evidence and supplied user files remain. No interactive physical USB sleep/wake qualification is claimed.

## v0.1.2 local validation

The integrated Apple Silicon workspace passed 56 Rust tests (49 core and 7 desktop); the frontend passed 38 tests, typechecking, lint and its production build. Five new core regressions cover invalid FAT32 root clusters, preserving valid candidates before and after a conflicting FAT entry, preserving results before a damaged directory continuation, fatal metadata I/O errors and fatal cancellation. Healthy FAT/exFAT fixtures have no event warnings and retain separate method notes. UI render tests distinguish subsecond elapsed time and metadata-only completion. Native tagged builds, Windows scratch-VHD/manifest checks and uploaded-asset verification are recorded below once completed.

## v0.1.2 published asset verification

v0.1.2 was published from `6c2630e43dde66e84e6f534d35edc2c7029fa8a8`. [All three native builds and publication](https://github.com/ANAYSSA/memory-regainer/actions/runs/40212098812) and [source verification/audit](https://github.com/ANAYSSA/memory-regainer/actions/runs/40211969764) passed. Each native build ran 56 Rust and 38 frontend tests; Windows raw-VHD and administrator-manifest checks passed. The downloaded EXE, both DMGs and source archive matched all published SHA-256 values. Both DMGs passed `hdiutil verify`; every one of the archive's 1,247 files matched the tagged Git tree. Structured evidence is in [release-v0.1.2.json](release-v0.1.2.json). Research scratch and downloaded proprietary payload copies were removed; original user-supplied files were preserved. The subsequent video request continues as v0.1.3.

## v0.1.3 local video validation

The integrated Apple Silicon workspace passed **83 Rust tests** (76 core and 7 desktop), workspace formatting and strict Clippy. The frontend production build, lint and **40 tests** passed: **123 local tests total**. The dependency inventory remains 694 entries with zero unresolved license expressions.

The video suite includes 24 public integration tests and three additional private parser regressions. Six independently generated H.264/HEVC MP4/MOV clips were recovered byte-for-byte, matched their original SHA-256 hashes, and fully decoded with strict FFmpeg error handling. See [video evidence](VIDEO_RECOVERY_VALIDATION.md). FFmpeg is a local validation tool and is not an application or CI dependency. The two Unix sparse-file tests distinguish a source position above 4 GiB from an actual candidate whose extended `mdat` and file-relative `co64` offsets exceed 4 GiB. The latter establishes parser geometry using sparse padding, not playback or throughput of a multi-gigabyte original. Windows excludes those two Unix tests and therefore runs 81 Rust tests.

Independent review found and corrected two structural acceptance gaps before release: truncated media-handler fixed fields and out-of-bounds visual sample-description extension atoms. Regression tests now cover both, missing AVC configuration and malformed AVC/HEVC configuration lengths. Frame decoding remains outside runtime parser scope. Native publication and downloaded-asset verification are recorded below after completion.

## v0.1.3 published asset verification

v0.1.3 was published from `a980638fdf92782777d59cf0418b8ff3ef2ae74a`. [All three tagged native builds and publication](https://github.com/ANAYSSA/memory-regainer/actions/runs/40213725993) and [source verification/audit](https://github.com/ANAYSSA/memory-regainer/actions/runs/40213562570) passed. Each Mac target ran 83 Rust tests; Windows ran 81 (the two sparse-file geometry cases are Unix-only). All targets ran 40 frontend tests. The Windows read-only VHD regression and extracted administrator-manifest checks passed again.

The downloaded EXE, both DMGs and matching source archive passed all published SHA-256 checks. Both DMGs passed `hdiutil verify`. All **1,260 files** in the source archive matched the tagged Git tree. Sizes, hashes and run links are retained in [release-v0.1.3.json](release-v0.1.3.json).

A browser UI smoke check confirmed the initial Deep selection, MP4/MOV checkbox selection, Quick coverage explanation and Russian/English presentation. This did not perform native disk access. The six recovered-video decode checks described above exercised the core recovery API on synthetic images; no claim is made about the user's physical USB or recovery of scattered fragments.

Task-owned build/dependency directories, generated schemas, downloaded proprietary payloads, research scratch, recovered test exports and verification downloads were removed. The test server was stopped and its temporary browser tab closed. Project source, reproducible fixtures, retained evidence and original user-supplied files remain.

## v0.2.0 streaming scan validation

The v0.1.3 audit identified two distinct causes of misleading early results: the desktop withheld its result index until the entire core call returned, and FAT/exFAT enumeration excluded existing files and skipped deleted directories. The application also listed whole Windows physical disks without mounted drive-letter sources, returned no structured metadata paths, exported a flat list, and lacked RAR and descriptor-based ZIP carving. The initial user comparison was against Recoverit's ongoing results, not a completed recovery benchmark. No conclusion about the user's unreadable or overwritten bytes is inferred from that comparison.

The existing engine was extended in place. Desktop Scan now performs filesystem metadata traversal before content carving, publishes validated entries immediately, and permits a separate recovery worker for a snapshot of selected results while the scan continues. The backend has independent cancellation and busy slots, lightweight status snapshots, bounded result pages, source revalidation and physical-destination checks. Core APIs retain their legacy metadata-only mode for callers and regression tests; the desktop exposes one Scan action with collapsed expert options.

Local integrated checks on Apple Silicon passed **125 Rust tests** (110 core and 15 desktop), formatting and strict workspace Clippy. The final frontend production build, lint and **54 tests** passed. Production npm audit reported zero vulnerabilities; the license inventory inspected 694 records with zero unresolved expressions. GitHub's RustSec job passed with the existing documented exceptions in [AUDIT.md](AUDIT.md), not a claim of zero advisories on every platform.

A synthetic FAT32 tree contains 42 original playable MP4 entries and two original photos, including existing fragmented allocation, live subdirectories and cautiously recovered deleted ancestry. Tests recover all expected file bytes, compare hashes and verify that the image is unchanged; separate corruption, cycle, range and cancellation cases preserve independent valid entries. An exported 33,827,840-byte FAT32 fixture with FSInfo and backup boot records passed `/sbin/fsck_msdos -n` with exit 0. Its SHA-256 was `1aa94dd9133458547c85a00532fad71d017497ce04ffa2357ad0ba37ff5d2081`.

A native Tauri debug-bundle test used an earlier equivalent tree fixture, sparsely extended to 4 GiB. All 44 metadata rows appeared in under one second while content scanning continued. A separate copy restored two selected photos while the scan was at 36.8%; the scan subsequently continued to 53.1% before user cancellation. All 44 rows, selection and preview remained available. Independent SHA-256 calculations matched the output reports and source-image bytes. The test discovered a frontend ETA display-clock skew, subsequently fixed and covered by a focused test. Exact observations and limits are retained in [gui-streaming-v0.2.0.json](gui-streaming-v0.2.0.json).

Archive recovery, independent extraction and the bounded local dispatch microbenchmark are recorded in [ARCHIVE_RECOVERY_VALIDATION.md](ARCHIVE_RECOVERY_VALIDATION.md). The single source of supported formats is [formats.json](../crates/recovery-core/src/formats.json); a regression checks that the legacy public list matches it.

Windows regression coverage creates only uniquely named disposable VHD files on the CI runner. Every formatting operation is guarded by the exact image-path-to-disk relationship, virtual bus identity, size and non-system status. The scanner runs after read-only attachment, including a raw mounted FAT32 drive letter, native length/sector geometry, reads near the volume tail and exact recovery of 42 clips, two photos and a unique marker. Actual native run and release asset outcomes are recorded below when completed. This is not an interactive installation, a physical USB test or damaged-media qualification.

The implementation follows Microsoft's [raw CreateFile guidance](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew), [volume-length and extended DASD guidance](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-ioctl_disk_get_length_info), [exFAT specification](https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification), CGSecurity's [FAT undelete documentation](https://www.cgsecurity.org/testdisk_doc/undelete_fat.html) and [PhotoRec workflow](https://www.cgsecurity.org/wiki/PhotoRec_Step_By_Step). Recoverit's [official guide](https://recoverit.wondershare.com/guide.html) was used to compare the visible one-scan, live-results and recovery workflow. These sources do not disclose or establish equivalence with its proprietary recovery algorithms.


## v0.2.0 native and published asset verification

v0.2.0 was published from `bb1703190aaf54c5d01ce85459b194da9831eaee`. [Source verification and audit](https://github.com/ANAYSSA/memory-regainer/actions/runs/40292243420) and [all tagged native builds and publication](https://github.com/ANAYSSA/memory-regainer/actions/runs/40292507512) passed. Each Mac target ran 125 Rust tests; Windows ran 123, excluding two Unix-only cases. All three targets ran 54 frontend tests. Windows raw physical-device, mounted FAT32-volume and extracted `requireAdministrator` manifest checks passed.

On the mounted Windows test volume, the native size was 133,103,616 bytes. Both metadata and content scans streamed 46 entries and exactly reconstructed all 45 expected fixtures: 42 MP4 files, two JPEGs and a unique marker. The extra entry was not part of the fixture assertions. The native formatter left the informational BPB label as `NO NAME`; the authoritative root-directory volume label was `MRPROBE`. Read-only tail/unaligned reads and full-volume scans passed. Structured native evidence is in [native-volume-v0.2.0.json](native-volume-v0.2.0.json).

The three downloaded published installers and source archive matched every published SHA-256 value. Both DMGs passed `hdiutil verify`; read-only mounting confirmed bundle version 0.2.0 and the expected arm64/x86_64 architecture. Every one of the source archive's 1,293 files matched the tagged Git tree, and the separately published license/notices matched the same tag. Evidence is in [release-v0.2.0.json](release-v0.2.0.json). Windows and Intel installers were not interactively installed on the user's machines; the real damaged USB remains unqualified.

Task-owned build/dependency folders, temporary native preview, generated schemas, synthetic images, benchmark/research scratch and downloaded verification copies were removed after validation. The main local cleanup removed approximately 3.71 GB of allocated files. Source, original reproducible fixtures, validation evidence and all supplied user files were preserved.

## v0.2.1 directory recovery and discovery validation

The integrated Apple Silicon workspace passed **138 Rust tests** (120 core and 18 desktop) and **61 frontend tests**. Formatting, strict workspace/all-target Clippy, frontend production build/lint, production npm audit and dependency license inventory passed. No runtime dependencies changed; the inventory contains 694 records with no expressions requiring review.

Ten new core regressions cover rejected FAT/exFAT aliases followed by valid references, free versus allocated first clusters, live self/parent admission, E5-erased special names, precise invalid-cluster/size reasons and bounded nonzero deleted-directory sizes. The nested deleted-name case reconstructs a UTF-16 filename and parent path, verifies exact video bytes, and checks that the synthetic source is unchanged. Three added desktop tests and the strengthened recovery worker test verify collision-only renaming, nested Unicode output folders, distinct partitions and explicit unknown-path grouping. Seven frontend lifecycle regressions exercise initial loading, late bridge readiness, empty/transient discovery, retry error clearing, overlapping refreshes, idle transitions and asynchronous focus-listener cleanup.

Independent review found no actionable issue in the directory admission diff. The new live-header checks still require native Windows formatter qualification; native CI and tagged installer results are recorded below when completed. The user's report that v0.2.0 recovered content does not independently verify original names or paths. The actual USB and its recovery report have not been inspected in this Mac-attached execution session.
