#!/usr/bin/env python3
"""Measure the Phase 1 editor performance budgets (docs/PARITY-PLAN.md section 5).

    cargo build --release -p rot-editor -p rot-mcp --features rot-editor/inspection
    python3 scripts/measure_budgets.py --foreground          # only while the user is away

Uses scripts/ui_drive.py to launch disposable editor windows. Hidden-workspace
rendering is throttled, so timings taken without --foreground are not
performance evidence. Prints one JSON object; put the numbers in the commit.
Start time is when the launcher execs the editor, so "start" includes Vulkan
and window creation but not compositor launch latency.
"""
import argparse
import json
import os
import socket
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ui_drive  # noqa: E402


class _FastPoll:
    """ui_drive polls every 150 ms; poll every 5 ms so start times are precise."""

    def __getattr__(self, name):
        return getattr(time, name)

    @staticmethod
    def sleep(seconds):
        time.sleep(min(seconds, 0.005))


ui_drive.time = _FastPoll()
ROOT = Path(__file__).resolve().parents[1]
SCENE = ROOT / "examples/arrangement.scene.json"


def started_at(editor):
    """Wall time at which the launcher exec'd the editor (it writes `pid` just before)."""
    return (editor.run / "pid").stat().st_mtime


def wait_label(editor, label, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if label in editor.labels():
            return time.time()
        time.sleep(0.002)
    raise SystemExit(f"timed out waiting for {label!r}")


def step(editor):
    return editor.tree()[0]


def cpu_seconds(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def memory_mb(pid):
    values = {}
    for name in ("status", "smaps_rollup"):
        for line in Path(f"/proc/{pid}/{name}").read_text().splitlines():
            key, _, rest = line.partition(":")
            if key in ("VmRSS", "VmHWM", "Pss"):
                values[key] = round(int(rest.split()[0]) / 1024, 1)
    return values


def idle(editor, seconds):
    """Frames drawn and CPU used while nothing changes.

    Reading the tree of an idle editor costs two frames (checked: the step
    counter advances by 2 whether the gap is 0.1 s, 3 s or 10 s), so those are
    subtracted.
    """
    time.sleep(2)
    first, cpu = step(editor), cpu_seconds(editor.pid)
    time.sleep(seconds)
    used = cpu_seconds(editor.pid) - cpu
    return {"seconds": seconds, "frames": step(editor) - first - 2,
            "cpu_percent_one_core": round(used / seconds * 100, 2)}


def launch(args, out, scene=None, env=None):
    return ui_drive.Editor(args.binary, out, scene, 120, env or {}, args.foreground)


def editor_socket(pid):
    """The editor's MCP listener path, found from its own open sockets."""
    inodes = set()
    for fd in Path(f"/proc/{pid}/fd").iterdir():
        target = os.readlink(fd)
        if target.startswith("socket:["):
            inodes.add(target[8:-1])
    for line in Path("/proc/net/unix").read_text().splitlines()[1:]:
        parts = line.split()
        if len(parts) >= 8 and parts[6] in inodes and parts[7].endswith("/editor.sock"):
            return Path(parts[7])
    raise SystemExit("editor has no MCP listener; was ROT_MCP_ENABLE=1 set?")


class Adapter:
    def __init__(self, binary, locator):
        env = {k: v for k, v in os.environ.items() if not k.startswith("ROT_")}
        env["ROT_MCP_LOCATOR"] = str(locator)
        self.process = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=env)
        self.next_id = 0
        self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                    "clientInfo": {"name": "measure_budgets", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message).encode() + b"\n")
        self.process.stdin.flush()

    def request(self, method, params=None):
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params or {}})
        line = self.process.stdout.readline()
        return line, json.loads(line)

    def tool(self, name, arguments=None):
        start = time.perf_counter()
        _, reply = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        return time.perf_counter() - start, reply["result"]["structuredContent"]

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=5)


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, round(fraction * (len(ordered) - 1)))]


def ms(seconds):
    return round(seconds * 1000, 2)


def measure_mcp(args, out):
    result = {}
    line, listing = Adapter(args.mcp, "/nonexistent").request("tools/list")
    result["tool_list_bytes"] = len(line)
    result["tool_count"] = len(listing["result"]["tools"])
    with launch(args, out / "mcp", SCENE, {"ROT_MCP_ENABLE": "1"}) as editor:
        wait_label(editor, "Objects")
        adapter = Adapter(args.mcp, editor_socket(editor.pid).with_name("connection.json"))
        _, first = adapter.tool("scene_inspect")
        assert first["error"]["code"] == "approval_required", first
        for label in ("File", "Advanced", "Rot connection…", "Review adapter approval", "Confirm AI connection"):
            wait_label(editor, label, 5)
            editor.click(label)
        inspects, edits = [], []
        for index in range(args.edits):
            took, state = adapter.tool("scene_inspect")
            inspects.append(took)
            state = state["result"]
            target = state["objects"][0]
            transform = target["transform"]
            transform["translation"][1] = 0.01 * (index % 2)
            took, edit = adapter.tool("scene_transform", {
                "expected_session_token": state["session_token"], "expected_revision": state["revision"],
                "object_id": target["id"], "transform": transform})
            assert edit["ok"], edit
            edits.append(took)
        result["edits"] = len(edits)
        result["edit_ms_p50"], result["edit_ms_p95"] = ms(percentile(edits, .5)), ms(percentile(edits, .95))
        result["inspect_ms_p50"] = ms(percentile(inspects, .5))
        result["inspect_ms_p95"] = ms(percentile(inspects, .95))
        captures = []
        for _ in range(args.captures):
            _, state = adapter.tool("scene_inspect")
            _, view = adapter.tool("viewport_inspect")
            start = time.perf_counter()
            _, started = adapter.tool("viewport_capture_start", {
                "expected_session_token": state["result"]["session_token"],
                "expected_revision": state["result"]["revision"],
                "expected_view_revision": view["result"]["view_revision"]})
            if not started.get("ok"):
                result["capture_error"] = started
                break
            capture_id = started["result"]["capture_id"]
            while True:
                _, ready = adapter.tool("viewport_capture_inspect", {"capture_id": capture_id})
                status = ready.get("result", {}).get("status")
                if status != "pending":
                    break
                time.sleep(0.002)
            if status != "ready":
                result["capture_error"] = ready
                break
            captures.append(time.perf_counter() - start)
        if captures:
            result["captures"] = len(captures)
            result["capture_ms_p50"] = ms(statistics.median(captures))
            result["capture_ms_max"] = ms(max(captures))
        adapter.close()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default=str(ROOT / "target/release/rot-editor"))
    parser.add_argument("--mcp", default=str(ROOT / "target/release/rot-mcp"))
    parser.add_argument("--out", default=str(ROOT / "target/measure"))
    parser.add_argument("--runs", type=int, default=5, help="launches per start-time measurement")
    parser.add_argument("--idle", type=float, default=10, help="seconds of idle sampling")
    parser.add_argument("--edits", type=int, default=200)
    parser.add_argument("--captures", type=int, default=10)
    parser.add_argument("--foreground", action="store_true", help="focused window; only when the user is away")
    parser.add_argument("--skip", nargs="*", default=[], choices=["hub", "scene", "mcp"])
    args = parser.parse_args()
    out = Path(args.out)
    report = {"binary": args.binary, "foreground": args.foreground}
    if "hub" not in args.skip:
        starts = []
        for run in range(args.runs):
            with launch(args, out / f"hub{run}") as editor:
                starts.append(wait_label(editor, "New Project…") - started_at(editor))
                if run == 0:
                    report["hub_idle"] = idle(editor, args.idle)
                    report["hub_memory_mb"] = memory_mb(editor.pid)
        report["hub_start_ms"] = sorted(ms(s) for s in starts)
    if "scene" not in args.skip:
        starts = []
        for run in range(args.runs):
            with launch(args, out / f"scene{run}", SCENE) as editor:
                starts.append(wait_label(editor, "Objects") - started_at(editor))
                if run == 0:
                    report["scene_idle"] = idle(editor, args.idle)
                    report["scene_memory_mb"] = memory_mb(editor.pid)
                    # Play needs a Reset first in this editor; time both clicks.
                    clicked = time.time()
                    editor.click("Reset")
                    reset = wait_label(editor, "Runtime · arrangement.scene")
                    editor.click("Play")
                    playing = wait_label(editor, "Pause")
                    report["reset_ms"], report["reset_then_play_ms"] = ms(reset - clicked), ms(playing - clicked)
        report["scene_start_ms"] = sorted(ms(s) for s in starts)
    if "mcp" not in args.skip:
        report["mcp"] = measure_mcp(args, out)
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
