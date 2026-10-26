# Memory Regainer v0.2.1

This update fixes directory references that could hide recoverable FAT/exFAT metadata, preserves available filenames without mandatory numeric prefixes, and improves automatic drive discovery when the app opens.

## Changes

- A rejected FAT directory reference no longer marks its cluster as visited and blocks a later valid reference. Live subdirectories are checked against their self/parent headers before admission. exFAT also records visits only after validating directory extents and allocation state.
- Deleted FAT special records may have E5 deletion markers in their first name bytes. They are accepted tentatively only with surviving name suffixes, valid directory attributes, zero special-record sizes, matching self/parent pointers and a currently free first cluster. A nonzero outer deleted-directory size is ignored only after those checks and never expands the read range.
- FAT warnings now identify the metadata record, cluster and specific rejection reason. Duplicate references point to the path already inspected.
- Files with surviving metadata keep sanitized original basenames and folder paths. Only actual collisions receive suffixes, such as `clip (2).mp4`; the former unconditional `000001_` prefix is removed. Carved candidates retain generated names and explicit unknown-path grouping.
- A failed temporary-file creation no longer deletes a preexisting file at that path. Cleanup is limited to temporary files created by the current operation.
- Drive discovery runs automatically at startup, retries while idle and refreshes when the window becomes active again. Initial enumeration shows a loading state, concurrent refreshes are coalesced, and a successful retry clears its discovery error. Image selections remain available.

## Scope and validation

A file's content can survive even when its filename and parent-directory records do not. This update improves supported metadata recovery; it cannot infer missing names from MP4/MOV content. Deleted FAT directory inspection still covers only the confirmed first free cluster. Orphan directory discovery, arbitrary fragmented deleted-file reconstruction, empty-directory recovery, missing video metadata repair and scan checkpoints remain unsupported.

Local validation passed **139 Rust tests and 61 frontend tests**, formatting, strict Clippy, frontend build/lint and dependency checks. Regression coverage includes stale aliases followed by valid paths, nested deleted Unicode names with erased special records, invalid allocation/header rejection, collision-safe output trees and automatic source-discovery lifecycle cases. Native Windows raw-volume, administrator-manifest and installer build results are recorded in [validation](https://github.com/ANAYSSA/memory-regainer/blob/main/docs/VALIDATION.md). The user's physical USB has not been directly inspected from this Mac session; synthetic tests do not establish its recovery outcome.

See [directory recovery details](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.1/docs/DIRECTORY_RECOVERY.md), [feature limits](https://github.com/ANAYSSA/memory-regainer/blob/v0.2.1/docs/FEATURES.md) and [source code](https://github.com/ANAYSSA/memory-regainer/tree/v0.2.1).

Downloads include Windows x64 EXE, Apple Silicon DMG, Intel DMG, matching source, notices and SHA-256 checksums. Windows installers are unsigned; macOS builds are not Developer ID signed or notarized. Recovery works offline; installing missing WebView2 may need internet. Idle-sleep protection cannot override manual sleep, lid closure, shutdown or power loss.

Memory Regainer is independent GPL-3.0-or-later software. No proprietary recovery code or binaries are included.
