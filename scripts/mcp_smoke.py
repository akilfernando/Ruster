#!/usr/bin/env python3
"""One-command check that the AI route works end to end.

Starts the editor on a disposable copy of a scene (hidden workspace 2, through
ui_drive), starts rot-mcp over stdio with no configuration, and as an AI app
would: initialize, list tools (prints the size), read the scene, create and
move a cube, take a screenshot and undo. The editor is told to trust this
script's app name in its own private settings, so no prompt is needed.

    cargo build -p rot-editor --features inspection && cargo build -p rot-mcp
    python3 scripts/mcp_smoke.py

Standard library only. Exits non-zero on the first failed step.
"""
import argparse
import base64
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import ui_drive  # noqa: E402

APP = "rot-smoke"


class Adapter:
    """rot-mcp over stdio, spoken to as an MCP client."""

    def __init__(self, binary, cwd, config):
        env = {k: v for k, v in os.environ.items() if not k.startswith("ROT_")}
        env["XDG_CONFIG_HOME"] = str(config)
        self.process = subprocess.Popen([str(binary)], cwd=cwd, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True)
        self.next_id = 0

    def request(self, method, params=None):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            message["params"] = params
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise SystemExit(f"rot-mcp closed during {method}")
        return json.loads(line)

    def notify(self, method):
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
        self.process.stdin.flush()

    def call(self, name, arguments=None):
        reply = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        if "error" in reply:
            raise SystemExit(f"{name}: {reply['error']}")
        return reply["result"]

    def close(self):
        self.process.stdin.close()
        self.process.wait(5)


def step(label, ok, detail=""):
    print(f"{'ok  ' if ok else 'FAIL'} {label}{(': ' + detail) if detail else ''}")
    if not ok:
        raise SystemExit(1)


def inspected_objects(adapter, first_page):
    objects = list(first_page["objects"])
    page = first_page
    while page["next_offset"] is not None:
        reply = adapter.call("scene_inspect", {"offset": page["next_offset"]})["structuredContent"]
        step("read more objects", reply.get("ok") is True, str(reply.get("error", "")))
        page = reply["result"]
        objects.extend(page["objects"])
    step("read complete object list", len(objects) == first_page["object_count"])
    return objects


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile", default="debug", help="target/<profile> holding both binaries")
    parser.add_argument("--scene", default=str(ROOT / "examples/arrangement.scene.json"))
    parser.add_argument("--out", default=str(ROOT / "target/mcp-smoke"))
    args = parser.parse_args()
    binaries = ROOT / "target" / args.profile
    out = Path(args.out) / time.strftime("%Y%m%d-%H%M%S")

    with ui_drive.Editor(binaries / "rot-editor", out, scene=args.scene, lease=180) as editor:
        editor.wait_for("File", 30)
        config = editor.run / "config"
        store = config / "rot"
        store.mkdir(parents=True, exist_ok=True)
        os.chmod(store, 0o700)
        (store / "ai-apps.json").write_text(json.dumps(
            {"apps": [{"name": APP, "token": os.urandom(32).hex(), "allowed": True}]}))
        os.chmod(store / "ai-apps.json", 0o600)

        adapter = Adapter(binaries / "rot-mcp", editor.scene.parent, config)
        try:
            init = adapter.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                                  "clientInfo": {"name": APP, "version": "1"}})
            step("initialize", "result" in init, init.get("result", {}).get("serverInfo", {}).get("name", ""))
            adapter.notify("notifications/initialized")
            listed = adapter.request("tools/list", {})
            tools = listed["result"]["tools"]
            size = len(json.dumps(listed["result"], separators=(",", ":")))
            step("tools/list", size < 40 * 1024, f"{len(tools)} tools, {size} bytes")

            scene = adapter.call("scene_inspect")["structuredContent"]
            step("read the scene", scene.get("ok") is True,
                 f"{scene.get('result', {}).get('object_count')} objects" if scene.get("ok") else str(scene.get("error")))
            before = inspected_objects(adapter, scene["result"])

            created = adapter.call("scene_create", {"object_id": "smoke_cube", "name": "Smoke Cube"})["structuredContent"]
            step("create a cube", created.get("ok") is True, str(created.get("error", "")))
            transform = {"translation": [0, 1.5, 0], "rotation": [0, 0.5, 0], "scale": [1, 1, 1]}
            moved = adapter.call("scene_transform", {"object_id": "smoke_cube", "transform": transform})["structuredContent"]
            step("move it", moved.get("ok") is True, str(moved.get("error", "")))

            changed = adapter.call("scene_inspect")["structuredContent"]
            step("read the edited scene", changed.get("ok") is True, str(changed.get("error", "")))
            cube = next((obj for obj in inspected_objects(adapter, changed["result"])
                         if obj["id"] == "smoke_cube"), None)
            step("read back the cube transform", cube is not None and cube["transform"] == transform)

            shot = adapter.call("viewport_screenshot")
            images = [c for c in shot.get("content", []) if c.get("type") == "image"]
            if images:
                (out / "screenshot.png").write_bytes(base64.b64decode(images[0]["data"]))
            step("screenshot", bool(images), str(out / "screenshot.png") if images else str(shot["structuredContent"]))

            for _ in range(2):  # the move, then the create
                undone = adapter.call("scene_undo")["structuredContent"]
                step("undo", undone.get("ok") is True, str(undone.get("error", "")))
            restored = adapter.call("scene_inspect")["structuredContent"]
            step("read the restored scene", restored.get("ok") is True, str(restored.get("error", "")))
            after = inspected_objects(adapter, restored["result"])
            step("original objects restored exactly", after == before, f"{len(after)} objects")
        finally:
            adapter.close()
    print("AI route works end to end.")


if __name__ == "__main__":
    main()
