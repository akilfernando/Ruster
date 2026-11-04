#!/usr/bin/env python3
"""Create an atomic, relocatable Linux development archive of an owned example."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile

import dependency_notices

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-unknown-linux-gnu"
APP = "ruster-yard"
# Resource additions require deliberate review, never a recursive project copy.
RESOURCES = ("truck.obj",)


@dataclass(frozen=True)
class AppSpec:
    package: str
    sample: str
    crates: tuple[str, ...]
    resources: tuple[str, ...]
    source_readme: str
    binary_readme: str


KIT_CRATES = ("ruster-app", "ruster-assets", "ruster-scene", "ruster-render", "ruster-core")
APPS = {
    "yard": AppSpec(APP, "examples/yard-app", KIT_CRATES, RESOURCES,
                    "packaging/SOURCE-README.md", "packaging/BINARY-README.txt"),
    "motion-lab": AppSpec("ruster-motion-lab", "examples/motion-lab",
                          KIT_CRATES + ("ruster-script", "ruster-behavior"),
                          ("motion.rhai", "generic.scene.json"),
                          "packaging/MOTION-SOURCE-README.md", "packaging/MOTION-BINARY-README.txt"),
}


def app_spec(name: str) -> AppSpec:
    try:
        return APPS[name]
    except KeyError as exc:
        raise ValueError(f"unknown application {name!r}; choose {', '.join(APPS)}") from exc


def read_regular(root: Path, relative: str) -> bytes:
    path = root / relative
    if Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise ValueError(f"unsafe payload path: {relative}")
    cursor = root
    for part in Path(relative).parts:
        cursor = cursor / part
        if cursor.is_symlink():
            raise ValueError(f"symlink is not allowed in payload: {relative}")
    if not stat.S_ISREG(path.stat().st_mode):
        raise ValueError(f"not a regular payload file: {relative}")
    return path.read_bytes()


def put(stage: Path, relative: str, data: bytes, executable: bool = False) -> None:
    path = stage / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(0o755 if executable else 0o644)


def cargo() -> str:
    found = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
    if not Path(found).is_file():
        raise ValueError("cargo not found")
    return found


def metadata(root: Path, application: str = APP) -> dict:
    result = json.loads(subprocess.check_output(
        [cargo(), "metadata", "--locked", "--offline", "--format-version", "1",
         "--filter-platform", TARGET], cwd=root, text=True))
    # Workspace metadata unifies features with unrelated editor/viewer members.
    # Cargo tree's selected package view avoids shipping those as app dependencies.
    tree = subprocess.check_output([cargo(), "tree", "--locked", "--offline", "-p", application,
                                    "--target", TARGET, "--edges", "normal,build",
                                    "--prefix", "none", "--format", "{p}"], cwd=root, text=True)
    result["selected_packages"] = {tuple(match) for match in
                                   re.findall(r"^([A-Za-z0-9_-]+) v([^\s]+)", tree, re.MULTILINE)}
    return result


def active_packages(meta: dict, application: str = APP) -> list[dict]:
    packages = {p["id"]: p for p in meta["packages"]}
    if "selected_packages" in meta:
        return sorted((p for p in packages.values() if (p["name"], p["version"]) in meta["selected_packages"]),
                      key=lambda p: (p["name"], p["version"]))
    roots = [p["id"] for p in meta["packages"] if p["name"] == application]
    if len(roots) != 1:
        raise ValueError(f"expected exactly one {application} package")
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen: set[str] = set()
    pending = roots[:]
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(k["kind"] != "dev" for k in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    return sorted((packages[k] for k in seen), key=lambda p: (p["name"], p["version"]))


def notices(root: Path, packages: list[dict]) -> tuple[bytes, list[dict]]:
    supplemental = json.loads(read_regular(root, "packaging/license-sources.json"))
    covered: set[tuple[str, str]] = set()
    parts = ["SUPPLEMENTAL UPSTREAM LICENSE AND NOTICE TEXTS\n\n"]
    for item in supplemental["texts"]:
        data = read_regular(root / "packaging", item["path"])
        if hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise ValueError(f"supplemental license checksum mismatch: {item['path']}")
        covered.update((p["name"], p["version"]) for p in item["packages"])
        parts += [json.dumps(item, sort_keys=True) + "\n\n", data.decode("utf-8"), "\n\n"]
    inventory = []
    missing = []
    workspace = set(KIT_CRATES) | {"ruster-script", "ruster-behavior"} | {spec.package for spec in APPS.values()}
    for package in packages:
        if package["name"] in workspace and not package.get("source"):
            continue
        texts, _ = dependency_notices.package_texts(package)
        if not texts and (package["name"], package["version"]) not in covered:
            missing.append(f"{package['name']} {package['version']}")
        # Include actual active source texts, even when full-workspace inventory is stale.
        for relative, data in texts:
            parts += [f"{package['name']} {package['version']}: {relative}\n\n", data.decode("utf-8"), "\n\n"]
        inventory.append({k: package.get(k) for k in ("name", "version", "license", "repository")})
    if missing:
        raise ValueError("active Linux dependencies lack readable notices: " + ", ".join(missing))
    return "".join(parts).encode(), inventory


def elf_requirements(binary: Path) -> dict:
    header = subprocess.check_output(["readelf", "-h", str(binary)], text=True)
    if "ELF64" not in header or "X86-64" not in header:
        raise ValueError("only Linux x86_64 ELF binaries are packaged")
    dynamic = subprocess.check_output(["readelf", "-d", str(binary)], text=True)
    if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
        raise ValueError("binary RPATH/RUNPATH needs explicit distribution review")
    program = subprocess.check_output(["readelf", "-l", str(binary)], text=True)
    versions = subprocess.check_output(["readelf", "--version-info", str(binary)], text=True)
    interpreter = re.search(r"Requesting program interpreter: ([^\]]+)", program)
    return {"needed": sorted(set(re.findall(r"Shared library: \[([^\]]+)\]", dynamic))),
            "interpreter": interpreter.group(1) if interpreter else None,
            "glibc_symbols": sorted(set(re.findall(r"\bGLIBC_[0-9.]+", versions)))}


def file_manifest(stage: Path) -> list[dict]:
    return [{"path": p.relative_to(stage).as_posix(), "bytes": p.stat().st_size,
             "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in sorted(stage.rglob("*")) if p.is_file()]


def publish_archive(stage: Path, output: Path, application: str = APP) -> None:
    # Hard-link publication is atomic and fails if output already exists, including symlinks.
    # The temporary archive lives on the destination filesystem.
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError(f"refusing to overwrite {output}")
    with tempfile.NamedTemporaryFile(prefix=".ruster-package-", dir=output.parent) as temp:
        with tarfile.open(fileobj=temp, mode="w:gz", format=tarfile.PAX_FORMAT) as archive:
            for path in sorted(stage.rglob("*")):
                if not path.is_file() or path.is_symlink():
                    continue
                info = archive.gettarinfo(str(path), arcname=application + "/" + path.relative_to(stage).as_posix())
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ""
                with path.open("rb") as stream:
                    archive.addfile(info, stream)
        temp.flush()
        os.fsync(temp.fileno())
        os.link(temp.name, output)


def package(root: Path, binary: Path, output: Path, app: str = "yard") -> dict:
    spec = app_spec(app)
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("development packaging is currently Linux x86_64 only")
    if not os.access(binary, os.X_OK) or binary.is_symlink():
        raise ValueError("binary must be a regular executable, not a symlink")
    binary_data = read_regular(binary.parent, binary.name)
    if any(prefix in binary_data for prefix in (os.fsencode(root.resolve()), os.fsencode(Path.home()))):
        raise ValueError("binary contains checkout/home paths; rebuild release with --remap-path-prefix and no debuginfo")
    requirements = elf_requirements(binary)
    active = active_packages(metadata(root, spec.package), spec.package)
    license_text, dependencies = notices(root, active)
    with tempfile.TemporaryDirectory(prefix="ruster-payload-") as temporary:
        stage = Path(temporary)
        put(stage, spec.package, binary_data, executable=True)
        for name in spec.resources:
            put(stage, "resources/" + name, read_regular(root / spec.sample / "resources", name))
        for name in ("LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt"):
            put(stage, name, read_regular(root, name))
        put(stage, "SUPPLEMENTAL_NOTICES.txt", license_text)
        put(stage, "LICENSE-SOURCES.json", read_regular(root, "packaging/license-sources.json"))
        put(stage, "README.txt", read_regular(root, spec.binary_readme))
        manifest = {"schema": 1, "application": spec.package, "target": TARGET,
                    "kind": "development-package", "elf": requirements,
                    "dependencies": dependencies, "files": file_manifest(stage)}
        put(stage, "manifest.json", (json.dumps(manifest, indent=2) + "\n").encode())
        publish_archive(stage, output, spec.package)
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", choices=APPS, default="yard", help="owned example to package (default yard)")
    parser.add_argument("--binary", type=Path, help="built executable; defaults to the selected app release path")
    parser.add_argument("--output", type=Path, required=True, help="new .tar.gz path; never overwritten")
    args = parser.parse_args()
    try:
        spec = app_spec(args.app)
        manifest = package(ROOT, args.binary or ROOT / "target/release" / spec.package, args.output, args.app)
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"package app: {exc}", file=sys.stderr)
        return 2
    print(f"created {args.output}: {len(manifest['files'])} payload files; {len(manifest['dependencies'])} external dependency versions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
