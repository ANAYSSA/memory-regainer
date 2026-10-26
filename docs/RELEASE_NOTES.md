# Memory Regainer v0.2.0

Results now appear during scanning, and supported FAT/exFAT metadata recovery preserves useful folder paths, includes existing files and cautiously inspects some deleted directories. Content scanning also adds DOCX and RAR4/RAR5, broader ZIP support and visible candidate diagnostics.

## Changes

- One **Scan** action combines metadata and content searching. Collapsed **Expert options** holds optional format filters and byte ranges.
- Accepted results arrive through a separate paged API. Browse location/type trees, filter names, paths and deletion status, sort results and recover selected files while scanning continues. Cancelling a scan retains accepted candidates; source identity is checked again before recovery.
- FAT existing files follow current allocation chains, including fragmentation. Deleted-directory inspection is limited to a currently free first cluster with consistent directory records. Deleted file recovery still requires contiguous, currently free clusters.
- exFAT existing streams and bounded contiguous deleted-directory candidates are included on supported clean single-FAT revision 1.00 volumes. Tentative deleted ancestry stays visible in the evidence. Arbitrarily fragmented deleted files remain unsupported.
- Recovery preserves sanitized metadata hierarchy in a new output folder, handles name collisions and verifies copied bytes by SHA-256 read-back.
- ZIP validation supports signed/unsigned data descriptors and ZIP64. DOCX classification checks bounded package relationships, content type and the document XML root. RAR4/RAR5 require valid header checksums, finite data bounds and an explicit end record. Split and encrypted archives are unsupported; ordinary archive payloads are not decoded by the app.
- A versioned JSON registry supplies the eleven format categories, aliases and bounded signatures. Changes require rebuilding and can only select implemented validators. Precompiled first-byte dispatch reduces signature-search overhead. Actual read counters and bounded rejection reasons help explain scan activity.
- Windows discovery and regression coverage now include mounted volumes alongside whole physical disks. Standard UAC, read-only source handles and destination checks remain in place.

## Scope and evidence

The default Scan examines supported FAT/exFAT metadata and then scans file contents. The legacy core Quick mode remains metadata-only; its byte counter is not a full-disk scan. Carved files have generated names and unknown original paths. Results labeled existing or deleted describe metadata evidence, not a guarantee that their contents are intact.

MP4/MOV recovery remains limited to classic, self-contained contiguous containers with intact metadata. This release does not add video fragment reconstruction, missing-metadata repair, NTFS/ext4/APFS undelete, pause/resume checkpoints or bad-sector imaging retries. Result transfer is paged, but backend and frontend catalogs remain in memory.

Local validation passed **125 Rust tests and 54 frontend tests**, formatting, strict Clippy, build/lint and dependency checks. Nine archive fixtures recovered exactly; independent ZIP decompression/CRC checks and RAR extraction matched their originals. A generated FAT32 image passed the macOS filesystem checker, and a nested fixture with 42 videos and two photos recovered byte-for-byte. Native macOS GUI checks observed 44 results before content scanning completed, folder selection, concurrent recovery and retention after cancellation; two recovered photos matched source bytes. See [GUI evidence](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.0/docs/gui-streaming-v0.2.0.json). These synthetic checks do not qualify a damaged physical USB device. All three tagged native builds passed: 125 Rust tests on each Mac target, 123 on Windows (two Unix-only cases excluded), and 54 frontend tests on every target. Windows raw-disk/mounted-volume and administrator-manifest checks passed. Published installer/source SHA-256 values matched, both DMGs verified with version 0.2.0 and the correct CPU architecture, and all 1,293 source archive files matched the tag. [Build results](https://github.com/ANAYSSA/memory-regainer/actions/runs/40292507512).

See [feature limits](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.0/docs/FEATURES.md), [archive validation](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.0/docs/ARCHIVE_RECOVERY_VALIDATION.md), [video validation](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.0/docs/VIDEO_RECOVERY_VALIDATION.md) and [release validation](https://github.com/ANAYSSA/memory-regainer/blob/main/docs/VALIDATION.md).

Release targets are Windows x64 EXE, Apple Silicon DMG and Intel DMG, with matching source, notices and SHA-256 checksums. Windows installers are unsigned; macOS builds are not Developer ID signed or notarized. Recovery works offline; installing missing WebView2 may need internet. Idle-sleep protection cannot override manual sleep, lid closure, shutdown or power loss.

Memory Regainer is independent GPL-3.0-or-later software. No proprietary recovery code or binaries are included.
