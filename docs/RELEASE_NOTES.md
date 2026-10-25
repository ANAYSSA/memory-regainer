Memory Regainer is an early, independently implemented, offline recovery utility.

This prerelease focuses on safe image-based recovery, FAT metadata recovery, narrow exFAT metadata undelete and seven bounded signature carvers. See the source README and feature matrix for the exact supported behavior. Recognition of a filesystem does not mean deleted-file recovery is implemented for it. A recognizable header does not guarantee an intact file.

The exFAT implementation supports clean single-FAT revision 1.00 volumes and intact inactive entry sets for fully initialized contiguous NoFatChain files. It preserves Unicode names, traverses live directories, and requires every candidate data cluster to be free in the current allocation bitmap. Inactive entry checksums are interpreted as a recovery heuristic and results remain low-confidence. Fragmented deleted streams, deleted directories, partial initialization, TexFAT and NameHash/up-case validation are not supported. Deleted-PDF fixture recovery is hash-checked; the generated fixture also passed native read-only filesystem validation.

Installers are built by native GitHub-hosted Windows and macOS runners. A successful build confirms compilation and the included automated tests; it does not establish that raw access works on every device or that the installer was interactively tested on every OS version.

The Windows installer is unsigned. macOS builds are not signed with an Apple Developer ID or notarized; the packaging tool may apply an ad-hoc signature. Windows SmartScreen and macOS Gatekeeper may therefore show publisher/security warnings. Verify the SHA-256 checksum and inspect the source before choosing to trust a build. No signing credentials are bundled.

Use a separate healthy destination disk. Stop using the affected storage, and prefer scanning a disk image. Overwritten bytes, SSD TRIM and inaccessible encrypted data cannot be recovered by this application. Physically failing media can require specialist handling.

Source for this exact release is attached. Dependency versions, origins and license notices are recorded in the source tree. GPL-3.0-or-later; no subscription, activation, telemetry, watermark or paid recovery limit.
