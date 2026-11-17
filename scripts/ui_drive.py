#!/usr/bin/env python3
"""Drive and screenshot the Rot Engine editor without touching the user's desktop.

Launches an inspection build of the editor on Hyprland workspace 2 (silent, no
focus), sends app-scoped events over eframe's inspection socket, saves
screenshots and accessibility labels, then closes it. No global input is used.

    cargo build -p rot-editor --features inspection
    python3 scripts/ui_drive.py --scene examples/arrangement.scene.json \
        shot:start "click:Import Asset…" waitfor:Browse… shot:import key:Escape

The scene is opened from a disposable copy (the scene file, its asset catalog
and the files the catalog names), so a test can never change your project.
Pass --in-place to open the original instead.

Steps (run in order):
    click:LABEL        click the control with this exact accessible label
    click:LABEL#2      second control with that label (1-based)
    key:ctrl+shift+Z   press a key (egui key names: Tab, Enter, Escape, A, F2 ...)
    text:hello         type text into the focused field
    waitfor:LABEL      wait up to 10 s for a label to appear
    wait:0.5           sleep
    shot:NAME          save NAME.png
    labels:NAME        save NAME-labels.json (every visible label, role and bounds)
    tabwalk:40         press Tab N times and save the focus order to tabwalk.json
    resize:1600x1000   resize the test window

Output goes to --out (default target/ui/<timestamp>). View the PNGs before
calling a UI change done. Only standard-library Python is needed.
"""
import argparse
import json
import os
import shlex
import shutil
import socket
import struct
import subprocess
import sys
import time
from pathlib import Path

APP_CLASS = "org.rot.editor"
MODS = dict(alt=False, ctrl=False, shift=False, mac_cmd=False, command=False)
NVIDIA_ICD = "/usr/share/vulkan/icd.d/nvidia_icd.json"


# --- minimal msgpack (the inspection protocol's wire format) -----------------
def pack(value):
    if value is None:
        return b"\xc0"
    if value is True:
        return b"\xc3"
    if value is False:
        return b"\xc2"
    if isinstance(value, int):
        if 0 <= value < 128:
            return bytes([value])
        if -32 <= value < 0:
            return struct.pack("b", value)
        return (b"\xcf" + struct.pack(">Q", value)) if value >= 0 else (b"\xd3" + struct.pack(">q", value))
    if isinstance(value, float):
        return b"\xcb" + struct.pack(">d", value)
    if isinstance(value, str):
        data = value.encode()
        head = bytes([0xA0 | len(data)]) if len(data) < 32 else b"\xdb" + struct.pack(">I", len(data))
        return head + data
    if isinstance(value, (bytes, bytearray)):
        return b"\xc6" + struct.pack(">I", len(value)) + bytes(value)
    if isinstance(value, (list, tuple)):
        return b"\xdd" + struct.pack(">I", len(value)) + b"".join(pack(item) for item in value)
    if isinstance(value, dict):
        body = b"".join(pack(key) + pack(item) for key, item in value.items())
        return b"\xdf" + struct.pack(">I", len(value)) + body
    raise TypeError(type(value))


def unpack(data, at=0):
    tag = data[at]
    at += 1

    def take(fmt):
        nonlocal at
        size = struct.calcsize(fmt)
        (number,) = struct.unpack_from(fmt, data, at)
        at += size
        return number

    def raw(length):
        nonlocal at
        chunk = data[at:at + length]
        at += length
        return bytes(chunk)

    def sequence(length):
        nonlocal at
        items = []
        for _ in range(length):
            item, at = unpack(data, at)
            items.append(item)
        return items

    def mapping(length):
        flat = sequence(length * 2)
        return {(tuple(k) if isinstance(k, list) else k): v for k, v in zip(flat[::2], flat[1::2])}

    if tag < 0x80:
        return tag, at
    if tag >= 0xE0:
        return tag - 256, at
    if 0x80 <= tag <= 0x8F:
        return mapping(tag & 15), at
    if 0x90 <= tag <= 0x9F:
        return sequence(tag & 15), at
    if 0xA0 <= tag <= 0xBF:
        return raw(tag & 31).decode("utf-8", "replace"), at
    simple = {0xC0: None, 0xC2: False, 0xC3: True}
    if tag in simple:
        return simple[tag], at
    numbers = {0xCA: ">f", 0xCB: ">d", 0xCC: ">B", 0xCD: ">H", 0xCE: ">I", 0xCF: ">Q",
               0xD0: ">b", 0xD1: ">h", 0xD2: ">i", 0xD3: ">q"}
    if tag in numbers:
        return take(numbers[tag]), at
    lengths = {0xC4: ">B", 0xC5: ">H", 0xC6: ">I", 0xD9: ">B", 0xDA: ">H", 0xDB: ">I",
               0xDC: ">H", 0xDD: ">I", 0xDE: ">H", 0xDF: ">I"}
    if tag in lengths:
        length = take(lengths[tag])
        if tag in (0xC4, 0xC5, 0xC6):
            return raw(length), at
        if tag in (0xD9, 0xDA, 0xDB):
            return raw(length).decode("utf-8", "replace"), at
        if tag in (0xDC, 0xDD):
            return sequence(length), at
        return mapping(length), at
    fixed_ext = {0xD4: 1, 0xD5: 2, 0xD6: 4, 0xD7: 8, 0xD8: 16}
    if tag in fixed_ext:
        return raw(1 + fixed_ext[tag]), at
    if tag in (0xC7, 0xC8, 0xC9):
        length = take({0xC7: ">B", 0xC8: ">H", 0xC9: ">I"}[tag])
        return raw(1 + length), at
    raise ValueError(f"unsupported msgpack tag {tag:#x}")


# --- compositor helpers -------------------------------------------------------
# Agent shells may lack HYPRLAND_INSTANCE_SIGNATURE; fall back to the first instance.
HYPRCTL = ["hyprctl"] + ([] if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") else ["-i", "0"])


def hypr(command):
    return json.loads(subprocess.check_output([*HYPRCTL, "-j", command], timeout=3))


def hypr_eval(code):
    subprocess.run([*HYPRCTL, "eval", code], capture_output=True, text=True, timeout=3, check=True)


def desktop():
    return {"workspace": hypr("activeworkspace")["id"], "window": hypr("activewindow").get("address")}


def stage_copy(scene, dest):
    """Copy a scene with its asset catalog and catalog-listed sources into dest."""
    scene, root = Path(scene).resolve(strict=True), Path(scene).resolve().parent
    dest.mkdir(parents=True, exist_ok=True)
    shutil.copy2(scene, dest / scene.name)
    wanted = [path.name for path in root.glob("*.wgsl")]
    catalog = root / ".rot-assets.json"
    if catalog.is_file():
        wanted.append(catalog.name)
        data = json.loads(catalog.read_text())
        for group in ("assets", "models"):
            wanted += [entry["source"] for entry in data.get(group, {}).values() if "source" in entry]
    for relative in wanted:
        source = root / relative
        if source.is_file():
            (dest / relative).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, dest / relative)
    if (root / ".rot-models").is_dir():
        shutil.copytree(root / ".rot-models", dest / ".rot-models", dirs_exist_ok=True)
    return dest / scene.name


# --- the launched editor ------------------------------------------------------
class Editor:
    """One owned editor process. Use as a context manager; it always cleans up."""

    def __init__(self, binary, out, scene=None, lease=120, env=None, foreground=False, prefs=None,
                 in_place=False):
        self.binary = Path(binary).resolve(strict=True)
        self.out = Path(out).absolute()  # the compositor starts the editor from its own cwd
        self.out.mkdir(parents=True, exist_ok=True)
        self.run = self.out / "run"
        self.scene = Path(scene).resolve(strict=True) if scene else None
        if self.scene and not in_place:
            self.scene = stage_copy(self.scene, self.run / "project")
        self.lease, self.extra_env, self.foreground, self.prefs = lease, env or {}, foreground, prefs
        self.pid = self.sock = self.before = None
        self.actions = []

    def __enter__(self):
        for name in ("config", "cache", "state", "recovery"):
            (self.run / name).mkdir(parents=True, exist_ok=True)
        if self.prefs:
            (self.run / "state/editor.json").write_bytes(Path(self.prefs).read_bytes())
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        env = {
            "XDG_CONFIG_HOME": str(self.run / "config"), "XDG_CACHE_HOME": str(self.run / "cache"),
            "XDG_STATE_HOME": str(self.run / "state"), "ROT_EDITOR_STATE": str(self.run / "state/editor.json"),
            "ROT_RECOVERY_DIR": str(self.run / "recovery"), "EGUI_INSPECTION": f"127.0.0.1:{self.port}",
        }
        if os.path.exists(NVIDIA_ICD):
            env["VK_DRIVER_FILES"] = NVIDIA_ICD
        env.update(self.extra_env)
        launch = {"binary": str(self.binary), "scene": str(self.scene) if self.scene else None,
                  "env": env, "cwd": str(self.scene.parent if self.scene else self.run)}
        (self.run / "launch.json").write_text(json.dumps(launch, indent=1))
        argv = ["/usr/bin/timeout", "--kill-after=2s", f"{self.lease}s", sys.executable,
                str(Path(__file__).resolve()), "--exec", str(self.run)]
        compositor = shutil.which("hyprctl") is not None
        if compositor:
            self.before = desktop()
        if self.foreground:
            subprocess.Popen(argv, start_new_session=True)
        else:
            if not compositor:
                raise SystemExit("Hidden-workspace launch needs Hyprland. Use --foreground when the user is away.")
            if self.before["workspace"] == 2:
                raise SystemExit("Workspace 2 is in use as the hidden test workspace; switch away first.")
            hypr_eval("hl.exec_cmd(" + json.dumps(shlex.join(argv)) +
                      ',{workspace="2 silent",no_initial_focus=true,float=true,render_unfocused=true})')
        deadline = time.monotonic() + 30
        pid_file = self.run / "pid"
        while time.monotonic() < deadline:
            if pid_file.exists():
                self.pid = int(pid_file.read_text())
                try:
                    self.sock = socket.create_connection(("127.0.0.1", self.port), timeout=2)
                    break
                except OSError:
                    pass
            time.sleep(0.15)
        if not self.sock:
            self.cleanup()
            raise SystemExit(f"Editor did not start; see {self.run / 'editor.log'}")
        self.sock.settimeout(20)
        if self._exact(8) != b"eins\0\0\0\1":
            raise SystemExit("Not an inspection build. Build with: cargo build -p rot-editor --features inspection")
        if not self.foreground:
            window = None
            while not window and time.monotonic() < deadline:
                window = self._window()
                time.sleep(0.15)
            if not window or window["workspace"]["id"] != 2 or window["class"] != APP_CLASS:
                self.cleanup()
                raise SystemExit(f"Test window is not isolated on workspace 2: {window}")
        return self

    def __exit__(self, *_):
        (self.out / "actions.json").write_text(json.dumps(self.actions, indent=1))
        self.cleanup()
        if self.before and desktop() != self.before:
            print("warning: the active workspace or window changed during the run", file=sys.stderr)

    def cleanup(self):
        if self.sock:
            self.sock.close()
            self.sock = None
        if not self.pid:
            return
        try:
            if Path(f"/proc/{self.pid}/exe").resolve() == self.binary:
                os.kill(self.pid, 15)
                for _ in range(20):
                    time.sleep(0.1)
                    os.kill(self.pid, 0)
                os.kill(self.pid, 9)
        except (ProcessLookupError, FileNotFoundError, PermissionError):
            pass
        self.pid = None

    def _window(self):
        return next((w for w in hypr("clients") if w["pid"] == self.pid), None)

    def _exact(self, count):
        data = b""
        while len(data) < count:
            part = self.sock.recv(count - len(data))
            if not part:
                raise EOFError("inspection socket closed")
            data += part
        return data

    def call(self, request):
        data = pack(request)
        self.sock.sendall(struct.pack(">I", len(data)) + data)
        return unpack(self._exact(struct.unpack(">I", self._exact(4))[0]))[0]

    def events(self, values):
        self.call({"ApplyEvents": {"events": values}})
        time.sleep(0.15)

    # -- reading --
    def tree(self):
        return self.call("GetTree")["Tree"]

    def nodes(self):
        """[(id, role, label, bounds, actions)] for every accessible node."""
        result = []
        for node_id, node in self.tree()[2][0]:
            props = node[-1] if isinstance(node[-1], dict) else {}
            label = str(props.get("label", props.get("value", ""))).strip()
            result.append((node_id, node[0], label, props.get("bounds"), node[1]))
        return result

    def labels(self):
        return [label for _, _, label, _, _ in self.nodes() if label]

    def focused(self):
        tree = self.tree()
        node = dict(tree[2][0]).get(tree[2][3])
        if not node:
            return None
        props = node[-1] if isinstance(node[-1], dict) else {}
        return [node[0], str(props.get("label", props.get("value", ""))).strip()]

    # -- acting --
    def click(self, label, nth=1):
        hits = [n for n in self.nodes() if n[2] == label]
        if len(hits) < nth:
            raise SystemExit(f"No control labelled {label!r} (#{nth}). Visible: {sorted(set(self.labels()))}")
        node_id, role, _, bounds, actions = hits[nth - 1]
        action = "focus" if role == "textInput" else "click"
        if actions & (2 if action == "focus" else 1):
            self.events([{"AccessKitActionRequest": {"action": action, "targetTree": bytes(16),
                                                      "targetNode": node_id, "data": None}}])
        else:
            pos = {"x": (bounds[0] + bounds[2]) / 2, "y": (bounds[1] + bounds[3]) / 2}
            for pressed in (True, False):
                self.events([{"PointerMoved": pos}, {"PointerButton": {
                    "pos": pos, "button": "Primary", "pressed": pressed, "modifiers": MODS}}])
        self.actions.append(f"click:{label}")

    def key(self, combo):
        *modifiers, key = combo.split("+")
        mods = dict(MODS)
        for name in modifiers:
            mods[name.lower()] = True
        mods["command"] = mods["ctrl"]
        for pressed in (True, False):
            self.events([{"Key": dict(key=key, physical_key=None, pressed=pressed, repeat=False, modifiers=mods)}])
        self.actions.append(f"key:{combo}")

    def text(self, value):
        self.events([{"Text": value}])
        self.actions.append(f"text:{value}")

    def wait_for(self, label, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if label in self.labels():
                return
            time.sleep(0.15)
        raise SystemExit(f"Timed out waiting for {label!r}. Visible: {sorted(set(self.labels()))}")

    def shot(self, name):
        image = self.call({"GetScreenshot": {"pixels_per_point": 1.0}})["Screenshot"]
        path = self.out / f"{name}.png"
        path.write_bytes(bytes(image[1]))
        return path

    def dump_labels(self, name):
        rows = [dict(role=role, label=label, bounds=bounds) for _, role, label, bounds, _ in self.nodes() if label]
        (self.out / f"{name}-labels.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False))

    def tab_walk(self, count):
        order = []
        for _ in range(count):
            self.key("Tab")
            order.append(self.focused())
        (self.out / "tabwalk.json").write_text(json.dumps(order, indent=1, ensure_ascii=False))
        return order

    def resize(self, width, height):
        hypr_eval(f'for _,w in ipairs(hl.get_windows({{class="{APP_CLASS}"}})) do if w.pid=={self.pid} then '
                  f'hl.dispatch(hl.dsp.window.resize({{window=w,x={width},y={height},relative=false}})) end end')
        time.sleep(0.5)

    def step(self, spec):
        kind, _, value = spec.partition(":")
        if kind == "click":
            label, _, nth = value.partition("#")
            self.click(label, int(nth or 1))
        elif kind == "key":
            self.key(value)
        elif kind == "text":
            self.text(value)
        elif kind == "waitfor":
            self.wait_for(value)
        elif kind == "wait":
            time.sleep(float(value))
        elif kind == "shot":
            print(self.shot(value))
        elif kind == "labels":
            self.dump_labels(value)
        elif kind == "tabwalk":
            for entry in self.tab_walk(int(value)):
                print(entry)
        elif kind == "resize":
            width, height = value.lower().split("x")
            self.resize(int(width), int(height))
        else:
            raise SystemExit(f"Unknown step {spec!r}; see --help")


def exec_editor(run):
    """Runs inside the leased process: become the editor with a private environment."""
    run = Path(run)
    launch = json.loads((run / "launch.json").read_text())
    env = {k: v for k, v in os.environ.items() if not k.startswith(("ROT_", "EGUI_INSPECTION", "MANGOHUD"))}
    env.update(launch["env"])
    (run / "pid").write_text(str(os.getpid()))
    log = os.open(run / "editor.log", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.dup2(log, 1)
    os.dup2(log, 2)
    os.chdir(launch["cwd"])
    argv = [launch["binary"]] + (["--scene", launch["scene"]] if launch["scene"] else [])
    os.execve(launch["binary"], argv, env)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--exec":
        exec_editor(sys.argv[2])
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("steps", nargs="*")
    parser.add_argument("--binary", default="target/debug/rot-editor")
    parser.add_argument("--scene", help="scene file to open; omit for the project picker")
    parser.add_argument("--out", default=None)
    parser.add_argument("--size", default="1280x800")
    parser.add_argument("--lease", type=int, default=120, help="hard time limit for the editor process, seconds")
    parser.add_argument("--prefs", help="editor.json preferences file to start from")
    parser.add_argument("--env", action="append", default=[], metavar="KEY=VALUE")
    parser.add_argument("--foreground", action="store_true",
                        help="open a normal, focused window instead of hidden workspace 2 (only when the user is away)")
    parser.add_argument("--in-place", action="store_true",
                        help="open the scene file itself instead of a disposable copy")
    args = parser.parse_args()
    out = Path(args.out or f"target/ui/{time.strftime('%Y%m%d-%H%M%S')}")
    env = dict(item.split("=", 1) for item in args.env)
    with Editor(args.binary, out, args.scene, args.lease, env, args.foreground, args.prefs,
                args.in_place) as editor:
        if editor.before is not None:
            width, height = args.size.lower().split("x")
            editor.resize(int(width), int(height))
        time.sleep(1.0)
        for spec in args.steps:
            editor.step(spec)
    print(f"output: {out}")


if __name__ == "__main__":
    main()
