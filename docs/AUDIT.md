# Dependency audit record

Checked on **25 October 2026** against the committed `Cargo.lock` whose SHA-256 is `980d36f4fca4a0b51b8b641501331b77f8f6ad355e2b20cf3e09592575fda688`.

The [Rust audit job](https://github.com/ANAYSSA/memory-regainer/actions/runs/40202356372/job/121196015408) ran pinned `cargo-audit 0.22.2` at commit `e5131589d8427e6e2e2acf15eaf9b0455c9c4c40` and exited successfully. It reported **five allowed warnings**. This is not a warning-free audit or a statement that the entire dependency graph is free of security defects. The default `cargo audit` policy permits these informational advisory categories without a nonzero exit code; no advisory ignore list was added.

## Findings

| Package in the lockfile | Advisory | Meaning and current dependency path |
| --- | --- | --- |
| `proc-macro-error 1.0.4` | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | Unmaintained. Selected by the Linux GTK/glib procedural-macro stack. |
| `zbus_names 4.3.5` | [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html) | Standalone crate unmaintained after integration into zbus 6. Selected through Linux `rfd → ashpd → zbus`. |
| `zvariant 5.15.1` | [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html) | Standalone crate unmaintained after functionality moved into zbus 6 / zgvariant. Selected through the same Linux portal stack. |
| `zvariant_derive 5.15.1` | [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html) | Standalone derive crate unmaintained after integration into zbus 6. Selected through the same Linux portal stack. |
| `glib 0.18.5` | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) | **Unsound `VariantStrIter` iterator operations** can cause undefined behavior and crashes. Upstream identifies `>=0.20.0` as patched. Selected through Linux GTK/WebKitGTK in Tauri/wry. |

The unmaintained notices concern maintenance status; they are not themselves proof of an exploitable vulnerability. The glib finding concerns actual unsound behavior and must not be dismissed as merely an outdated package. Application-level reachability of the affected glib iterator was not analyzed.

## Target-specific assessment

The following commands were run for each of the five package/version pairs, using the existing lockfile:

```sh
cargo tree --locked --target aarch64-apple-darwin --invert glib@0.18.5 --depth 5
cargo tree --locked --target x86_64-apple-darwin --invert glib@0.18.5 --depth 5
cargo tree --locked --target x86_64-pc-windows-msvc --invert glib@0.18.5 --depth 5
cargo tree --locked --target x86_64-unknown-linux-gnu --invert glib@0.18.5 --depth 5
```

All five packages were absent from each macOS and Windows target graph (`nothing to print`). All five were present in the Linux target graph. A second check of the desktop package with `--features tauri/custom-protocol` likewise found none of the five packages in any of the three release target graphs.

| Release/source target | Finding |
| --- | --- |
| macOS Apple Silicon `aarch64-apple-darwin` | These five advisory packages are not selected by the active Rust dependency graph. |
| macOS Intel `x86_64-apple-darwin` | These five advisory packages are not selected by the active Rust dependency graph. |
| Windows x64 `x86_64-pc-windows-msvc` | These five advisory packages are not selected by the active Rust dependency graph. |
| Linux `x86_64-unknown-linux-gnu` | All five are selected. Linux is not packaged or security-qualified for this release. |

This establishes target selection for the checked lockfile and features; it does not audit operating-system WebViews, system libraries, native tooling or every possible feature combination. The inventory includes other-platform packages because Cargo resolves them into the shared lockfile.

## Follow-up

Before distributing a Linux desktop build, resolve the glib issue through a compatible supported GTK/WebKitGTK dependency stack or an independently reviewed fix, migrate the portal dependencies as their upstreams permit, and rerun the audit and Linux tests. Simply forcing a different glib major/minor version into incompatible bindings is not a verified fix. No dependency changes were made solely to hide these warnings from the current macOS/Windows release audit.

Recheck all advisories after changes to dependencies, features or supported targets. A successful audit is a dated database check, not a permanent security guarantee.
