#!/usr/bin/env python3
"""Prepare a reusable Linux runtime kit or package bounded creator-owned content."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import stat
import struct
import subprocess
import sys
import tempfile
import time

# A prepared kit is immutable: importing its copied helpers must not create extras.
sys.dont_write_bytecode = True
import package_app

ROOT = Path(__file__).resolve().parent.parent
MIB = 1024 * 1024
MAX_KIT_FILES = 32
MAX_KIT_BYTES = 300 * MIB
MAX_BINARY_BYTES = 256 * MIB
MAX_METADATA_BYTES = MIB
MAX_NOTICE_BYTES = 8 * MIB
MAX_MESHES = 64
PROFILE = {"format": "ruster-runtime", "version": 1,
           "descriptor_format": "ruster-application", "descriptor_version": 1,
           "os": "linux", "arch": "x86_64"}
CAMERA_PROFILE = {**PROFILE, "capabilities": ["initial_camera"]}
LICENSE_FILES = ("LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt",
                 "SUPPLEMENTAL_NOTICES.txt", "LICENSE-SOURCES.json")
HELPER_FILES = ("scripts/package_creation.py", "scripts/package_app.py",
                "scripts/dependency_notices.py")
KIT_FILES = frozenset(("runtime", "README.txt") + LICENSE_FILES + HELPER_FILES)
HASH = re.compile(r"[0-9a-f]{64}\Z")
MESH_PATH = re.compile(r"meshes/[0-9]{4}\.obj\Z")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def kit_file_limit(name: str) -> int:
    if name == "runtime":
        return MAX_BINARY_BYTES
    if name in ("THIRD_PARTY_NOTICES.txt", "SUPPLEMENTAL_NOTICES.txt"):
        return MAX_NOTICE_BYTES
    return MAX_METADATA_BYTES


def encoded(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode()


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def decode(data: bytes, label: str) -> dict:
    def invalid_constant(value):
        raise ValueError(f"nonfinite JSON number: {value}")
    try:
        value = json.loads(data, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    except (UnicodeError, ValueError, RecursionError) as exc:
        raise ValueError(f"invalid {label}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a JSON object")
    return value


def exact_fields(value: object, fields: set[str], label: str) -> None:
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError(f"invalid {label} fields")


def cancelled(cancel_file: Path | None) -> None:
    if cancel_file is not None and (cancel_file.exists() or cancel_file.is_symlink()):
        raise ValueError("creation packaging cancelled")


def relative_parts(relative: str) -> list[str]:
    if (not isinstance(relative, str) or not relative or len(relative.encode()) > 256
            or "\\" in relative or any(ord(c) < 32 or ord(c) == 127 for c in relative)
            or relative.startswith("/") or any(p in ("", ".", "..") for p in relative.split("/"))):
        raise ValueError(f"unsafe payload path: {relative!r}")
    return relative.split("/")


def read_bounded(root: Path, relative: str, maximum: int, executable: bool = False) -> bytes:
    """Open every component without symlinks and bound the actual bytes read."""
    parts = relative_parts(relative)
    directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        descriptor = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        with os.fdopen(descriptor, "rb") as stream:
            metadata = os.fstat(stream.fileno())
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError(f"not a regular payload file: {relative}")
            if executable and not metadata.st_mode & 0o111:
                raise ValueError(f"payload is not executable: {relative}")
            if metadata.st_size > maximum:
                raise ValueError(f"payload exceeds byte limit: {relative}")
            data = stream.read(maximum + 1)
            if len(data) > maximum:
                raise ValueError(f"payload exceeds byte limit: {relative}")
            return data
    finally:
        os.close(directory)


def inventory(root: Path) -> set[str]:
    """The kit has one known subdirectory; never recursively walk arbitrary trees."""
    found = set()
    if root.is_symlink() or not root.is_dir():
        raise ValueError("runtime kit must be a regular directory")
    for parent, prefix in ((root, ""), (root / "scripts", "scripts/")):
        with os.scandir(parent) as entries:
            for entry in entries:
                if not prefix and entry.name == "scripts":
                    if not entry.is_dir(follow_symlinks=False):
                        raise ValueError("runtime kit scripts must be a regular directory")
                    continue
                if not entry.is_file(follow_symlinks=False):
                    raise ValueError("runtime kit contains a symlink or nonregular file")
                found.add(prefix + entry.name)
                if len(found) > MAX_KIT_FILES:
                    raise ValueError("runtime kit exceeds file limit")
    return found


def check_profile(profile: object) -> None:
    if (profile not in (PROFILE, CAMERA_PROFILE)
            or any(type(profile.get(key)) is not type(value) for key, value in PROFILE.items())):
        raise ValueError("unsupported runtime creation profile; expected Linux x86_64 protocol version 1")


def initial_camera(value: object) -> dict | None:
    if value is None:
        return None
    exact_fields(value, {"target", "yaw", "pitch", "distance", "orthographic"}, "initial camera")
    def number(value):
        if type(value) not in (int, float):
            raise ValueError("initial camera values must be finite numbers")
        try:
            rounded = struct.unpack("f", struct.pack("f", value))[0]
        except (OverflowError, struct.error) as exc:
            raise ValueError("initial camera values exceed f32") from exc
        if not math.isfinite(rounded):
            raise ValueError("initial camera values must be finite")
        return rounded
    target = value["target"]
    if type(target) is not list or len(target) != 3:
        raise ValueError("initial camera target requires exactly three numbers")
    camera = {"target": [number(n) for n in target],
              "yaw": number(value["yaw"]), "pitch": number(value["pitch"]),
              "distance": number(value["distance"]), "orthographic": value["orthographic"]}
    if (any(abs(n) > 1e7 for n in camera["target"])
            or abs(camera["pitch"]) > number(math.pi / 2)
            or not number(0.01) <= camera["distance"] <= 1e7
            or type(camera["orthographic"]) is not bool):
        raise ValueError("invalid initial camera bounds or projection")
    return camera


def run_runtime(binary: Path, arguments: list[str], cancel_file: Path | None = None) -> dict:
    """Only fixed implementation-owned invocations; always reap our runtime child."""
    cancelled(cancel_file)
    with tempfile.TemporaryDirectory(prefix="ruster-preflight-") as temporary:
        environment = {"PATH": "/usr/bin:/bin", "HOME": temporary, "TMPDIR": temporary,
                       "LANG": "C.UTF-8"}
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            child = subprocess.Popen([str(binary.absolute()), *arguments], cwd=temporary,
                                     env=environment, stdin=subprocess.DEVNULL,
                                     stdout=stdout, stderr=stderr)
            try:
                deadline = time.monotonic() + 15
                while True:
                    cancelled(cancel_file)
                    if stdout.tell() > 4 * MIB or stderr.tell() > MIB:
                        raise ValueError("runtime preflight output exceeds byte limit")
                    if child.poll() is not None:
                        break
                    if time.monotonic() >= deadline:
                        raise ValueError("runtime preflight timed out after 15 seconds")
                    time.sleep(0.05)
                stdout.seek(0)
                stderr.seek(0)
                data = stdout.read(4 * MIB + 1)
                errors = stderr.read(MIB + 1)
                if len(data) > 4 * MIB or len(errors) > MIB:
                    raise ValueError("runtime preflight output exceeds byte limit")
                if child.returncode:
                    raise ValueError("runtime preflight failed: " + errors.decode("utf-8", "replace")[:4096])
                cancelled(cancel_file)
                return decode(data, "runtime response")
            finally:
                if child.poll() is None:
                    child.kill()
                child.wait()


def publish(stage: Path, output: Path, archive_root: str, cancel_file: Path | None = None) -> dict:
    output = output.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError(f"refusing to overwrite {output}")
    cancelled(cancel_file)
    # Compression may finish its current file. Recheck cancellation before final publication.
    with tempfile.TemporaryDirectory(prefix=".ruster-creation-", dir=output.parent) as temporary:
        prepared = Path(temporary) / "payload.tar.gz"
        package_app.publish_archive(stage, prepared, archive_root)
        digest = hashlib.sha256()
        with prepared.open("rb") as stream:
            for chunk in iter(lambda: stream.read(MIB), b""):
                cancelled(cancel_file)
                digest.update(chunk)
        receipt = {"archive": str(output), "archive_sha256": digest.hexdigest(),
                   "archive_bytes": prepared.stat().st_size}
        cancelled(cancel_file)
        try:
            os.link(prepared, output)
        except FileExistsError as exc:
            raise ValueError(f"refusing to overwrite {output}") from exc
    return receipt


def prepare_runtime(root: Path, binary: Path, output: Path) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("runtime kits currently support Linux x86_64 only")
    binary = binary.absolute()
    binary_data = read_bounded(binary.parent, binary.name, MAX_BINARY_BYTES, executable=True)
    if any(prefix in binary_data for prefix in (os.fsencode(root.resolve()), os.fsencode(Path.home()))):
        raise ValueError("binary contains checkout/home paths; rebuild release with --remap-path-prefix and no debuginfo")
    requirements = package_app.elf_requirements(binary)
    profile = run_runtime(binary, ["--creation-info"])
    check_profile(profile)
    spec = package_app.app_spec("motion-lab")
    active = package_app.active_packages(package_app.metadata(root, spec.package), spec.package)
    license_text, dependencies = package_app.notices(root, active)
    with tempfile.TemporaryDirectory(prefix="ruster-runtime-kit-") as temporary:
        stage = Path(temporary)
        package_app.put(stage, "runtime", binary_data, executable=True)
        for name in LICENSE_FILES:
            if name == "SUPPLEMENTAL_NOTICES.txt":
                data = license_text
            else:
                source = "packaging/license-sources.json" if name == "LICENSE-SOURCES.json" else name
                data = read_bounded(root, source, kit_file_limit(name))
            package_app.put(stage, name, data)
        for name in HELPER_FILES:
            package_app.put(stage, name, read_bounded(root, name, MAX_METADATA_BYTES))
        package_app.put(stage, "README.txt", b"Ruster Linux x86_64 runtime kit\n\nUse Python3 scripts/package_creation.py package --runtime-kit . --content CONTENT --output NEW.tar.gz\nPrepare content with the Ruster editor or native MCP export controller. This kit contains trusted executable/tooling code; its hashes detect changes, not code safety. Native graphics require a Vulkan-capable driver, a desktop session and the recorded ELF prerequisites.\n")
        manifest = {"schema": 1, "kind": "ruster-runtime-kit", "target": package_app.TARGET,
                    "executable": "runtime", "helper": "scripts/package_creation.py", "profile": profile,
                    "elf": requirements, "dependencies": dependencies, "files": package_app.file_manifest(stage)}
        package_app.put(stage, "runtime.json", encoded(manifest))
        # Apply the consumer's same size/inventory rules before publishing a kit.
        load_kit(stage)
        receipt = publish(stage, output, "ruster-runtime")
        receipt.update(runtime_kit_sha256=sha256(encoded(manifest)),
                       runtime_binary_sha256=sha256(binary_data), file_count=len(KIT_FILES) + 1)
        return receipt


def load_kit(root: Path, cancel_file: Path | None = None) -> tuple[dict, dict[str, bytes], str]:
    cancelled(cancel_file)
    if inventory(root) != KIT_FILES | {"runtime.json"}:
        raise ValueError("runtime kit inventory differs from the fixed payload allowlist")
    raw = read_bounded(root, "runtime.json", MAX_METADATA_BYTES)
    manifest = decode(raw, "runtime.json")
    exact_fields(manifest, {"schema", "kind", "target", "executable", "helper", "profile", "elf", "dependencies", "files"}, "runtime kit")
    if (type(manifest["schema"]) is not int or manifest["schema"] != 1
            or manifest["kind"] != "ruster-runtime-kit" or manifest["target"] != package_app.TARGET
            or manifest["executable"] != "runtime" or manifest["helper"] != "scripts/package_creation.py"):
        raise ValueError("unsupported runtime kit")
    check_profile(manifest["profile"])
    if not isinstance(manifest["elf"], dict) or not isinstance(manifest["dependencies"], list):
        raise ValueError("invalid runtime provenance")
    records = manifest["files"]
    if not isinstance(records, list) or len(records) != len(KIT_FILES):
        raise ValueError("invalid runtime kit file inventory")
    payload = {}
    total = len(raw)
    for record in records:
        cancelled(cancel_file)
        exact_fields(record, {"path", "bytes", "sha256"}, "runtime file record")
        name = record["path"]
        if not isinstance(name, str) or name not in KIT_FILES or name in payload:
            raise ValueError("unknown or duplicate runtime kit inventory path")
        limit = kit_file_limit(name)
        if type(record["bytes"]) is not int or not 0 <= record["bytes"] <= limit:
            raise ValueError("invalid runtime kit payload byte count")
        if not isinstance(record["sha256"], str) or not HASH.fullmatch(record["sha256"]):
            raise ValueError("invalid runtime kit payload hash")
        total += record["bytes"]
        if total > MAX_KIT_BYTES:
            raise ValueError("runtime kit exceeds total byte limit")
        data = read_bounded(root, name, limit, executable=name == "runtime")
        if len(data) != record["bytes"] or sha256(data) != record["sha256"]:
            raise ValueError(f"runtime kit checksum mismatch: {name}")
        payload[name] = data
    cancelled(cancel_file)
    return manifest, payload, sha256(raw)


def source_content(root: Path, cancel_file: Path | None) -> tuple[dict, bytes, dict, dict[str, bytes]]:
    cancelled(cancel_file)
    content = decode(read_bounded(root, "content.json", MAX_METADATA_BYTES), "content.json")
    fields = {"schema", "title", "tick_millis", "scene_sha256", "meshes"}
    exact_fields(content, fields | ({"initial_camera"} if "initial_camera" in content else set()), "content")
    if "initial_camera" in content:
        content["initial_camera"] = initial_camera(content["initial_camera"])
    title = content["title"]
    if (type(content["schema"]) is not int or content["schema"] != 1
            or not isinstance(title, str) or not title or title != title.strip()
            or len(title.encode()) > 128 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in title)
            or type(content["tick_millis"]) is not int or not 1 <= content["tick_millis"] <= 1000):
        raise ValueError("invalid content schema, title or fixed tick")
    scene_data = read_bounded(root, "scene.json", MIB)
    if not isinstance(content["scene_sha256"], str) or sha256(scene_data) != content["scene_sha256"]:
        raise ValueError("scene checksum mismatch")
    document = decode(scene_data, "Scene5")
    if document.get("format") != "ruster-scene" or type(document.get("format_version")) is not int or document["format_version"] != 5 or not isinstance(document.get("scene"), dict):
        raise ValueError("content requires a canonical Scene5 document")
    meshes = content["meshes"]
    if not isinstance(meshes, dict) or len(meshes) > MAX_MESHES:
        raise ValueError("content exceeds 64 mesh mappings")
    mesh_data = {}
    for identity, record in meshes.items():
        cancelled(cancel_file)
        exact_fields(record, {"path", "sha256"}, "mesh record")
        # The runtime remains the authority for scene IDs and OBJ semantics.
        if not isinstance(identity, str) or not identity or len(identity.encode()) > 128:
            raise ValueError("invalid mesh identity")
        name = record["path"]
        if not isinstance(name, str) or not MESH_PATH.fullmatch(name) or name in mesh_data:
            raise ValueError("meshes require unique generated meshes/0000.obj paths")
        data = read_bounded(root, name, MIB)
        if not isinstance(record["sha256"], str) or sha256(data) != record["sha256"]:
            raise ValueError(f"mesh checksum mismatch: {identity}")
        mesh_data[name] = data
    cancelled(cancel_file)
    return content, scene_data, document, mesh_data


def typed_scene(document: dict) -> dict:
    """Scene fields are f32; serde_json::Value exposes their exact f64 promotions."""
    scene = json.loads(json.dumps(document["scene"]))
    try:
        for obj in scene["objects"]:
            obj["color"] = [struct.unpack("f", struct.pack("f", value))[0] for value in obj["color"]]
            for field in ("translation", "rotation", "scale"):
                obj["transform"][field] = [struct.unpack("f", struct.pack("f", value))[0]
                                            for value in obj["transform"][field]]
    except (KeyError, TypeError, OverflowError, struct.error) as exc:
        raise ValueError("invalid Scene5 numeric fields") from exc
    return scene


def package(runtime_kit: Path, content_root: Path, output: Path,
            cancel_file: Path | None = None) -> dict:
    kit, payload, kit_hash = load_kit(runtime_kit, cancel_file)
    content, scene_data, document, meshes = source_content(content_root, cancel_file)
    camera = content.get("initial_camera")
    if camera is not None and kit["profile"] != CAMERA_PROFILE:
        raise ValueError("runtime kit does not support initial_camera; prepare a current runtime kit")
    descriptor = {"format": "ruster-application", "version": 1, "title": content["title"],
                  "scene": "resources/scene.json", "tick_millis": content["tick_millis"],
                  "meshes": {identity: "resources/" + record["path"]
                             for identity, record in content["meshes"].items()}}
    if camera is not None:
        descriptor["initial_camera"] = camera
    with tempfile.TemporaryDirectory(prefix="ruster-creation-") as temporary:
        stage = Path(temporary)
        package_app.put(stage, "application", payload["runtime"], executable=True)
        for name in LICENSE_FILES:
            cancelled(cancel_file)
            package_app.put(stage, name, payload[name])
        package_app.put(stage, "resources/scene.json", scene_data)
        for name, data in meshes.items():
            cancelled(cancel_file)
            package_app.put(stage, "resources/" + name, data)
        package_app.put(stage, "app.json", encoded(descriptor))
        requirements = kit["elf"]
        readme = (content["title"] + "\n\nRun ./application from any working directory.\n"
                  "Linux x86_64; native graphics require a Vulkan-capable driver and desktop session.\n"
                  "ELF prerequisites: " + json.dumps(requirements, sort_keys=True) + "\n"
                  "The saved authored Scene5 bytes and mapped OBJ resources accompany this runtime.\n")
        package_app.put(stage, "README.txt", readme.encode())
        result = run_runtime(stage / "application", ["--headless", "--ticks", "0"], cancel_file)
        if (result.get("mode") != "creation" or result.get("title") != content["title"]
                or result.get("tick") != "0" or result.get("fixed_dt") != content["tick_millis"] / 1000
                or result.get("authored_scene") != typed_scene(document)):
            raise ValueError("runtime preflight did not retain creation identity, tick or authored scene")
        if camera is not None and result.get("initial_camera") != camera:
            raise ValueError("runtime preflight did not retain explicit initial camera")
        cancelled(cancel_file)
        manifest = {"schema": 1, "kind": "ruster-creation-package", "application": "application",
                    "target": package_app.TARGET, "title": content["title"],
                    "runtime_kit_sha256": kit_hash, "runtime_binary_sha256": sha256(payload["runtime"]),
                    "scene_sha256": sha256(scene_data), "profile": kit["profile"],
                    "elf": requirements, "dependencies": kit["dependencies"],
                    "files": package_app.file_manifest(stage)}
        manifest_data = encoded(manifest)
        package_app.put(stage, "manifest.json", manifest_data)
        receipt = publish(stage, output, "application", cancel_file)
        receipt.update(manifest_sha256=sha256(manifest_data), scene_sha256=sha256(scene_data),
                       runtime_kit_sha256=kit_hash, runtime_binary_sha256=sha256(payload["runtime"]),
                       file_count=len(manifest["files"]) + 1, title=content["title"])
        if camera is not None:
            receipt["initial_camera"] = camera
        return receipt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare-runtime", help="prepare a kit from an already built remapped executable")
    prepare.add_argument("--binary", type=Path, required=True)
    prepare.add_argument("--output", type=Path, required=True)
    creation = commands.add_parser("package", help="package creation content without Cargo or readelf")
    creation.add_argument("--runtime-kit", type=Path, required=True)
    creation.add_argument("--content", type=Path, required=True)
    creation.add_argument("--output", type=Path, required=True)
    creation.add_argument("--cancel-file", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "prepare-runtime":
            receipt = prepare_runtime(ROOT, args.binary, args.output)
        else:
            receipt = package(args.runtime_kit, args.content, args.output, args.cancel_file)
    except (OSError, ValueError, subprocess.SubprocessError, RecursionError) as exc:
        print(f"package creation: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(receipt, ensure_ascii=False, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
