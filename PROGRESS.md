# Implementation progress

Updated: 25 October 2026. This file is a continuation record, not a claim of full completion of the master specification.

## Completed implementation work

- Rust recovery-core and separate Tauri desktop shell created.
- Read-only source primitives, checked extents, MBR/GPT discovery, FAT free-contiguous undelete and seven signature handlers implemented and covered by targeted tests.
- Desktop source registration, background jobs, native dialogs, previews, recovery output handling, imaging and source fingerprints implemented.
- Russian/English React interface and virtualized results connected to the real backend.
- Native Windows x64, macOS ARM64 and macOS Intel installer workflows created; publication remains pending actual runs.
- GPL-3.0-or-later license, independent-project notices, privacy/security/contribution guidance and initial name-search limitations recorded.
- Resolved dependency metadata inspected: 694 packages; no unresolved license expression. Exact-commit upstream notice supplements retained where registry archives omit their workspace license.
- Native release `.app` built and launched on Apple Silicon macOS. GUI recovery of 3 synthetic deleted files / 979 bytes completed; independent SHA-256 matched originals. Evidence: docs/gui-recovery-evidence.json.
- Repository created at https://github.com/ANAYSSA/memory-regainer; source push and release publication pending.

## Required before this release is considered built

- [x] Run engine unit/integration tests: final core run passed 1 unit + 20 integration; previous integrated workspace run passed 1 unit + 19 integration before the final range case.
- [x] Run desktop backend tests: 4 passed. Frontend: 18 tests, typecheck/build/lint passed. Workspace fmt/clippy passed.
- [x] Review and revise Windows destination-volume mapping for canonical paths and volume mount points. Native runtime qualification remains pending.
- [x] Physical-device manifests skip automatic local session/history writes.
- [x] Recovery/acquisition error paths revised to preserve reports on cancellation and source-change errors.
- [x] Build and launch real macOS release app; verify end-to-end GUI fixture recovery.
- [ ] Generate and inspect a real DMG.
- [ ] Run native Windows and Intel macOS build jobs; collect real installers and checksums.
- [ ] Run RustSec/npm vulnerability audits and resolve or document findings.
- [ ] Record exact publication URLs and verified release assets.
- [x] Record current local evidence in docs/VALIDATION.md; keep platform/release/audit checks pending until executed.

## Known scope limits

- No NTFS, exFAT, ext4 or APFS metadata undelete; detection is distinct from recovery.
- No encrypted-volume bypass, generic fragmented-file reconstruction, media repair, bootable environment or partition modifications.
- No pause/resume checkpoints. Saved image sessions validate a limited fingerprint and rerun the scan.
- Imaging stops on the first read error; no retry map or resumable ddrescue integration.
- Results are indexed in memory and returned after scanning. No multi-terabyte or million-result resource benchmark.
- No Developer ID / Windows code-signing certificates supplied; no Apple notarization.
- Physical device access and mapping require further platform qualification on disposable hardware.

## Next engineering milestones

1. Expand parser corruption/property/fuzz cases and automate desktop end-to-end recovery against disposable image fixtures.
2. Add paginated/persistent result indexing and live result batches before large-media performance claims.
3. Add tested exFAT deleted-entry recovery, then NTFS MFT/runlist and ext4 extent work with explicit support matrices.
4. Add conservative resumable acquisition with documented provenance and a reviewed optional ddrescue integration.
5. Qualify raw-device discovery/destination safety on disposable Windows/macOS/Linux devices, including multi-store containers and mounted volumes.
6. Add signed/notarized release builds when signing credentials become available; investigate APFS recovery separately.
