#!/usr/bin/env python3
"""One-time explicit saved-project registration; never grants native/browser consent."""
import argparse
import json
import os
from pathlib import Path
import stat


def canonical(raw):
    path = Path(raw)
    if not path.is_absolute() or path.resolve(strict=True) != path:
        raise ValueError("Use an absolute canonical path without symlinks")
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scene", required=True)
    parser.add_argument("--native", required=True)
    parser.add_argument("--adapter", required=True)
    parser.add_argument("--registration", required=True, help="New private directory; never overwrites")
    parser.add_argument("--id", required=True)
    parser.add_argument("--name", required=True)
    args = parser.parse_args()
    scene, native, adapter = map(canonical, (args.scene, args.native, args.adapter))
    if not args.id or len(args.id) > 128 or any(not (c.isascii() and (c.isalnum() or c in "-_")) for c in args.id):
        parser.error("ID must be 1–128 ASCII letters, digits, hyphens or underscores")
    if not args.name or len(args.name) > 128 or any(ord(c) < 32 or ord(c) == 127 for c in args.name):
        parser.error("Name must be 1–128 printable characters")
    for binary in (native, adapter):
        m = binary.stat()
        if not stat.S_ISREG(m.st_mode) or m.st_uid != os.getuid() or m.st_mode & 0o022 or not m.st_mode & 0o100:
            parser.error("Native and adapter must be trusted owner-writable executables")
    document = json.loads(scene.read_text())
    scene_id = document.get("scene", document)["id"]
    root = Path(args.registration)
    if not root.is_absolute() or root.parent.resolve(strict=True) != root.parent:
        parser.error("Registration directory must have an existing canonical absolute parent")
    root.mkdir(mode=0o700)
    routing = root / "routing"
    routing.mkdir(mode=0o700)
    m = native.stat()
    record = {"id": args.id, "name": args.name, "scene_id": scene_id, "startup": {
        "scene_path": str(scene), "native_binary": str(native), "state_directory": str(routing),
        "binary_identity": {"dev": m.st_dev, "ino": m.st_ino, "len": m.st_size,
                            "mtime": m.st_mtime_ns // 1_000_000_000, "mtime_nsec": m.st_mtime_ns % 1_000_000_000}}}
    registry = root / "registry.json"
    with registry.open("x") as f:
        os.chmod(registry, 0o600)
        json.dump({"projects": [record]}, f, indent=2)
    launcher = root / "connect.py"
    with launcher.open("x") as f:
        os.chmod(launcher, 0o700)
        f.write("#!/usr/bin/env python3\nimport os\n")
        f.write("os.environ.pop('RUSTER_MCP_LOCATOR', None)\n")
        f.write(f"os.environ['RUSTER_MCP_PROJECT_REGISTRY'] = {str(registry)!r}\n")
        f.write(f"os.execv({str(adapter)!r}, [{str(adapter)!r}])\n")
    print(json.dumps({"registry": str(registry), "stable_mcp_command": str(launcher),
                      "project": args.name, "selection_is_consent": False}, indent=2))


if __name__ == "__main__":
    main()
