# Memory Regainer v0.1.3

Deep and Advanced scans now find and recover **MP4 and MOV video files** from readable images or devices when their original content remains contiguous and the required container metadata is intact.

## Recover a video

1. Select your USB drive or a raw image of it.
2. Use **Deep Scan**, now the initial mode. Leave all format boxes unchecked to search every supported type, or select **MP4** and **MOV**.
3. Let the scan finish, select the results, and recover them to separate healthy storage.
4. Open recovered videos in a local player and check their contents. A copy hash confirms the copied bytes, not the original video's completeness.

Quick Scan still checks supported FAT/exFAT deleted records; it does not perform a full video-content search.

## Changes

- An independent MP4/MOV parser checks finite container boundaries, video tracks, local data references, sample counts/sizes, sample-to-chunk mappings and media chunk ranges. Movie metadata may be before or after media data. The parser supports 64-bit atom sizes and chunk offsets; the ZIP/WAV 1 GiB limit does not apply to video.
- MP4/MOV format filters, video result icons and bilingual recovery guidance are included. Preview stays bounded; video files can be checked in a local player after recovery.
- Sizes use **MB/GB** with decimal conversion. Subsecond operations show **Less than 1 s**, and general method information is separate from actual scan warnings.
- Administrator access on Windows, progress/elapsed time and protection against automatic idle system sleep are retained.

## Video scope and validation

This release supports classic, self-contained MP4/MOV files. It does **not** reconstruct arbitrarily scattered fragments, rebuild missing movie metadata, repair overwritten video, decrypt tracks, or recover fragmented-MP4 (`moof`/`mvex`) layouts. Legacy MOV without `ftyp` and unbounded size-zero media-data atoms are unsupported. The app checks structure; it does not decode every recovered frame. These limits remain visible in scan and per-file notes.

Tests use small, real synthetic H.264/HEVC videos with AAC or PCM audio, including head/tail movie metadata and extended-size/64-bit-offset variants. They compare recovered bytes with known originals and exercise malformed tables, selected ranges and cancellation. Exact local decoding evidence, executed test counts and platform outcomes are recorded in [video validation](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.3/docs/VIDEO_RECOVERY_VALIDATION.md) and [release validation](https://github.com/ANAYSSA/memory-regainer/blob/main/docs/VALIDATION.md). This is not a real damaged-USB or recovery-rate benchmark.

All three tagged native builds passed. Each Mac target passed 83 Rust and 40 frontend tests; Windows passed 81 Rust and 40 frontend tests, excluding two Unix-only sparse-file cases. Windows raw-device and administrator-manifest checks passed. Downloaded installer/source SHA-256 values matched, both DMGs verified, and all 1,260 archived files matched the release tag. [Build results](https://github.com/ANAYSSA/memory-regainer/actions/runs/40213725993).

The release provides Windows x64 EXE, Apple Silicon DMG, Intel DMG, matching source, notices and SHA-256 checksums. Windows installers are unsigned; macOS builds are not Developer ID signed or notarized. Recovery works offline; installing missing WebView2 may need internet. Idle protection cannot override manual sleep, lid closure, shutdown or power loss.

Memory Regainer is independent software under GPL-3.0-or-later. No Recoverit code or binaries are included. See the [feature matrix](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.3/docs/FEATURES.md) and the [bounded Recoverit research report](https://github.com/ANAYSSA/memory-regainer/blob/v0.1.3/docs/RECOVERIT_ARCHITECTURE_REVIEW.md).
