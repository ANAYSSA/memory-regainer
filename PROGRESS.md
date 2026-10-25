# Implementation progress

Updated: 25 October 2026. This file is a continuation record, not a claim of full completion of the master specification.

## Completed implementation work

- Rust recovery-core and separate Tauri desktop shell created.
- Read-only source primitives, checked extents, MBR/GPT discovery, FAT free-contiguous undelete, narrow exFAT metadata undelete and seven signature handlers implemented and covered by targeted tests.
- exFAT support uses original code based on the public Microsoft specification. Eleven exFAT tests pass, including exact deleted-PDF SHA-256 recovery, Unicode names, live directories, allocation conflicts, malformed metadata, range bounds, cancellation, fragmented live-directory traversal and entry-set boundary cases. The generated image also passed native `fsck_exfat -n` after read-only attachment; it was detached afterward. Final core total: 32 tests (1 unit + 20 previous integration + 11 exFAT), with formatting and clippy passed. Final integrated desktop/build evidence belongs in docs/VALIDATION.md.
- Desktop source registration, background jobs, native dialogs, previews, recovery output handling, imaging and source fingerprints implemented.
- Russian/English React interface and virtualized results connected to the real backend.
- Initial native Windows x64, macOS ARM64 and macOS Intel installer builds succeeded in [run 40202352920](https://github.com/ANAYSSA/memory-regainer/actions/runs/40202352920). Those builds preceded the exFAT addition; the updated release requires fresh builds.
- GPL-3.0-or-later license, independent-project notices, privacy/security/contribution guidance and initial name-search limitations recorded.
- Resolved dependency metadata inspected: 694 packages; no unresolved license expression. Exact-commit upstream notice supplements retained where registry archives omit their workspace license.
- Native release `.app` built and launched on Apple Silicon macOS. GUI recovery of 3 synthetic deleted files / 979 bytes completed; independent SHA-256 matched originals. Evidence: docs/gui-recovery-evidence.json.
- Source published at https://github.com/ANAYSSA/memory-regainer. The updated release publication and artifact verification remain pending final builds.
- CI [run 40202356372](https://github.com/ANAYSSA/memory-regainer/actions/runs/40202356372) passed. RustSec emitted five allowed Linux dependency warnings; see docs/AUDIT.md. That run preceded the exFAT addition.

## Required before this release is considered built

- [x] Run engine unit/integration tests: 32 core tests passed, including all 11 exFAT cases and the two independent-review boundary cases. Final integrated desktop totals are recorded in docs/VALIDATION.md.
- [x] Run desktop backend tests: 4 passed. Frontend: 18 tests, typecheck/build/lint passed. Workspace fmt/clippy passed.
- [x] Review and revise Windows destination-volume mapping for canonical paths and volume mount points. Native runtime qualification remains pending.
- [x] Physical-device manifests skip automatic local session/history writes.
- [x] Recovery/acquisition error paths revised to preserve reports on cancellation and source-change errors.
- [x] Build and launch real macOS release app; verify end-to-end GUI fixture recovery.
- [ ] Generate and inspect final DMGs containing the exFAT addition.
- [ ] Rebuild native Windows and both macOS architectures after the exFAT addition; collect final installers and checksums.
- [x] Run RustSec/npm vulnerability audits and document findings. Repeat if the dependency graph changes.
- [ ] Record exact publication URLs and verified release assets.
- [x] Record current local evidence in docs/VALIDATION.md; keep platform/release/audit checks pending until executed.

## Known scope limits

- No NTFS, ext4 or APFS metadata undelete; detection is distinct from recovery. exFAT is limited to clean single-FAT revision 1.00 volumes and intact fully initialized contiguous deleted streams in live directories.
- No encrypted-volume bypass, generic fragmented-file reconstruction, media repair, bootable environment or partition modifications.
- No pause/resume checkpoints. Saved image sessions validate a limited fingerprint and rerun the scan.
- Imaging stops on the first read error; no retry map or resumable ddrescue integration.
- Results are indexed in memory and returned after scanning. No multi-terabyte or million-result resource benchmark.
- No Developer ID / Windows code-signing certificates supplied; no Apple notarization.
- Physical device access and mapping require further platform qualification on disposable hardware.

## Next engineering milestones

1. Expand parser corruption/property/fuzz cases and automate desktop end-to-end recovery against disposable image fixtures.
2. Add paginated/persistent result indexing and live result batches before large-media performance claims.
3. Expand exFAT interoperability tests and NameHash/up-case validation; then add NTFS MFT/runlist and ext4 extent recovery with explicit support matrices.
4. Add conservative resumable acquisition with documented provenance and a reviewed optional ddrescue integration.
5. Qualify raw-device discovery/destination safety on disposable Windows/macOS/Linux devices, including multi-store containers and mounted volumes.
6. Add signed/notarized release builds when signing credentials become available; investigate APFS recovery separately.

Final local integration before tagging: 36 Rust tests + 18 frontend tests passed; formatting, strict Clippy, lint and production frontend build passed. Local DMG verified and launched; GUI imaging and saved-session restart passed. Native preflight installers for all three platforms passed; tagged release publication remains the next action.

Release v0.1.0 is published: https://github.com/ANAYSSA/memory-regainer/releases/tag/v0.1.0 . Final Windows x64 and macOS ARM64/Intel builds and tests passed; published artifact hashes verified. The next engineering work is the documented unsupported functionality, not completion of this installer release. Task-owned test images, downloaded verification copies, development build outputs and test application caches are being cleaned up at the user request.
