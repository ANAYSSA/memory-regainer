# Third-party notices

Memory Regainer's original code is GPL-3.0-or-later. Third-party components remain governed by their own licenses. The exact resolved versions, license expressions, origins and package archive links are recorded in [docs/dependency-inventory.json](docs/dependency-inventory.json). Upstream copyright and license texts are retained in [docs/third-party-licenses](docs/third-party-licenses/). Both are generated from the installed, locked dependency graph, not a list of guessed versions.

The inventory includes build tools, test dependencies and optional packages for platforms other than the inventory host. Their presence in the lockfile does not mean that every package is embedded in every binary. Optional packages absent from the inventory host are identified explicitly. Native installer builds retain the project's collected notice resources.

When a registry archive omitted its workspace-root license, additional upstream notices were collected at the revision recorded by that archive. Those supplements and their source URLs are preserved under `docs/upstream-license-supplements` and copied into the bundled notice set. Some packages publish their licensing statement only in package metadata or a README; those original statements are retained without inventing a copyright owner. The objc2 upstream Apple SDK caveat is preserved and discussed in [LEGAL.md](LEGAL.md).

Principal component families:

| Component | License family | Purpose |
| --- | --- | --- |
| Tauri, Tauri build tooling and official plugins | MIT OR Apache-2.0 | Native app container, command bridge, system dialogs and packaging |
| React and React DOM | MIT | User interface |
| Vite and associated build tooling | MIT and individually recorded dependency terms | Frontend compilation |
| TypeScript | Apache-2.0 | Type checking |
| Tailwind CSS | MIT | Interface styling |
| Lucide | ISC | Interface icons |
| TanStack Virtual | MIT | Virtualized file list |
| Serde / serde_json | MIT OR Apache-2.0 | Typed local data serialization |
| RustCrypto SHA-2 | MIT OR Apache-2.0 | Copy-integrity hashes |

Consult the inventory for whether a particular package/version is actually resolved, and for additional transitive components. System fonts are requested through platform CSS rather than redistributed. Platform WebViews and OS frameworks are supplied by the operating system; no commercial recovery engine is bundled.

Original Memory Regainer logo and interface assets are included under GPL-3.0-or-later. No TestDisk, PhotoRec, GNU ddrescue or Sleuth Kit executable or library is distributed in this release. Their evaluation is recorded in [LEGAL.md](LEGAL.md).

To reproduce the notice inventory after dependency installation:

```sh
python3 scripts/dependency_inventory.py
```

Review any reported unknown expression before distribution. Retain this file, upstream notices, the project license, and corresponding-source access when redistributing the application.
