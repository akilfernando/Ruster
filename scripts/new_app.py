#!/usr/bin/env python3
"""Export an editable standalone application workspace, with no checkout dependency."""

from __future__ import annotations

import argparse
import ctypes
import errno
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib

from package_app import APPS, KIT_CRATES, ROOT, app_spec, cargo, file_manifest, notices, put, read_regular

CRATES = KIT_CRATES
ROOT_FILES = ("Cargo.lock", "rust-toolchain.toml", "LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt")
VENDOR_FILES = ("Cargo.toml", "Cargo.toml.orig", "build.rs", "README.md", "LICENSE.MIT", "LICENSE.APACHE", "RUSTER-PATCH.md")
PACKAGING_FILES = ("scripts/package_app.py", "scripts/dependency_notices.py")
MLUA_FILES = ("Cargo.toml", "Cargo.toml.orig", "README.md", "README.upstream", "FAQ.upstream", "CHANGELOG.upstream", "LICENSE", "RUSTER-PATCH.md", "RUSTER-local.patch", ".cargo_vcs_info.json", "rustfmt.toml", "tarpaulin.toml", "typos.toml", "codecov.yml", ".gitignore")


def copy_tree(root: Path, relative: str, stage: Path, extensions: set[str]) -> None:
    base = root / relative
    if base.is_symlink() or not base.is_dir():
        raise ValueError(f"missing/unsafe source directory: {relative}")
    for path in sorted(base.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"symlink is not allowed in source tree: {path.relative_to(root)}")
        if path.is_file() and path.suffix in extensions and not any(
            part.startswith(".") or part in {"target", "node_modules", "__pycache__"}
            for part in path.relative_to(base).parts
        ):
            name = path.relative_to(root).as_posix()
            put(stage, name, read_regular(root, name))


def publish_tree(stage: Path, output: Path) -> None:
    # Linux renameat2(RENAME_NOREPLACE): atomically expose the whole tree and never
    # replace any existing file, directory or dangling symlink, including races.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = getattr(libc, "renameat2", None)
    if rename is None:
        raise ValueError("atomic source export requires Linux renameat2")
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(stage), -100, os.fsencode(output), 1) != 0:
        number = ctypes.get_errno()
        if number == errno.EEXIST:
            raise ValueError(f"refusing to overwrite {output}")
        raise OSError(number, os.strerror(number), str(output))


def export(root: Path, output: Path, normalize_lock: bool = True, app: str = "yard") -> dict:
    spec = app_spec(app)
    output = output.absolute()
    output = output.parent.resolve(strict=True) / output.name
    if output.exists() or output.is_symlink():
        raise ValueError(f"refusing to overwrite {output}")
    if output.is_relative_to(root.resolve()):
        raise ValueError("export destination must be outside the checkout")
    notices(root, [])  # Validate supplemental source text hashes before publishing.
    with tempfile.TemporaryDirectory(prefix=".ruster-source-", dir=output.parent) as temporary:
        stage = Path(temporary) / "payload"
        stage.mkdir()
        workspace = tomllib.loads(read_regular(root, "Cargo.toml").decode())
        members = ["crates/" + name for name in spec.crates] + [spec.sample]
        excluded = ["third_party/wgpu-hal"] + (["third_party/mlua"] if app == "motion-lab" else [])
        package = workspace["workspace"]["package"]
        # Deliberately synthesize only the inherited fields needed by this kit.
        lines = ["[workspace]", "members = " + json.dumps(members), 'resolver = "2"',
                 'exclude = ' + json.dumps(excluded), "", "[workspace.package]"]
        lines += [f"{key} = {json.dumps(package[key])}" for key in
                  ("version", "edition", "publish", "license", "repository")]
        lines += ["", "[workspace.lints.rust]", 'unsafe_code = "forbid"', "",
                  "[patch.crates-io]", 'wgpu-hal = { path = "third_party/wgpu-hal" }', ""]
        put(stage, "Cargo.toml", "\n".join(lines).encode())
        for name in ROOT_FILES:
            put(stage, name, read_regular(root, name))
        for name in PACKAGING_FILES:
            put(stage, name, read_regular(root, name))
        put(stage, spec.binary_readme, read_regular(root, spec.binary_readme))
        for name in spec.crates:
            relative = "crates/" + name
            put(stage, relative + "/Cargo.toml", read_regular(root, relative + "/Cargo.toml"))
            copy_tree(root, relative + "/src", stage, {".rs", ".wgsl"})
            if (root / relative / "templates").is_dir():
                copy_tree(root, relative + "/templates", stage, {".lua"})
            if (root / relative / "tests").is_dir():
                copy_tree(root, relative + "/tests", stage, {".rs", ".json", ".obj", ".wgsl"})
        sample = spec.sample
        put(stage, sample + "/Cargo.toml", read_regular(root, sample + "/Cargo.toml"))
        copy_tree(root, sample + "/src", stage, {".rs", ".wgsl"})
        if (root / sample / "tests").is_dir():
            copy_tree(root, sample + "/tests", stage, {".rs", ".json", ".obj", ".wgsl"})
        # The shared core's existing compatibility test opens this owned fixture.
        put(stage, "examples/logistics.scenario.json", read_regular(root, "examples/logistics.scenario.json"))
        for name in spec.resources:
            relative = sample + "/resources/" + name
            put(stage, relative, read_regular(root, relative))
        for name in VENDOR_FILES:
            relative = "third_party/wgpu-hal/" + name
            put(stage, relative, read_regular(root, relative))
        copy_tree(root, "third_party/wgpu-hal/src", stage, {".rs", ".wgsl", ".h", ".mm", ".frag", ".vert"})
        copy_tree(root, "third_party/wgpu-hal/examples", stage, {".rs", ".wgsl", ".html"})
        if app == "motion-lab":
            for name in MLUA_FILES:
                relative = "third_party/mlua/" + name
                put(stage, relative, read_regular(root, relative))
            for directory in ("src", "docs", "tests", "examples", "benches"):
                copy_tree(root, "third_party/mlua/" + directory, stage,
                          {".rs", ".md", ".lua", ".luau", ".stderr", ".toml", ".json"})
        # Preserve the checked-in license supplements and provenance, not local cache paths.
        put(stage, "packaging/license-sources.json", read_regular(root, "packaging/license-sources.json"))
        sources = json.loads(read_regular(root, "packaging/license-sources.json"))
        for text in sources["texts"]:
            relative = "packaging/" + text["path"]
            put(stage, relative, read_regular(root, relative))
        put(stage, "README.md", read_regular(root, spec.source_readme))
        if normalize_lock:
            # Remove irrelevant workspace-only lock entries without updating any
            # existing selected versions; offline means this exports cached deps.
            subprocess.run([cargo(), "metadata", "--offline", "--format-version", "1"],
                           cwd=stage, check=True, stdout=subprocess.DEVNULL)
        manifest = {"schema": 1, "kind": "editable-source-template", "application": spec.package, "files": file_manifest(stage)}
        put(stage, "source-manifest.json", (json.dumps(manifest, indent=2) + "\n").encode())
        publish_tree(stage, output)
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", choices=APPS, default="yard", help="owned example to export (default yard)")
    parser.add_argument("destination", type=Path, help="new directory outside the checkout")
    args = parser.parse_args()
    try:
        manifest = export(ROOT, args.destination, app=args.app)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"new app: {exc}", file=sys.stderr)
        return 2
    print(f"created {args.destination}: {len(manifest['files'])} source files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
