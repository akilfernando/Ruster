#!/usr/bin/env python3
"""Generate a locked Cargo dependency inventory and bundled source notices."""

from __future__ import annotations

import argparse
import difflib
import json
from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
INVENTORY = ROOT / "docs" / "DEPENDENCIES.md"
NOTICES = ROOT / "THIRD_PARTY_NOTICES.txt"


def cargo_metadata() -> dict:
    cargo = shutil.which("cargo")
    if not cargo and Path.home().joinpath(".cargo/bin/cargo").is_file():
        cargo = str(Path.home() / ".cargo/bin/cargo")
    if not cargo:
        raise RuntimeError("cargo was not found on PATH or ~/.cargo/bin")
    result = subprocess.run(
        [cargo, "metadata", "--locked", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def external_packages(metadata: dict) -> list[dict]:
    return sorted(
        (p for p in metadata["packages"] if p.get("source")),
        key=lambda p: (p["name"].casefold(), p["name"], p["version"], p["source"]),
    )


def license_named(path: Path) -> bool:
    name = path.name.casefold()
    return (
        name.startswith(("license", "licence", "copying", "notice"))
        or name.startswith(("ofl", "ufl"))
        or name in {"copyright", "patents"}
    )


def package_texts(package: dict) -> tuple[list[tuple[str, bytes]], list[str]]:
    manifest = Path(package["manifest_path"])
    base = manifest.parent
    found: dict[str, Path] = {}
    declared = package.get("license_file")
    if declared:
        license_path = (base / declared).resolve()
        try:
            license_path.relative_to(base.resolve())
        except ValueError:
            license_path = None
        if license_path and license_path.is_file():
            found[license_path.relative_to(base.resolve()).as_posix()] = license_path

    if base.is_dir():
        for path in base.rglob("*"):
            if path.is_file() and license_named(path):
                try:
                    relative = path.relative_to(base).as_posix()
                    resolved = path.resolve()
                    if not resolved.is_relative_to(base.resolve()):
                        continue
                    # Exclude generated build and VCS trees.
                    if any(part in {"target", ".git"} for part in path.relative_to(base).parts):
                        continue
                    found.setdefault(relative, resolved)
                except ValueError:
                    continue

    texts: list[tuple[str, bytes]] = []
    missing: list[str] = []
    for relative, path in sorted(found.items()):
        try:
            data = path.read_bytes()
            data.decode("utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if data.strip():
            texts.append((relative, data))
    if not texts:
        missing.append(f"{package['name']} {package['version']}")
    return texts, missing


def outputs(metadata: dict) -> tuple[str, str, dict]:
    packages = external_packages(metadata)
    missing: list[str] = []
    content_groups: dict[bytes, list[tuple[str, str, str]]] = {}
    for package in packages:
        texts, package_missing = package_texts(package)
        for relative, data in texts:
            content_groups.setdefault(data, []).append((package["name"], package["version"], relative))
        missing.extend(package_missing)

    lines = [
        "# Third-party dependency inventory",
        "",
        "Generated from `cargo metadata --locked --format-version 1` by `scripts/dependency_notices.py`.",
        "This inventory covers every external package in the locked workspace resolution, including optional and target-specific packages; it is not a list of what one particular binary includes.",
        "Cargo's declared SPDX license expression is metadata supplied by each package. Its presence is not an independent license review or certification.",
        "Project source licensing does not change the licenses of dependencies.",
        "",
        f"External locked package versions: **{len(packages)}**. Packages with no discovered readable license text: **{len(missing)}**.",
        "",
        "| Package | Version | Cargo source | Declared license expression | Upstream repository |",
        "| --- | --- | --- | --- | --- |",
    ]
    for p in packages:
        vals = [p["name"], p["version"], p.get("source") or "", p.get("license") or "(not declared)", p.get("repository") or "(not declared)"]
        vals = [v.replace("|", "\\|").replace("\n", " ") for v in vals]
        repo = f"[{vals[4]}]({vals[4]})" if p.get("repository") else vals[4]
        lines.append(f"| {vals[0]} | {vals[1]} | `{vals[2]}` | `{vals[3]}` | {repo} |")
    lines.extend(["", "## Source text coverage", "", "Readable UTF-8 license, notice, copying, or font-license files found in package source trees are reproduced in [`THIRD_PARTY_NOTICES.txt`](../THIRD_PARTY_NOTICES.txt). Identical file contents are bundled once, with all package/path associations listed. Binary and non-UTF-8 candidates are omitted.", ""])
    font_packages = [p for p in packages if p["name"] == "epaint_default_fonts"]
    if font_packages:
        font = font_packages[0]
        lines.extend(["## Notable declared license expressions", "", f"`{font['name']} {font['version']}` declares `{font.get('license')}`. Its package source contains `fonts/OFL.txt` and `fonts/UFL.txt`; both texts are included in the notices bundle. This records package metadata and source files, not a determination of which terms apply to a particular distribution or font use.", ""])
    if missing:
        lines.append("No readable matching source text was found for the following packages; metadata declarations are retained above and absence of bundled text is not interpreted as permission:")
        lines.append("")
        lines.extend(f"- `{item}`" for item in missing)
    else:
        lines.append("A readable matching source text was found for every package.")
    lines.extend(["", "Regenerate after dependency or lockfile changes with `python3 scripts/dependency_notices.py`. Use `--check` to verify committed output without changing files. The generator runs locked Cargo metadata by default; `--metadata PATH` accepts a saved Cargo metadata JSON document for offline/reproducible regeneration.", ""])

    notice_lines = [
        "THIRD-PARTY LICENSE AND NOTICE TEXTS",
        "",
        "Original UTF-8 text files from resolved external Cargo packages. Contents are preserved byte-for-byte within each section; only section headers are added. Identical contents appear once, with all known package and relative-path associations listed.",
        "",
        "Declared Cargo license expressions are recorded separately in docs/DEPENDENCIES.md and are not a substitute for reviewing these texts and applicable obligations.",
        "",
    ]
    notice_parts = ["\n".join(notice_lines)]
    for content in sorted(content_groups, key=lambda b: (min(content_groups[b]), b)):
        associations = sorted(set(content_groups[content]))
        section = ["=" * 78, "Associated package source files:"]
        section.extend(f"  {name} {version}: {relative}" for name, version, relative in associations)
        section.extend(["=" * 78, ""])
        notice_parts.append("\n".join(section) + "\n")
        notice_parts.append(content.decode("utf-8"))
        if not content.endswith(b"\n"):
            notice_parts.append("\n")
        notice_parts.append("\n")

    stats = {"packages": len(packages), "missing": missing, "text_groups": len(content_groups), "associations": sum(len(set(v)) for v in content_groups.values())}
    return "\n".join(lines), "".join(notice_parts), stats


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, help="read saved Cargo metadata JSON instead of invoking Cargo")
    parser.add_argument("--check", action="store_true", help="fail if generated files differ; do not write")
    args = parser.parse_args()
    try:
        metadata = json.loads(args.metadata.read_text(encoding="utf-8")) if args.metadata else cargo_metadata()
        inventory, notices, stats = outputs(metadata)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"dependency notices: {exc}", file=sys.stderr)
        return 2

    expected = {INVENTORY: inventory, NOTICES: notices}
    mismatches = []
    for path, text in expected.items():
        if args.check:
            current = path.read_bytes().decode("utf-8") if path.exists() else ""
            if current != text:
                mismatches.append((path, current, text))
        else:
            path.write_text(text, encoding="utf-8", newline="\n")
    if mismatches:
        for path, current, expected_text in mismatches:
            print(f"--- {path.relative_to(ROOT)} (current)\n+++ {path.relative_to(ROOT)} (generated)")
            sys.stdout.writelines(difflib.unified_diff(current.splitlines(True), expected_text.splitlines(True)))
        return 1
    print(f"dependency notices: {stats['packages']} package versions; {stats['text_groups']} unique texts / {stats['associations']} package-path associations; {len(stats['missing'])} missing readable texts")
    if stats["missing"]:
        print("missing: " + ", ".join(stats["missing"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
