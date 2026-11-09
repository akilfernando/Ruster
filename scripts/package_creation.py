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
SURFACE_PROFILE = {**PROFILE, "descriptor_version": 3,
                   "capabilities": ["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "surface_materials"]}
ROUGHNESS_PROFILE = {**PROFILE, "descriptor_version": 4,
                     "capabilities": ["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "surface_materials", "roughness_maps"]}
INPUT_PROFILE = {**ROUGHNESS_PROFILE, "descriptor_version": 5,
                 "capabilities": [*ROUGHNESS_PROFILE["capabilities"], "input_actions"]}
SPATIAL_PROFILE = {**INPUT_PROFILE,
                   "capabilities": [*INPUT_PROFILE["capabilities"], "spatial_queries"]}
STATE_PROFILE = {**SPATIAL_PROFILE,
                 "capabilities": [*SPATIAL_PROFILE["capabilities"], "runtime_state"]}
LIFECYCLE_PROFILE = {**STATE_PROFILE,
                     "capabilities": [*STATE_PROFILE["capabilities"], "lifecycle_commands"]}
HUD_PROFILE = {**LIFECYCLE_PROFILE,
               "capabilities": [*LIFECYCLE_PROFILE["capabilities"], "runtime_hud"]}
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
    if (profile not in (PROFILE, CAMERA_PROFILE, MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE, SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE)
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


def validate_surface_fields(document: dict) -> None:
    version = document["format_version"]
    def display(value: object) -> None:
        if not isinstance(value, dict) or value.get("type") not in ("legacy_sdr", "surface_hdr"):
            raise ValueError("invalid scene display profile")
        exact_fields(value, {"type", "exposure_stops"} if value["type"] == "surface_hdr" else {"type"}, "display")
        if value["type"] == "surface_hdr" and (type(value["exposure_stops"]) not in (int, float) or not math.isfinite(value["exposure_stops"]) or not -8 <= value["exposure_stops"] <= 8):
            raise ValueError("exposure_stops must be finite in -8..8")
    def response(value: object) -> None:
        if not isinstance(value, dict) or value.get("type") not in ("legacy_diffuse", "dielectric"):
            raise ValueError("invalid material response")
        exact_fields(value, {"type", "roughness"} if value["type"] == "dielectric" else {"type"}, "material response")
        if value["type"] == "dielectric" and (type(value["roughness"]) not in (int, float) or not math.isfinite(value["roughness"]) or not .08 <= value["roughness"] <= 1):
            raise ValueError("roughness must be finite in .08..1")
    scene = document.get("scene", {})
    if not isinstance(scene, dict):
        raise ValueError("invalid scene")
    if "display" in scene:
        if version < 9: raise ValueError("display requires Scene9")
        display(scene["display"])
    current_materials = scene.get("materials", {})
    materials = list(current_materials.values()) if isinstance(current_materials, dict) else []
    for stack in ("undo", "redo"):
        for change in document.get(stack, []):
            if not isinstance(change, dict): continue
            for delta in change.get("scene", []) if version >= 5 else [change]:
                if isinstance(delta, dict) and delta.get("kind") == "display":
                    if version < 9: raise ValueError("display history requires Scene9")
                    display(delta.get("before")); display(delta.get("after"))
            material_deltas = change.get("materials", [])
            for delta in material_deltas if isinstance(material_deltas, list) else []:
                if isinstance(delta, dict): materials.extend([delta.get("before"), delta.get("after")])
    for material in materials:
        if isinstance(material, dict) and "roughness_texture" in material:
            if version < 10: raise ValueError("roughness_texture requires Scene10 roughness_maps")
            texture = material["roughness_texture"]
            if texture is not None and (not isinstance(texture, str) or not IDENTITY.fullmatch(texture)):
                raise ValueError("invalid roughness texture identity")
        if isinstance(material, dict) and "response" in material:
            if version < 9: raise ValueError("material response requires Scene9")
            response(material["response"])


def validate_input_fields(document: dict) -> None:
    """Own only action positions/shapes/caps; the real Session validates semantics.

    Never reinterpret behavior text/properties or replay history in Python.
    Key grammar, reserved/duplicate keys, IDs and reachable-history consistency
    remain the typed runtime preflight's responsibility before publication.
    """
    version = document["format_version"]

    def definition(value: object) -> None:
        if not isinstance(value, dict) or set(value) not in ({"kind"}, {"kind", "label"}):
            raise ValueError("invalid action definition fields")
        if value.get("label") is not None and not isinstance(value["label"], str):
            raise ValueError("invalid action label")
        kind = value["kind"]
        if not isinstance(kind, dict) or kind.get("type") not in ("button", "axis1d"):
            raise ValueError("invalid action kind")
        fields = ("keys",) if kind["type"] == "button" else ("negative", "positive")
        exact_fields(kind, {"type", *fields}, "action kind")
        if any(not isinstance(kind[field], list) or any(not isinstance(key, str) for key in kind[field])
               for field in fields) or sum(len(kind[field]) for field in fields) > 4:
            raise ValueError("action requires at most4 total logical-key strings")

    authoring = document.get("authoring", {})
    if isinstance(authoring, dict) and "actions" in authoring:
        if version < 11:
            raise ValueError("input actions require Scene11")
        actions = authoring["actions"]
        if not isinstance(actions, dict) or len(actions) > 32:
            raise ValueError("action map requires at most32 definitions")
        for action in actions.values():
            definition(action)
    for stack in ("undo", "redo"):
        changes = document.get(stack, [])
        for change in changes if isinstance(changes, list) else []:
            if not isinstance(change, dict) or "actions" not in change:
                continue
            if version < 11:
                raise ValueError("input action history requires Scene11")
            deltas = change["actions"]
            if not isinstance(deltas, list) or len(deltas) > 32:
                raise ValueError("action history requires at most32 deltas")
            for delta in deltas:
                if (not isinstance(delta, dict) or "id" not in delta
                        or not isinstance(delta["id"], str) or set(delta) - {"id", "before", "after"}):
                    raise ValueError("invalid action history fields")
                for endpoint in ("before", "after"):
                    if delta.get(endpoint) is not None:
                        definition(delta[endpoint])


def validate_collider_fields(document: dict) -> None:
    """Admit actual collider wire owners; typed runtime owns transforms/history."""
    version = document["format_version"]

    def vector(value: object, minimum: float, maximum: float, label: str) -> None:
        if (not isinstance(value, list) or len(value) != 3 or any(
                type(component) not in (int, float) or not minimum <= component <= maximum
                or not math.isfinite(component) for component in value)):
            raise ValueError(f"invalid collider {label}")

    def component(value: object) -> None:
        if value is None:
            return  # Optional whole component; old-version presence rejects below.
        exact_fields(value, {"enabled", "role", "offset", "shape"}, "collider")
        if type(value["enabled"]) is not bool or value["role"] not in ("blocker", "sensor"):
            raise ValueError("invalid collider enabled flag or role")
        vector(value["offset"], -10000, 10000, "offset")
        shape = value["shape"]
        if not isinstance(shape, dict) or shape.get("type") not in ("sphere", "box"):
            raise ValueError("invalid collider shape")
        if shape["type"] == "sphere":
            exact_fields(shape, {"type", "radius"}, "collider sphere")
            radius = shape["radius"]
            if type(radius) not in (int, float) or not .001 <= radius <= 1000 or not math.isfinite(radius):
                raise ValueError("collider radius must be finite in .001..1000")
        else:
            exact_fields(shape, {"type", "half_extents"}, "collider box")
            vector(shape["half_extents"], .001, 1000, "half extents")

    def object_fields(value: object) -> None:
        if isinstance(value, dict) and "collider" in value:
            if version < 12:
                raise ValueError("collider fields require Scene12 spatial_queries")
            component(value["collider"])

    scene = document.get("scene", {})
    objects = scene.get("objects", []) if isinstance(scene, dict) else []
    for obj in objects if isinstance(objects, list) else []:
        object_fields(obj)
    for stack in ("undo", "redo"):
        changes = document.get(stack, [])
        for change in changes if isinstance(changes, list) else []:
            if not isinstance(change, dict):
                continue
            deltas = [change] if version <= 4 else change.get("scene", [])
            for delta in deltas if isinstance(deltas, list) else []:
                if not isinstance(delta, dict):
                    continue
                kind = delta.get("kind")
                if kind == "collider":
                    if version < 12:
                        raise ValueError("collider history requires Scene12 spatial_queries")
                    for endpoint in ("before", "after"):
                        component(delta.get(endpoint))
                elif kind in ("create", "delete"):
                    object_fields(delta.get("object"))
                elif kind == "objects":
                    for section in ("created", "deleted"):
                        entries = delta.get(section, [])
                        for entry in entries if isinstance(entries, list) else []:
                            if isinstance(entry, dict):
                                object_fields(entry.get("object"))


def authored_actions(document: dict) -> dict:
    """Expected typed current map; only optional null labels omit on serialization."""
    validate_input_fields(document)
    actions = json.loads(json.dumps(document.get("authoring", {}).get("actions", {})))
    for action in actions.values():
        if action.get("label") is None:
            action.pop("label", None)
    return actions


def validate_state_fields(document: dict) -> None:
    """Check binding-owned state, never property/source text that resembles it.

    Python and serde_json need not print f64 identically. Numeric zero placeholders
    give only a canonical size lower bound here; mandatory typed runtime preflight
    owns exact encoded caps, reachable history and source execution validation.
    """
    version = document["format_version"]

    def identity(value: object) -> None:
        if not isinstance(value, str) or not IDENTITY.fullmatch(value):
            raise ValueError("invalid state/binding identity")

    def finite(value: object) -> None:
        try:
            valid = type(value) in (int, float) and math.isfinite(value)
        except OverflowError:
            valid = False
        if not valid:
            raise ValueError("state numbers must be finite binary64 values")

    def lower_bytes(value: object) -> int:
        return len(json.dumps(value, ensure_ascii=False, separators=(",", ":"),
                              allow_nan=False).encode())

    def typed_fields(fields: object) -> dict:
        if not isinstance(fields, dict) or len(fields) > 64:
            raise ValueError("state requires at most64 fields per binding")
        lower = {}
        for name, field in fields.items():
            identity(name)
            exact_fields(field, {"type", "value"}, "state value")
            kind, data = field["type"], field["value"]
            if kind == "number":
                finite(data)
                data = 0
            elif kind == "vec3":
                if not isinstance(data, list) or len(data) != 3:
                    raise ValueError("state vector requires three finite numbers")
                for component in data:
                    finite(component)
                data = [0, 0, 0]
            elif kind == "bool":
                if type(data) is not bool:
                    raise ValueError("state bool requires a boolean")
            elif kind == "text":
                if not isinstance(data, str) or len(data.encode()) > 1024:
                    raise ValueError("state text exceeds1024 UTF-8 bytes")
            else:
                raise ValueError("unsupported state value type")
            lower[name] = {"type": kind, "value": data}
        if lower_bytes(lower) > 16 * 1024:
            raise ValueError("state exceeds16KiB per binding")
        return lower

    def binding(value: object) -> dict:
        if not isinstance(value, dict):
            raise ValueError("invalid behavior binding")
        if version < 14 and "commands" in value:
            raise ValueError("application commands require Scene14 lifecycle_commands")
        if version < 13:
            if "target" in value or "state_defaults" in value:
                raise ValueError("application targets and state defaults require Scene13 runtime_state")
            return {}  # Existing old binding validation belongs to Session.
        required = {"source_id", "enabled", "properties"}
        owner = "object_id" if "object_id" in value else "target"
        exact_fields(value, required | {owner} | ({"state_defaults"} if "state_defaults" in value else set())
                     | ({"commands"} if "commands" in value else set()), "behavior binding")
        identity(value["source_id"])
        if type(value["enabled"]) is not bool or not isinstance(value["properties"], dict):
            raise ValueError("invalid binding enabled flag or properties")
        if owner == "object_id":
            identity(value[owner])
        else:
            exact_fields(value[owner], {"type"}, "application target")
            if value[owner]["type"] != "application":
                raise ValueError("invalid application target")
        lower = typed_fields(value.get("state_defaults", {}))
        commands = value.get("commands", {})
        if not isinstance(commands, dict) or len(commands) > 16:
            raise ValueError("application requires at most16 commands")
        if commands and owner != "target":
            raise ValueError("nonempty commands require an application binding")
        for command_id, definition in commands.items():
            identity(command_id)
            exact_fields(definition, {"arguments"}, "command definition")
            arguments = definition["arguments"]
            if not isinstance(arguments, dict) or len(arguments) > 16:
                raise ValueError("command requires at most16 arguments")
            typed_fields(arguments)
        return lower

    authoring = document.get("authoring", {})
    bindings = authoring.get("bindings", {}) if isinstance(authoring, dict) else {}
    if not isinstance(bindings, dict) or len(bindings) > 256:
        raise ValueError("authoring requires at most256 bindings")
    states, owners, application = {}, set(), False
    for name, value in bindings.items():
        identity(name)
        states[name] = binding(value)
        if version >= 13:
            if "target" in value:
                if application:
                    raise ValueError("only one application binding is supported")
                application = True
            else:
                owner = value["object_id"]
                if owner in owners:
                    raise ValueError("only one behavior binding per object is supported")
                owners.add(owner)
    if sum(lower_bytes(fields) for fields in states.values() if fields) > MIB:
        raise ValueError("state exceeds1MiB per runtime")
    for stack in ("undo", "redo"):
        changes = document.get(stack, [])
        for change in changes if isinstance(changes, list) else []:
            if not isinstance(change, dict) or "bindings" not in change:
                continue
            deltas = change["bindings"]
            if not isinstance(deltas, list):
                raise ValueError("binding history requires a delta array")
            for delta in deltas:
                if not isinstance(delta, dict) or "id" not in delta or set(delta) - {"id", "before", "after"}:
                    raise ValueError("invalid binding history fields")
                identity(delta["id"])
                for endpoint in ("before", "after"):
                    if delta.get(endpoint) is not None:
                        binding(delta[endpoint])


def typed_hud(value: object) -> dict:
    """Normalize serde defaults and validate authored data, without resolving state."""
    defaults = {"enabled": False, "anchor": "top_left", "width": 320.0,
                "margin": 12.0, "spacing": 6.0, "font_size": 14.0,
                "text_color": [1.0] * 4, "background": [0.08, 0.08, 0.08, 0.9],
                "elements": []}
    if not isinstance(value, dict) or set(value) - defaults.keys():
        raise ValueError("invalid HUD fields")
    hud = {**defaults, **value}
    if type(hud["enabled"]) is not bool or hud["anchor"] not in (
            "top_left", "top_right", "bottom_left", "bottom_right"):
        raise ValueError("invalid HUD enabled flag or anchor")
    for name, minimum, maximum in (("width", 64, 1024), ("margin", 0, 256),
                                    ("spacing", 0, 64), ("font_size", 8, 48)):
        number = hud[name]
        if type(number) not in (int, float) or not minimum <= number <= maximum:
            raise ValueError("HUD layout values exceed their finite bounds")
    for name in ("text_color", "background"):
        color = hud[name]
        if (not isinstance(color, list) or len(color) != 4 or any(
                type(channel) not in (int, float) or not 0 <= channel <= 1 for channel in color)):
            raise ValueError("HUD color requires four finite channels in0..1")
    if not isinstance(hud["elements"], list) or len(hud["elements"]) > 32:
        raise ValueError("HUD requires at most32 labels")
    elements, identities, text_bytes = [], set(), 0
    for label in hud["elements"]:
        exact_fields(label, {"id", "source"}, "HUD label")
        identity = label["id"]
        if not isinstance(identity, str) or not IDENTITY.fullmatch(identity) or identity in identities:
            raise ValueError("HUD label IDs must be valid and unique")
        identities.add(identity)
        source = label["source"]
        if not isinstance(source, dict):
            raise ValueError("invalid HUD source")
        if source.get("kind") == "static":
            exact_fields(source, {"kind", "text"}, "static HUD source")
            texts = [source["text"]]
        elif source.get("kind") == "state":
            source = {"prefix": "", "suffix": "", "precision": 2, **source}
            exact_fields(source, {"kind", "binding_id", "field", "value_type", "prefix", "suffix", "precision"}, "state HUD source")
            if any(not isinstance(source[key], str) or not IDENTITY.fullmatch(source[key])
                   for key in ("binding_id", "field")):
                raise ValueError("invalid HUD state reference")
            if (source["value_type"] not in ("number", "bool", "text", "vec3")
                    or type(source["precision"]) is not int or not 0 <= source["precision"] <= 6):
                raise ValueError("invalid HUD value type or precision")
            texts = [source["prefix"], source["suffix"]]
        else:
            raise ValueError("invalid HUD source kind")
        if any(not isinstance(text, str) for text in texts):
            raise ValueError("HUD authored text must be strings")
        count = sum(len(text.encode()) for text in texts)
        if count > 1024:
            raise ValueError("HUD label authored text exceeds1024 UTF-8 bytes")
        text_bytes += count
        elements.append({"id": identity, "source": source})
    if text_bytes > 32 * 1024:
        raise ValueError("HUD authored text exceeds32KiB")
    hud["elements"] = elements
    return hud


def validate_hud_fields(document: dict) -> None:
    version = document["format_version"]
    authoring = document.get("authoring", {})
    if isinstance(authoring, dict) and "hud" in authoring:
        if version < 15:
            raise ValueError("HUD requires Scene15 runtime_hud")
        typed_hud(authoring["hud"])
    for stack in ("undo", "redo"):
        changes = document.get(stack, [])
        for change in changes if isinstance(changes, list) else []:
            if not isinstance(change, dict) or "hud" not in change:
                continue
            if version < 15:
                raise ValueError("HUD history requires Scene15 runtime_hud")
            delta = change["hud"]
            if delta is not None:
                exact_fields(delta, {"before", "after"}, "HUD history")
                typed_hud(delta["before"])
                typed_hud(delta["after"])


def scene_dependencies(document: dict) -> tuple[set[str], set[str]]:
    """Derive current runtime dependencies independently of the content request.

    Full scene/history/material/UV validation remains the actual runtime's job.
    No traversal of arbitrary property/history JSON or external paths is needed.
    """
    version = document.get("format_version")
    scene = document.get("scene")
    if (document.get("format") != "ruster-scene" or type(version) is not int
            or not 1 <= version <= 15 or not isinstance(scene, dict)):
        raise ValueError("content requires a supported Scene1–15 document")
    validate_hud_fields(document)
    validate_state_fields(document)
    validate_collider_fields(document)
    validate_input_fields(document)
    validate_sun_shadows(document)
    validate_surface_fields(document)
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
            if material.get("roughness_texture") is not None:
                textures.add(identity(material["roughness_texture"], "roughness texture"))
    return meshes, textures


def scene_texture_roles(document: dict) -> dict[str, str]:
    """The same assigned-current closure, including inactive authored slots."""
    scene = document["scene"]
    roles = {}
    for obj in scene["objects"]:
        material = scene.get("materials", {}).get(obj.get("material"), {})
        for field, role in (("base_color_texture", "base_color_srgb"),
                            ("roughness_texture", "roughness_linear")):
            identity = material.get(field)
            if identity is not None:
                if identity in roles and roles[identity] != role:
                    raise ValueError("one texture identity cannot serve conflicting material roles")
                roles[identity] = role
    return roles


def source_content(root: Path, cancel_file: Path | None) -> tuple[dict, bytes, dict, dict[str, tuple[int, str, str]]]:
    cancelled(cancel_file)
    content = decode(read_bounded(root, "content.json", MAX_METADATA_BYTES), "content.json")
    schema = content.get("schema")
    if type(schema) is not int or schema not in (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11):
        raise ValueError("invalid content schema")
    tick_field = "tick_nanos" if schema in (7, 8, 9, 10, 11) else "tick_millis"
    fields = {"schema", "title", tick_field, "scene_sha256", "meshes"}
    if schema >= 2:
        fields.add("textures")
    exact_fields(content, fields | ({"initial_camera"} if "initial_camera" in content else set()), "content")
    if "initial_camera" in content:
        content["initial_camera"] = initial_camera(content["initial_camera"])
    title = content["title"]
    tick_minimum, tick_maximum = (1_000_000, 1_000_000_000) if schema in (7, 8, 9, 10, 11) else (1, 1000)
    if (not isinstance(title, str) or not title or title != title.strip()
            or len(title.encode()) > 128 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in title)
            or type(content[tick_field]) is not int or not tick_minimum <= content[tick_field] <= tick_maximum):
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
            or schema == 4 and document["format_version"] != 8
            or schema == 5 and document["format_version"] != 9
            or schema == 6 and document["format_version"] != 10
            or schema == 7 and document["format_version"] != 11
            or schema == 8 and document["format_version"] != 12
            or schema == 9 and document["format_version"] != 13
            or schema == 10 and document["format_version"] != 14
            or schema == 11 and document["format_version"] != 15):
        raise ValueError("content schema1 requires Scene1–5; schema2 requires Scene6; schema3 requires Scene7; schema4 requires Scene8; schema5 requires Scene9; schema6 requires Scene10; schema7 requires Scene11; schema8 requires Scene12; schema9 requires Scene13; schema10 requires Scene14; schema11 requires Scene15")
    meshes, textures = content["meshes"], content.get("textures", {})
    if (not isinstance(meshes, dict) or not isinstance(textures, dict)
            or len(meshes) + len(textures) > MAX_MESHES):
        raise ValueError("content exceeds 64 combined asset mappings")
    if set(meshes) != mesh_ids or set(textures) != texture_ids or set(meshes) & set(textures):
        raise ValueError("content mappings differ from the current scene mesh/texture dependency closure")
    texture_roles = scene_texture_roles(document)
    source_specs = {}
    for kind, records, pattern, maximum in (("mesh", meshes, MESH_PATH, MIB),
                                            ("texture", textures, TEXTURE_PATH, MAX_TEXTURE_BYTES)):
        for identity, record in records.items():
            cancelled(cancel_file)
            expected = ({"path", "sha256"}
                        | ({"normal_policy"} if schema >= 5 and kind == "mesh" else set())
                        | ({"texture_role"} if schema >= 6 and kind == "texture" else set()))
            exact_fields(record, expected, f"{kind} record")
            if schema >= 5 and kind == "mesh" and record["normal_policy"] not in ("flat", "authored_when_complete"):
                raise ValueError("invalid mesh normal policy")
            if schema >= 6 and kind == "texture" and record["texture_role"] != texture_roles.get(identity):
                raise ValueError("texture role differs from its assigned material slot")
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
    validate_collider_fields(document)
    validate_input_fields(document)
    validate_sun_shadows(document)
    validate_surface_fields(document)
    scene = json.loads(json.dumps(document["scene"]))
    try:
        display = scene.get("display", {"type":"legacy_sdr"})
        if display["type"] == "legacy_sdr": scene.pop("display", None)
        else: display["exposure_stops"] = struct.unpack("f", struct.pack("f", display["exposure_stops"]))[0]
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
            response = material.get("response", {"type":"legacy_diffuse"})
            if response["type"] == "legacy_diffuse": material.pop("response", None)
            else:
                rounded = struct.unpack("f", struct.pack("f", response["roughness"]))[0]
                response["roughness"] = .08 if rounded == struct.unpack("f", struct.pack("f", .08))[0] else rounded
            material["tint"] = [struct.unpack("f", struct.pack("f", value))[0]
                                for value in material["tint"]]
            for field in ("name", "base_color_texture", "roughness_texture"):
                if material.get(field) is None:
                    material.pop(field, None)
        for obj in scene["objects"]:
            for field in ("name", "mesh", "material", "collider"):
                if obj.get(field) is None:
                    obj.pop(field, None)
            if obj.get("collider") is not None:
                collider = obj["collider"]
                collider["offset"] = [struct.unpack("f", struct.pack("f", value))[0] for value in collider["offset"]]
                shape = collider["shape"]
                if shape["type"] == "sphere":
                    shape["radius"] = struct.unpack("f", struct.pack("f", shape["radius"]))[0]
                else:
                    shape["half_extents"] = [struct.unpack("f", struct.pack("f", value))[0] for value in shape["half_extents"]]
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
    if content["schema"] == 11 and kit["profile"] != HUD_PROFILE:
        raise ValueError("runtime kit does not support Scene15 runtime_hud; prepare a current runtime kit")
    if content["schema"] == 10 and kit["profile"] not in (LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene14 lifecycle_commands; prepare a current runtime kit")
    if content["schema"] == 9 and kit["profile"] not in (STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene13 runtime_state; prepare a current runtime kit")
    if content["schema"] == 8 and kit["profile"] not in (SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene12 spatial_queries; prepare a current runtime kit")
    if content["schema"] == 7 and kit["profile"] not in (INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene11 input_actions; prepare a current runtime kit")
    if content["schema"] == 6 and kit["profile"] not in (ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene10 roughness_maps; prepare a current runtime kit")
    if content["schema"] == 5 and kit["profile"] not in (SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene9 surface_materials; prepare a current runtime kit")
    if content["schema"] == 4 and kit["profile"] not in (SUN_SHADOW_PROFILE, SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene8 sun_shadows; prepare a current runtime kit")
    if content["schema"] == 3 and kit["profile"] not in (LIGHTING_PROFILE, SUN_SHADOW_PROFILE, SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene7 authored_lighting; prepare a current runtime kit")
    if content["schema"] == 2 and kit["profile"] not in (MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE, SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support Scene6 opaque_materials; prepare a current runtime kit")
    if camera is not None and kit["profile"] not in (CAMERA_PROFILE, MATERIAL_PROFILE, LIGHTING_PROFILE, SUN_SHADOW_PROFILE, SURFACE_PROFILE, ROUGHNESS_PROFILE, INPUT_PROFILE, SPATIAL_PROFILE, STATE_PROFILE, LIFECYCLE_PROFILE, HUD_PROFILE):
        raise ValueError("runtime kit does not support initial_camera; prepare a current runtime kit")
    descriptor_version = {1: 1, 2: 2, 3: 2, 4: 2, 5: 3, 6: 4, 7: 5, 8: 5, 9: 5, 10: 5, 11: 5}[content["schema"]]
    tick_field = "tick_nanos" if content["schema"] in (7, 8, 9, 10, 11) else "tick_millis"
    fixed_dt = content[tick_field] / (1_000_000_000 if content["schema"] in (7, 8, 9, 10, 11) else 1000)
    descriptor = {"format": "ruster-application", "version": descriptor_version, "title": content["title"],
                  "scene": "resources/scene.json", tick_field: content[tick_field],
                  "meshes": {identity: "resources/" + record["path"]
                             for identity, record in content["meshes"].items()}}
    if content["schema"] >= 5:
        descriptor["mesh_normal_policies"] = {identity: record["normal_policy"] for identity, record in content["meshes"].items()}
    if content["schema"] >= 6:
        descriptor["texture_roles"] = {identity: record["texture_role"] for identity, record in content["textures"].items()}
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
                or result.get("tick") != "0" or result.get("fixed_dt") != fixed_dt
                or result.get("authored_scene") != typed_scene(document)):
            raise ValueError("runtime preflight did not retain creation identity, tick or authored scene")
        if camera is not None and result.get("initial_camera") != camera:
            raise ValueError("runtime preflight did not retain explicit initial camera")
        if content["schema"] in (7, 8, 9, 10, 11) and result.get("authored_actions") != authored_actions(document):
            raise ValueError("runtime preflight did not retain authored input actions")
        if content["schema"] in (9, 10, 11):
            bindings = document.get("authoring", {}).get("bindings", {})
            def typed_defaults(binding: dict) -> dict:
                # StateValue is binary64, even when authored JSON used an integer
                # token that Python can retain with arbitrary precision.
                return {name: {"type": field["type"], "value": (
                    float(field["value"]) if field["type"] == "number" else
                    [float(n) for n in field["value"]] if field["type"] == "vec3" else field["value"])}
                    for name, field in binding.get("state_defaults", {}).items()}
            sources = document.get("authoring", {}).get("sources", {})
            # The public runtime report includes every captured binding. A
            # stateless legacy Object contributes an empty field map.
            defaults = {identity: typed_defaults(binding) for identity, binding in bindings.items()}
            api5 = any(binding.get("enabled") and
                       sources.get(binding["source_id"], {}).get("host_api_version") == 5
                       for binding in bindings.values())
            if api5 and kit["profile"] not in (LIFECYCLE_PROFILE, HUD_PROFILE):
                raise ValueError("API5 requires lifecycle_commands runtime capability")
            observed = result.get("runtime_state")
            if not isinstance(observed, dict) or set(observed) != set(defaults):
                raise ValueError("runtime preflight did not initialize authored state defaults")
            # Python bool compares equal to0/1. Validate typed values before
            # comparing; int/float JSON representations of one binary64 value
            # may legitimately differ between the authored file and Rust output.
            try:
                validate_state_fields({"format_version": 13, "authoring": {"bindings": {
                    identity: {**{key: value for key, value in binding.items() if key != "commands"}, "state_defaults": observed[identity]}
                    for identity, binding in bindings.items() if identity in observed}}})
            except ValueError as error:
                raise ValueError("runtime preflight did not initialize authored state defaults") from error
            if api5:
                if any(set(observed[identity]) != set(fields) or any(
                        observed[identity][name]["type"] != default["type"]
                        for name, default in fields.items()) for identity, fields in defaults.items()):
                    raise ValueError("runtime preflight changed initialized state schema")
            elif observed != defaults:
                raise ValueError("runtime preflight did not initialize authored state defaults")
        if content["schema"] in (10, 11) or (content["schema"] == 9 and api5):
            metadata = {"initialized", "commit_revision", "last_step_commit_revision",
                        "command_receipt", "command_receipts", "captured_commands", "runtime_scene"}
            if (not metadata <= result.keys() or result.get("initialized") is not True or result.get("commit_revision") != "0"
                    or result.get("last_step_commit_revision") is not None
                    or result.get("command_receipt") is not None or result.get("command_receipts") != []
                    or result.get("runtime_scene") != typed_scene(document)):
                raise ValueError("runtime preflight did not complete initialization without command/tick work")
            catalog = result.get("captured_commands")
            expected_catalog = {(owner, name): len(definition["arguments"])
                                for owner, binding in bindings.items()
                                for name, definition in binding.get("commands", {}).items()}
            if not isinstance(catalog, list) or len(catalog) != len(expected_catalog):
                raise ValueError("runtime preflight did not retain captured command catalog")
            seen = set()
            for row in catalog:
                if not isinstance(row, dict):
                    raise ValueError("invalid captured command metadata")
                key = (row.get("binding_id"), row.get("command_id"))
                if (key in seen or key not in expected_catalog or type(row.get("argument_count")) is not int
                        or row.get("argument_count") != expected_catalog[key]
                        or not isinstance(row.get("schema_hash"), str) or not HASH.fullmatch(row["schema_hash"])):
                    raise ValueError("runtime preflight did not retain captured command catalog")
                seen.add(key)
        if content["schema"] == 11:
            hud = typed_hud(document.get("authoring", {}).get("hud", {}))
            if hud != typed_hud({}) and result.get("authored_hud") != hud:
                raise ValueError("runtime preflight did not retain authored HUD")
            observed_hud = result.get("hud")
            exact_fields(observed_hud, {"labels"}, "resolved HUD")
            labels = observed_hud["labels"]
            expected_ids = [label["id"] for label in hud["elements"]] if hud["enabled"] else []
            if (result.get("hud_presentation") != "committed" or not isinstance(labels, list)
                    or len(labels) != len(expected_ids)):
                raise ValueError("runtime preflight did not resolve committed HUD labels")
            for label, identity in zip(labels, expected_ids):
                exact_fields(label, {"id", "text", "diagnostic"}, "resolved HUD label")
                if (label["id"] != identity or not isinstance(label["text"], str)
                        or len(label["text"].encode()) > 2048
                        or label["diagnostic"] not in (None, "missing_binding", "disabled_binding",
                            "missing_source", "missing_field", "type_changed", "unavailable_state", "output_overflow")):
                    raise ValueError("runtime preflight did not resolve bounded HUD labels")
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
