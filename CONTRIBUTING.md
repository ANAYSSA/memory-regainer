# Contributing

Contributions are licensed under GPL-3.0-or-later. Submit only code, documentation, assets and test data that you are entitled to contribute. Keep upstream notices intact. Do not copy proprietary recovery implementations or assets.

Use synthetic disk images and disposable test folders. Never run a test suite against a personal physical drive. A new parser or carving handler needs explicit bounds, checked arithmetic, malformed-input cases, cancellation behavior, a reproducible fixture and byte-for-byte recovery evidence. List unsupported cases rather than treating filesystem detection as recovery support.

Before adding a dependency, inspect its versioned license and source, decide whether its terms permit distribution, record obligations, and update `docs/dependency-inventory.json` and retained notices using `python3 scripts/dependency_inventory.py`. That script checks declared metadata; review remains necessary for license exceptions and bundled native code.

From the repository root:

```sh
cd apps/desktop
npm ci
npm run build
npm test -- --run
cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/dependency_inventory.py --check
```

Describe what changed, the exact test commands/results, how output safety is preserved, and any platform you did not verify. Security-sensitive findings follow [SECURITY.md](SECURITY.md). Cosmetic changes do not require new recovery tests, but changed parser behavior does.
