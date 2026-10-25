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
