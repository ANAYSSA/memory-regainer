#!/usr/bin/env python3
"""Record resolved licenses/origins and retain upstream notices without adding dependencies.

Run after npm ci and cargo fetch. --check checks license expressions only and does
not rewrite the committed inventory. This is a metadata gate, not legal advice.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    "0BSD", "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause",
    "BSD-2-Clause-Patent", "ISC", "Unlicense", "CC0-1.0", "Zlib", "BSL-1.0",
    "Unicode-3.0", "Unicode-DFS-2016", "MPL-2.0", "GPL-3.0-or-later",
    "Apache-2.0 WITH LLVM-exception", "CC-BY-4.0", "BlueOak-1.0.0",
}


def approved(expression):
    # Evaluate SPDX alternatives as alternatives; AND requires every component.
    expression = expression.strip()
    # Cargo historically accepted slash-separated alternatives; these exact
    # alternatives have been reviewed as their equivalent OR expressions.
    if "/" in expression:
        return all(part.strip() in ALLOWED for part in expression.split("/"))
    if expression in ALLOWED:
        return True
    # All expressions in this project's lockfiles use simple grouped OR/AND.
    if expression.startswith("(") and expression.endswith(")"):
        depth = 0
        encloses_all = True
        for char in expression[:-1]:
            depth += (char == "(") - (char == ")")
            if depth == 0:
                encloses_all = False
                break
        if encloses_all:
            expression = expression[1:-1].strip()
    depth = 0
    for operator in (" OR ", " AND "):
        pieces, previous = [], 0
        depth = 0
        i = 0
        while i < len(expression):
            if expression[i] == "(":
                depth += 1
            elif expression[i] == ")":
                depth -= 1
            elif depth == 0 and expression.startswith(operator, i):
                pieces.append(expression[previous:i])
                previous = i + len(operator)
                i += len(operator) - 1
            i += 1
        if pieces:
            pieces.append(expression[previous:])
            return (any if operator == " OR " else all)(approved(p) for p in pieces)
    return False


def notices(directory, output, key):
    found = []
    candidates = list(directory.iterdir())
    for sub in ("licenses", "license", "LICENSES"):
        folder = directory / sub
        if folder.is_dir():
            candidates.extend(folder.rglob("*"))
    for item in sorted(set(candidates)):
        if not item.is_file() or item.stat().st_size > 1024 * 1024:
            continue
        if item.parent == directory and not re.match(r"^(licen[sc]e|copying|notice|copyright)", item.name, re.I):
            continue
        relative = str(item.relative_to(directory)).replace("\\", "/")
        content = item.read_bytes()
        record = {"file": relative, "sha256": hashlib.sha256(content).hexdigest()}
        if output is not None:
            target = output / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
        found.append(record)
    # Some registry archives omit their workspace-root license. Retain the
    # exact-version upstream supplement and its provenance when collected.
    supplements = ROOT / "docs" / "upstream-license-supplements" / key
    if supplements.is_dir():
        for item in sorted(supplements.iterdir()):
            if not item.is_file():
                continue
            relative = "upstream-" + item.name
            content = item.read_bytes()
            if output is not None:
                output.mkdir(parents=True, exist_ok=True)
                (output / relative).write_bytes(content)
            found.append({"file": relative, "sha256": hashlib.sha256(content).hexdigest()})
    # Retain published license/author metadata and README when no standalone
    # license was shipped. No text or copyright owner is invented here.
    if not any("PROVENANCE" not in n["file"] for n in found):
        for name in ("README.md", "readme.md", "README", "Cargo.toml.orig", "package.json", "AUTHORS"):
            item = directory / name
            if not item.is_file():
                continue
            content = item.read_bytes()
            relative = "UPSTREAM-" + name
            if output is not None:
                output.mkdir(parents=True, exist_ok=True)
                (output / relative).write_bytes(content)
            found.append({"file": relative, "sha256": hashlib.sha256(content).hexdigest()})
    return found


def collect(write):
    notice_root = ROOT / "docs" / "third-party-licenses"
    if write:
        if notice_root.exists():
            shutil.rmtree(notice_root)
        notice_root.mkdir(parents=True)
    components = []
    cargo = os.environ.get("CARGO", "cargo")
    metadata = json.loads(subprocess.check_output([cargo, "metadata", "--format-version=1", "--locked"], cwd=ROOT))
    members = set(metadata["workspace_members"])
    direct = set()
    for package in metadata["packages"]:
        if package["id"] in members:
            direct.update(d["name"] for d in package["dependencies"])
    for package in metadata["packages"]:
        if package["id"] in members:
            continue
        key = f"rust-{package['name']}-{package['version']}"
        folder = Path(package["manifest_path"]).parent
        components.append({
            "ecosystem": "cargo", "name": package["name"], "version": package["version"],
            "license": package.get("license") or "UNKNOWN",
            "origin": package.get("repository") or package.get("homepage") or f"https://crates.io/crates/{package['name']}/{package['version']}",
            "source_archive": f"https://crates.io/api/v1/crates/{package['name']}/{package['version']}/download",
            "purpose": "direct Rust dependency" if package["name"] in direct else "transitive Rust build/runtime dependency",
            "notices": notices(folder, notice_root / key if write else None, key),
        })
    desktop = ROOT / "apps" / "desktop"
    lock = json.loads((desktop / "package-lock.json").read_text())
    manifest = json.loads((desktop / "package.json").read_text())
    for path, package in sorted(lock["packages"].items()):
        if not path or "node_modules/" not in path:
            continue
        folder = desktop / path
        if not (folder / "package.json").is_file():
            # Optional platform-specific npm packages absent on this host remain
            # in the inventory with lockfile metadata and their package origin.
            details = package
        else:
            details = json.loads((folder / "package.json").read_text())
        name = details.get("name") or path.rsplit("node_modules/", 1)[-1]
        version = package["version"]
        license_value = details.get("license") or package.get("license") or "UNKNOWN"
        if isinstance(license_value, dict):
            license_value = license_value.get("type", "UNKNOWN")
        key = "npm-" + name.replace("@", "").replace("/", "-") + "-" + version
        components.append({
            "ecosystem": "npm", "name": name, "version": version, "license": license_value,
            "origin": package.get("resolved") or f"https://www.npmjs.com/package/{name}/v/{version}",
            "purpose": "frontend runtime" if name in manifest.get("dependencies", {}) else "frontend development/transitive dependency",
            "development_only": package.get("dev", False),
            "installed_on_inventory_host": folder.is_dir(),
            "notices": notices(folder, notice_root / key if write else None, key) if folder.is_dir() else [],
        })
    components.sort(key=lambda p: (p["ecosystem"], p["name"], p["version"]))
    rejected = [p for p in components if not approved(p["license"])]
    if write:
        (ROOT / "docs" / "dependency-inventory.json").write_text(json.dumps({
            "format": "Memory Regainer dependency inventory 1",
            "scope": "All resolved Cargo packages and npm lock packages; includes build-only and optional packages. Not every listed package is shipped in every installer.",
            "components": components,
        }, indent=2) + "\n", encoding="utf-8")
    print(f"Inspected {len(components)} dependency records; {len(rejected)} expressions require review.")
    for package in rejected:
        print(f"REVIEW {package['ecosystem']} {package['name']} {package['version']}: {package['license']}")
    return not rejected


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    raise SystemExit(0 if collect(not args.check) else 1)
