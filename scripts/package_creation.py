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
MAX_OBJECTS = 256
MAX_MATERIALS = 64
MAX_TEXTURE_BYTES = 8 * MIB
PROFILE = {"format": "ruster-runtime", "version": 1,
           "descriptor_format": "ruster-application", "descriptor_version": 1,
           "os": "linux", "arch": "x86_64"}
CAMERA_PROFILE = {**PROFILE, "capabilities": ["initial_camera"]}
MATERIAL_PROFILE = {**PROFILE, "descriptor_version": 2,
                    "capabilities": ["initial_camera", "opaque_materials"]}
LIGHTING_PROFILE = {**PROFILE, "descriptor_version": 2,
                    "capabilities": ["initial_camera", "opaque_materials", "authored_lighting"]}
SUN_SHADOW_PROFILE = {**PROFILE, "descriptor_version": 2,
                      "capabilities": ["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows"]}
LICENSE_FILES = ("LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt",
                 "SUPPLEMENTAL_NOTICES.txt", "LICENSE-SOURCES.json")
HELPER_FILES = ("scripts/package_creation.py", "scripts/package_app.py",
                "scripts/dependency_notices.py")
KIT_FILES = frozenset(("runtime", "README.txt") + LICENSE_FILES + HELPER_FILES)
HASH = re.compile(r"[0-9a-f]{64}\Z")
MESH_PATH = re.compile(r"meshes/[0-9]{4}\.obj\Z")
TEXTURE_PATH = re.compile(r"textures/[0-9]{4}\.png\Z")
IDENTITY = re.compile(r"[A-Za-z0-9_.-]{1,128}\Z")


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
    if (profile not in (PROFILE, CAMERA_PROFILE, MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE)
            or any(type(profile.get(key)) is not type(value) for key, value in PROFILE.items())):
        raise ValueError("unsupported runtime creation profile; expected a fixed Linux x86_64 protocol version 1 profile")


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


def lighting_states(document: dict):
    """Owning current/history positions only; never arbitrary behavior JSON."""
    yield document.get("scene", {}).get("lighting", {})
    for stack in ("undo", "redo"):
        changes = document.get(stack, [])
        if not isinstance(changes, list):
            continue  # Full history validation belongs to the actual runtime.
        for change in changes:
            if not isinstance(change, dict):
                continue
            deltas = [change] if document["format_version"] <= 4 else change.get("scene", [])
            if not isinstance(deltas, list):
                continue
            for delta in deltas:
                if isinstance(delta, dict) and delta.get("kind") == "lighting":
                    yield delta.get("before", {})
                    yield delta.get("after", {})


def validate_sun_shadows(document: dict) -> None:
    """Shadow wire admission complements runtime-owned complete scene checks."""
    for lighting in lighting_states(document):
        lights = lighting.get("lights", {}) if isinstance(lighting, dict) else {}
        if not isinstance(lights, dict):
            continue
        for light in lights.values():
            if not isinstance(light, dict):
                continue
            kind = light.get("kind", {})
            has_kind_shadow = isinstance(kind, dict) and "shadow" in kind
            if document["format_version"] < 8 and ("shadow" in light or has_kind_shadow):
                raise ValueError("Sun shadow fields require Scene8 sun_shadows")
            if "shadow" in light:
                raise ValueError("Sun shadow settings belong to directional kind.shadow")
            if not has_kind_shadow:
                continue  # Only whole-field omission selects disabled defaults.
            if kind.get("type") != "directional":
                raise ValueError("Sun shadow settings require a directional light")
            shadow = kind["shadow"]
            exact_fields(shadow, {"enabled", "quality", "depth_bias_texels", "normal_bias_texels"}, "Sun shadow")
            if type(shadow["enabled"]) is not bool or shadow["quality"] not in ("standard", "high"):
                raise ValueError("invalid Sun shadow enabled flag or quality")
            for field in ("depth_bias_texels", "normal_bias_texels"):
                value = shadow[field]
                if type(value) not in (int, float) or not 0 <= value <= 4 or not math.isfinite(value):
                    raise ValueError("Sun shadow biases must be finite numbers in 0..4 texels")


def scene_dependencies(document: dict) -> tuple[set[str], set[str]]:
    """Derive current runtime dependencies independently of the content request.

    Full scene/history/material/UV validation remains the actual runtime's job.
    No traversal of arbitrary property/history JSON or external paths is needed.
    """
    version = document.get("format_version")
    scene = document.get("scene")
    if (document.get("format") != "ruster-scene" or type(version) is not int
            or not 1 <= version <= 8 or not isinstance(scene, dict)):
        raise ValueError("content requires a supported Scene1–8 document")
    validate_sun_shadows(document)
    objects = scene.get("objects")
    materials = scene.get("materials", {})
    if not isinstance(objects, list) or len(objects) > MAX_OBJECTS:
        raise ValueError("invalid scene object collection")
    if not isinstance(materials, dict) or len(materials) > MAX_MATERIALS:
        raise ValueError("invalid scene material collection")
    if version < 6 and ("materials" in scene or any(
            isinstance(obj, dict) and "material" in obj for obj in objects)):
        raise ValueError("material fields require Scene6")
    if version < 7 and "lighting" in scene:
        raise ValueError("lighting fields require Scene7")

    def identity(value: object, label: str) -> str:
        if not isinstance(value, str) or not IDENTITY.fullmatch(value):
            raise ValueError(f"invalid {label} identity")
        return value

    meshes, textures = set(), set()
    for obj in objects:
        if not isinstance(obj, dict):
            raise ValueError("invalid scene object")
        if obj.get("mesh") is not None:
            meshes.add(identity(obj["mesh"], "mesh"))
        if obj.get("material") is not None:
            material_id = identity(obj["material"], "material")
            material = materials.get(material_id)
            if not isinstance(material, dict):
                raise ValueError(f"missing assigned material: {material_id}")
            if material.get("base_color_texture") is not None:
                textures.add(identity(material["base_color_texture"], "texture"))
    return meshes, textures


def source_content(root: Path, cancel_file: Path | None) -> tuple[dict, bytes, dict, dict[str, tuple[int, str, str]]]:
    cancelled(cancel_file)
    content = decode(read_bounded(root, "content.json", MAX_METADATA_BYTES), "content.json")
    schema = content.get("schema")
    if type(schema) is not int or schema not in (1, 2, 3, 4):
        raise ValueError("invalid content schema")
    fields = {"schema", "title", "tick_millis", "scene_sha256", "meshes"}
    if schema >= 2:
        fields.add("textures")
    exact_fields(content, fields | ({"initial_camera"} if "initial_camera" in content else set()), "content")
    if "initial_camera" in content:
        content["initial_camera"] = initial_camera(content["initial_camera"])
    title = content["title"]
    if (not isinstance(title, str) or not title or title != title.strip()
            or len(title.encode()) > 128 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in title)
            or type(content["tick_millis"]) is not int or not 1 <= content["tick_millis"] <= 1000):
        raise ValueError("invalid content title or fixed tick")
    scene_data = read_bounded(root, "scene.json", MIB)
    if (not isinstance(content["scene_sha256"], str) or not HASH.fullmatch(content["scene_sha256"])
            or sha256(scene_data) != content["scene_sha256"]):
        raise ValueError("scene checksum mismatch")
    document = decode(scene_data, "authored scene")
    mesh_ids, texture_ids = scene_dependencies(document)
    if (schema == 1 and document["format_version"] > 5
            or schema == 2 and document["format_version"] != 6
            or schema == 3 and document["format_version"] != 7
            or schema == 4 and document["format_version"] != 8):
        raise ValueError("content schema1 requires Scene1–5; schema2 requires Scene6; schema3 requires Scene7; schema4 requires Scene8")
    meshes, textures = content["meshes"], content.get("textures", {})
    if (not isinstance(meshes, dict) or not isinstance(textures, dict)
            or len(meshes) + len(textures) > MAX_MESHES):
        raise ValueError("content exceeds 64 combined asset mappings")
    if set(meshes) != mesh_ids or set(textures) != texture_ids or set(meshes) & set(textures):
        raise ValueError("content mappings differ from the current scene mesh/texture dependency closure")
    source_specs = {}
    for kind, records, pattern, maximum in (("mesh", meshes, MESH_PATH, MIB),
                                            ("texture", textures, TEXTURE_PATH, MAX_TEXTURE_BYTES)):
        for identity, record in records.items():
            cancelled(cancel_file)
            exact_fields(record, {"path", "sha256"}, f"{kind} record")
            if not isinstance(identity, str) or not IDENTITY.fullmatch(identity):
                raise ValueError(f"invalid {kind} identity")
            name = record["path"]
            if not isinstance(name, str) or not pattern.fullmatch(name) or name in source_specs:
                raise ValueError(f"{kind} sources require unique generated {kind} paths")
            if not isinstance(record["sha256"], str) or not HASH.fullmatch(record["sha256"]):
                raise ValueError(f"{kind} checksum mismatch: {identity}")
            # Keep bounded metadata, not the entire encoded source closure.
            # Each source is checked while copying into disposable staging.
            source_specs[name] = (maximum, record["sha256"], f"{kind} checksum mismatch: {identity}")
    cancelled(cancel_file)
    return content, scene_data, document, source_specs


def typed_scene(document: dict) -> dict:
    """Scene fields are f32; serde_json::Value exposes their exact f64 promotions."""
    validate_sun_shadows(document)
    scene = json.loads(json.dumps(document["scene"]))
    try:
        lighting = scene.get("lighting", {"mode": "legacy"})
        if lighting["mode"] == "legacy":
            scene.pop("lighting", None)
        elif lighting["mode"] == "authored":
            ambient = lighting["ambient"]
            ambient["color"] = [struct.unpack("f", struct.pack("f", value))[0]
                                for value in ambient["color"]]
            ambient["intensity"] = struct.unpack("f", struct.pack("f", ambient["intensity"]))[0]
            for light in lighting["lights"].values():
                if light.get("name") is None:
                    light.pop("name", None)
                light["color"] = [struct.unpack("f", struct.pack("f", value))[0]
                                  for value in light["color"]]
                light["intensity"] = struct.unpack("f", struct.pack("f", light["intensity"]))[0]
                for field in ("translation", "rotation", "scale"):
                    light["transform"][field] = [struct.unpack("f", struct.pack("f", value))[0]
                                                for value in light["transform"][field]]
                if light["kind"]["type"] == "point":
                    light["kind"]["range"] = struct.unpack("f", struct.pack("f", light["kind"]["range"]))[0]
                elif "shadow" in light["kind"]:
                    shadow = light["kind"]["shadow"]
                    for field in ("depth_bias_texels", "normal_bias_texels"):
                        shadow[field] = struct.unpack("f", struct.pack("f", shadow[field]))[0]
                    if shadow == {"enabled": False, "quality": "standard",
                                  "depth_bias_texels": struct.unpack("f", struct.pack("f", 0.15))[0],
                                  "normal_bias_texels": 0.0}:
                        light["kind"].pop("shadow")
        else:
            raise ValueError("unsupported scene lighting mode")
        if not scene.get("materials"):
            scene.pop("materials", None)
        for material in scene.get("materials", {}).values():
            material["tint"] = [struct.unpack("f", struct.pack("f", value))[0]
                                for value in material["tint"]]
            for field in ("name", "base_color_texture"):
                if material.get(field) is None:
                    material.pop(field, None)
        for obj in scene["objects"]:
            for field in ("name", "mesh", "material"):
                if obj.get(field) is None:
                    obj.pop(field, None)
            obj["color"] = [struct.unpack("f", struct.pack("f", value))[0] for value in obj["color"]]
            for field in ("translation", "rotation", "scale"):
                obj["transform"][field] = [struct.unpack("f", struct.pack("f", value))[0]
                                            for value in obj["transform"][field]]
    except (KeyError, TypeError, OverflowError, struct.error) as exc:
        raise ValueError("invalid scene numeric fields") from exc
    return scene


def package(runtime_kit: Path, content_root: Path, output: Path,
            cancel_file: Path | None = None) -> dict:
    kit, payload, kit_hash = load_kit(runtime_kit, cancel_file)
    content, scene_data, document, source_specs = source_content(content_root, cancel_file)
    camera = content.get("initial_camera")
    if content["schema"] == 4 and kit["profile"] != SUN_SHADOW_PROFILE:
        raise ValueError("runtime kit does not support Scene8 sun_shadows; prepare a current runtime kit")
    if content["schema"] == 3 and kit["profile"] not in (LIGHTING_PROFILE, SUN_SHADOW_PROFILE):
        raise ValueError("runtime kit does not support Scene7 authored_lighting; prepare a current runtime kit")
    if content["schema"] == 2 and kit["profile"] not in (MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE):
        raise ValueError("runtime kit does not support Scene6 opaque_materials; prepare a current runtime kit")
    if camera is not None and kit["profile"] not in (CAMERA_PROFILE, MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE):
        raise ValueError("runtime kit does not support initial_camera; prepare a current runtime kit")
    descriptor = {"format": "ruster-application", "version": min(content["schema"], 2), "title": content["title"],
                  "scene": "resources/scene.json", "tick_millis": content["tick_millis"],
                  "meshes": {identity: "resources/" + record["path"]
                             for identity, record in content["meshes"].items()}}
    if content["schema"] >= 2:
        descriptor["textures"] = {identity: "resources/" + record["path"]
                                  for identity, record in content["textures"].items()}
    if camera is not None:
        descriptor["initial_camera"] = camera
    with tempfile.TemporaryDirectory(prefix="ruster-creation-") as temporary:
        stage = Path(temporary)
        package_app.put(stage, "application", payload["runtime"], executable=True)
        for name in LICENSE_FILES:
            cancelled(cancel_file)
            package_app.put(stage, name, payload[name])
        package_app.put(stage, "resources/scene.json", scene_data)
        for name, (maximum, expected_hash, diagnostic) in source_specs.items():
            cancelled(cancel_file)
            data = read_bounded(content_root, name, maximum)
            if sha256(data) != expected_hash:
                raise ValueError(diagnostic)
            package_app.put(stage, "resources/" + name, data)
            del data
        package_app.put(stage, "app.json", encoded(descriptor))
        requirements = kit["elf"]
        readme = (content["title"] + "\n\nRun ./application from any working directory.\n"
                  "Linux x86_64; native graphics require a Vulkan-capable driver and desktop session.\n"
                  "ELF prerequisites: " + json.dumps(requirements, sort_keys=True) + "\n"
                  "The saved authored scene bytes and mapped original content resources accompany this runtime.\n")
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
