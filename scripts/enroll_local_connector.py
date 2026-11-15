#!/usr/bin/env python3
"""Deliberately provision NEW private local connector enrollment/registry fixtures.

Never edits the input registry, client settings, live native owner or installation.
Use only matched upgraded artifacts. Persistent access is granted to the explicit
--project IDs, not to a client name, model claim, or arbitrary tool-supplied path.
"""
import argparse
import json
import os
from pathlib import Path
import secrets
import stat


def canonical(raw):
    path = Path(raw)
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ValueError("absolute canonical path without symlinks required")
    return path


def private_write(path, value, mode=0o600):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(fd, "w") as stream:
        stream.write(value)
        stream.flush()
        os.fsync(stream.fileno())


def strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def fields(value, required, optional=()):
    if not isinstance(value, dict) or not set(required) <= value.keys() or value.keys() - set(required) - set(optional):
        raise ValueError("invalid registry fields")


def bounded_text(value, limit):
    return isinstance(value, str) and 0 < len(value.encode("utf-8")) <= limit


def absolute(value):
    return isinstance(value, str) and bool(value.encode("utf-8")) and "\0" not in value and Path(value).is_absolute()


def integers(value, unsigned, signed=()):
    fields(value, (*unsigned, *signed))
    for key in unsigned:
        if type(value[key]) is not int or not 0 <= value[key] <= 2**64 - 1:
            raise ValueError("invalid unsigned identity")
    for key in signed:
        if type(value[key]) is not int or not -(2**63) <= value[key] < 2**63:
            raise ValueError("invalid signed identity")


def registry_records(raw):
    value = json.loads(raw, object_pairs_hook=strict_object, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))
    fields(value, ("projects",))
    records = value["projects"]
    if not isinstance(records, list) or not 1 <= len(records) <= 32:
        raise ValueError("1..32 projects required")
    identities = set()
    for record in records:
        fields(record, ("id", "name", "scene_id"), ("enrollment", "locator", "binding", "startup"))
        pid, name = record["id"], record["name"]
        if not bounded_text(pid, 128) or not all(c.isascii() and (c.isalnum() or c in "-_") for c in pid) or not bounded_text(name, 128) or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in name) or not bounded_text(record["scene_id"], 128) or pid in identities or name in identities:
            raise ValueError("invalid or ambiguous registered identity")
        identities.update((pid, name))
        if record.get("enrollment") is not None and not isinstance(record["enrollment"], str):
            raise ValueError("invalid enrollment path")
        if isinstance(record.get("enrollment"), str):
            record["enrollment"].encode("utf-8")
        locator, binding, startup = record.get("locator", ""), record.get("binding"), record.get("startup")
        if not isinstance(locator, str):
            raise ValueError("invalid locator")
        if startup is None:
            fields(binding, ("wire", "incarnation", "generation", "connection"))
            if not absolute(locator) or binding["wire"] != "ruster-owner-1" or not all(bounded_text(binding[k], 256) for k in ("incarnation", "generation", "connection")):
                raise ValueError("invalid registered routing")
        else:
            fields(startup, ("scene_path", "native_binary", "state_directory", "binary_identity", "routing_identity"))
            if locator or binding is not None or not all(absolute(startup[k]) for k in ("scene_path", "native_binary", "state_directory")):
                raise ValueError("invalid startup routing")
            integers(startup["binary_identity"], ("dev", "ino", "len"), ("mtime", "mtime_nsec"))
            integers(startup["routing_identity"], ("directory_dev", "directory_ino", "owner_ino", "startup_ino"))
    return records


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True)
    parser.add_argument("--adapter", required=True)
    parser.add_argument("--output", required=True, help="New private directory; never overwrites")
    parser.add_argument("--connector-id", required=True)
    parser.add_argument("--project", action="append", required=True, help="Exact explicitly allowed registered ID; repeat as needed")
    args = parser.parse_args()
    registry, adapter = map(canonical, (args.registry, args.adapter))
    meta = registry.stat()
    if not stat.S_ISREG(meta.st_mode) or meta.st_uid != os.getuid() or stat.S_IMODE(meta.st_mode) != 0o600 or meta.st_size > 262144:
        parser.error("private bounded registry required")
    image = adapter.stat()
    if not stat.S_ISREG(image.st_mode) or image.st_uid != os.getuid() or image.st_mode & 0o022 or not image.st_mode & 0o100:
        parser.error("trusted owner-writable adapter executable required")
    try:
        records = registry_records(registry.read_text())
    except (ValueError, TypeError, UnicodeError):
        parser.error("invalid production project registry")
    ids = [p["id"] for p in records]
    if not 1 <= len(records) <= 32 or len(ids) != len(set(ids)) or any(p not in ids for p in args.project):
        parser.error("1..32 unique registered projects and exact allowlisted IDs required")
    if not bounded_text(args.connector_id, 128):
        parser.error("connector ID must be 1..128 UTF-8 bytes")
    root = Path(args.output)
    if not root.is_absolute() or root.parent.resolve(strict=True) != root.parent:
        parser.error("new output directory needs a canonical absolute parent")

    enrollment = root / "enrollment.json"
    grant = {"version": 1, "connector_id": args.connector_id, "executable": str(adapter),
             "binary_identity": {"dev": image.st_dev, "ino": image.st_ino, "len": image.st_size,
                                 "mtime": image.st_mtime_ns // 1_000_000_000,
                                 "mtime_nsec": image.st_mtime_ns % 1_000_000_000},
             "secret": secrets.token_hex(32), "generation": 1,
             "projects": [{"id": p, "generation": 1, "allowed": p in args.project} for p in ids]}

    for record in records:
        # Even nonallowed records use the enrolled path and fail closed. Never
        # silently fall back to recurring legacy approval for an enrolled client.
        record["enrollment"] = str(enrollment)
    new_registry = root / "registry.json"
    grant_text = json.dumps(grant, indent=2)
    registry_text = json.dumps({"projects": records}, indent=2)
    if len(grant_text.encode("utf-8")) > 8192 or len(registry_text.encode("utf-8")) > 262144:
        parser.error("generated enrollment or registry exceeds production reader bounds")
    root.mkdir(mode=0o700)
    private_write(enrollment, grant_text)
    private_write(new_registry, registry_text)
    launcher = root / "connect.py"
    private_write(launcher, "#!/usr/bin/python3\nimport os\nos.environ.pop('RUSTER_MCP_LOCATOR', None)\n"
                  f"os.environ['RUSTER_MCP_PROJECT_REGISTRY'] = {str(new_registry)!r}\n"
                  f"os.execv({str(adapter)!r}, [{str(adapter)!r}])\n", 0o700)
    print(json.dumps({"registry": str(new_registry), "stable_mcp_command": str(launcher),
                      "enrolled_projects": args.project, "live_client_configuration_changed": False}))


if __name__ == "__main__":
    main()
