#!/usr/bin/env python3
"""Collect only completed native installers; fail if the expected build is absent."""
from pathlib import Path
import hashlib
import shutil
import sys

target, label = sys.argv[1:]
root = Path(__file__).resolve().parents[1]
bundle = root / "target" / target / "release" / "bundle"
extension = ".exe" if "windows" in target else ".dmg"
files = sorted(p for p in bundle.rglob(f"*{extension}") if p.is_file())
if not files:
    raise SystemExit(f"No {extension} installer found under {bundle}")
destination = root / "release-artifacts"
destination.mkdir(exist_ok=True)
checksums = []
for source in files:
    output = destination / source.name
    shutil.copy2(source, output)
    digest = hashlib.sha256()
    with output.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    checksums.append(f"{digest.hexdigest()}  {output.name}")
    print(f"Collected {output.name} ({output.stat().st_size} bytes)")
(destination / f"{label}-SHA256SUMS.txt").write_text("\n".join(checksums) + "\n", encoding="utf-8")
