# Validation record

This document records executed checks only. Initial implementation date: 25 October 2026. The source is being validated on an Apple Silicon Mac; platform build and test results are recorded below when available.

## Recorded checks

| Check | Result |
| --- | --- |
| License-expression inventory | 694 resolved Rust/npm package records inspected; zero unresolved expressions after review of legacy Cargo slash-separated alternatives |
| `cargo check --workspace` | Passed on Apple Silicon macOS |
| `cargo test --workspace --locked` | Integrated run passed: 4 desktop tests, 1 core unit test, 19 recovery integration tests; final integrated rerun pending the additional range-guard case |
| Final `cargo test -p recovery-core` | Passed after parser review: 21 total (1 unit + 20 integration), including range guards, Unicode long names/duplicate names and 128 deterministic mutations |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Frontend tests / typecheck / lint | Frontend author executed build/typecheck/lint and 18 passing Vitest tests |
| Native macOS release app build / launch | Passed; real `.app` launched and inspected through native UI automation |
| Native GUI recovery | Synthetic FAT image selected through native dialog; Quick Scan found 3 files; JPEG preview displayed; 3 files / 979 bytes restored with 0 failures; independent output SHA-256 checks matched originals |
| macOS DMG packaging | Pending |
| Native Windows / Intel macOS build | Pending GitHub Actions |
| RustSec audit | CI job configured; result not yet recorded |
| npm audit | Frontend author executed full dependency audit: 0 vulnerabilities |
| Physical damaged-media testing | Not performed; no personal drive used for destructive tests |
| Multi-terabyte / million-result benchmark | Not performed |
| Coverage / fuzz campaign | No measured coverage or long-running fuzz campaign yet |

GUI recovery evidence is retained in [gui-recovery-evidence.json](gui-recovery-evidence.json). The image/source files were synthetic; the internal APPLE SSD was only enumerated, and the application reported the actual access denial. No raw read or recovery success is inferred from device discovery.

Original fixture hashes are checked in at [SHA256.json](../crates/recovery-core/tests/fixtures/SHA256.json), with fixture generation code beside them. Tests cover actual deleted FAT directory entries and exact reconstructed bytes, supported carving formats, malformed/truncated structures, partition checksums and bounds, cancellation and rejected allocation cases. This is a targeted suite, not complete coverage of every item in the master specification.

The license inventory is a metadata check and notice collection step, not legal clearance. It includes build-only and other-platform dependencies. RustSec and npm audits depend on the advisory databases available at execution time.
