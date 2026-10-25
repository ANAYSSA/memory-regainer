# Licensing and distribution

Memory Regainer's original application code, tests, documentation and original branding assets are licensed under **GNU GPL version 3 or, at your option, any later version** (`GPL-3.0-or-later`). The full version 3 terms are in [LICENSE](LICENSE). Copyright 2026 Memory Regainer contributors. Individual upstream files retain their existing copyright and license notices.

Memory Regainer is an independent project. It is not affiliated with, endorsed by, or derived from the proprietary implementation or assets of commercial recovery software vendors. It does not circumvent product activation, encryption or operating-system access controls.

## What is distributed

The release combines original Rust recovery code with Tauri, React and their dependencies. Permissive upstream licenses remain attached to those components. Selecting a GPL-compatible alternative from a dual-license expression does not remove the obligation to retain the upstream notices. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), the versioned [inventory](docs/dependency-inventory.json), and the retained [license texts](docs/third-party-licenses/).

The project's license decision uses the actual dependency declarations and upstream license files. MIT, ISC, BSD, Apache-2.0, Zlib, Unicode and equivalent permissive components are used under their applicable terms. Apache-2.0 components are compatible with GPLv3, but that does not imply compatibility with GPLv2-only. MPL-2.0 components, where present in the resolved dependency graph, retain file-level notices and source availability; the inventory distinguishes resolution from inclusion in a specific installer. An automated metadata gate is supplementary and cannot replace review of changed upstream terms.

The upstream `objc2` family explicitly records a caveat about bindings derived from Apple SDKs in its versioned `LICENSE.md`. That notice is retained in the third-party license resources. Its declared MIT / Apache-2.0 / Zlib expressions do not settle Apple's SDK terms; native macOS builds require Apple's developer tools and their applicable agreements. This remains an item for qualified legal review, not a claim of clearance.

The matching source and build scripts are available at the same GitHub release as each binary, without charge. `Cargo.lock` and `package-lock.json` identify dependency versions; the inventory links upstream repositories and package source archives. Preserve this information when redistributing and provide corresponding source as required by GPLv3. A release's source tag must match its binaries. Do not use a moving branch as the sole source reference for a release.

## Recovery components evaluated, but not bundled

| Component | Upstream terms examined | Decision |
| --- | --- | --- |
| TestDisk / PhotoRec | GPL-2.0-or-later, as stated by [CGSecurity](https://www.cgsecurity.org/testdisk_doc/presentation.html) | Not bundled or invoked. A future integration would need exact-version source, notices and an explicit GPLv3-compatible license choice. A wrapper does not remove these obligations. |
| GNU ddrescue | GPL-2.0-or-later, as stated in the [official project's Licensing section](https://www.gnu.org/software/ddrescue/ddrescue.html) | Not bundled or invoked. Exact-version archive and COPYING terms must be inspected before integration; no license compatibility is assumed from the executable boundary. |
| The Sleuth Kit | Mixed per-file terms, including IPL/CPL and other licenses; [upstream overview](https://www.sleuthkit.org/sleuthkit/licenses.php) | Not linked or bundled. No blanket GPL-compatibility assumption is made for this mixed-license codebase. |

LGPL libraries are not deliberately introduced as a recovery backend. A future LGPL integration must account for the specific license version, linking arrangement, modification notices, corresponding library source and replacement/relinking requirements. GPL-2.0-only code must not be combined on the assumption that it can be automatically upgraded to GPLv3. New dependencies require a fresh review.

## Scope of assurances

The software is provided without warranty under the license. There is no guarantee of recovery, support, suitability for forensic evidence, completeness of trademark searches, or compatibility with every device. Nothing here establishes ownership of third-party trademarks or formal legal clearance. See [TRADEMARKS.md](TRADEMARKS.md) for the recorded initial name check.
