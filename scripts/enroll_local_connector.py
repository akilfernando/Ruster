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
    records = json.loads(registry.read_text())["projects"]
    ids = [p["id"] for p in records]
    if not 1 <= len(records) <= 32 or len(ids) != len(set(ids)) or any(p not in ids for p in args.project):
        parser.error("1..32 unique registered projects and exact allowlisted IDs required")
    if not args.connector_id or len(args.connector_id) > 128:
        parser.error("connector ID must be 1..128 characters")
    root = Path(args.output)
    if not root.is_absolute() or root.parent.resolve(strict=True) != root.parent:
        parser.error("new output directory needs a canonical absolute parent")
    root.mkdir(mode=0o700)
    enrollment = root / "enrollment.json"
    grant = {"version": 1, "connector_id": args.connector_id, "executable": str(adapter),
             "binary_identity": {"dev": image.st_dev, "ino": image.st_ino, "len": image.st_size,
                                 "mtime": image.st_mtime_ns // 1_000_000_000,
                                 "mtime_nsec": image.st_mtime_ns % 1_000_000_000},
             "secret": secrets.token_hex(32), "generation": 1,
             "projects": [{"id": p, "generation": 1, "allowed": p in args.project} for p in ids]}
    private_write(enrollment, json.dumps(grant, indent=2))
    for record in records:
        # Even nonallowed records use the enrolled path and fail closed. Never
        # silently fall back to recurring legacy approval for an enrolled client.
        record["enrollment"] = str(enrollment)
    new_registry = root / "registry.json"
    private_write(new_registry, json.dumps({"projects": records}, indent=2))
    launcher = root / "connect.py"
    private_write(launcher, "#!/usr/bin/python3\nimport os\nos.environ.pop('RUSTER_MCP_LOCATOR', None)\n"
                  f"os.environ['RUSTER_MCP_PROJECT_REGISTRY'] = {str(new_registry)!r}\n"
                  f"os.execv({str(adapter)!r}, [{str(adapter)!r}])\n", 0o700)
    print(json.dumps({"registry": str(new_registry), "stable_mcp_command": str(launcher),
                      "enrolled_projects": args.project, "live_client_configuration_changed": False}))


if __name__ == "__main__":
    main()
