#!/usr/bin/env python3
"""Bounded Linux/Hyprland release viewer workload; never enables validation.

Run each binary with identical session/window settings. CPU is process utime+stime
from /proc, not wall time in acquire/present. Output includes complete viewer logs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=float, default=3)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--workspace", type=int, default=2, help="Require an existing prelaunch rule isolating the viewer here")
    parser.add_argument("--modes", nargs="+", choices=["idle", "play", "step", "pan"], default=["idle", "play", "step", "pan"])
    args = parser.parse_args()
    binary = args.binary.resolve()
    env = dict(os.environ)
    instances = json.loads(subprocess.check_output(["hyprctl", "instances", "-j"]))
    env["HYPRLAND_INSTANCE_SIGNATURE"] = instances[0]["instance"]

    def ctl(*arguments):
        return subprocess.check_output(["hyprctl", *arguments], env=env, text=True)

    def dispatch(expression):
        result = ctl("repl", "local r=hl.dispatch(" + expression + "); if not r.ok then error(r.error) end")
        if "error" in result.lower() or "warning" in result.lower():
            raise RuntimeError(result)

    def cpu(pid):
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")

    initial_workspace = json.loads(ctl("activeworkspace", "-j"))["id"]
    results = []
    output = dict(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  command=[str(binary)], recorded_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                  validation=False, warmup_seconds=1,
                  sample_seconds=args.seconds, repeats=args.repeats, required_workspace=args.workspace,
                  initial_active_workspace=initial_workspace,
                  proc_clock_ticks=os.sysconf("SC_CLK_TCK"),
                  platform=os.uname()._asdict() if hasattr(os.uname(), "_asdict") else list(os.uname()),
                  hyprland_version=ctl("version"), monitors=json.loads(ctl("monitors", "-j")),
                  results=results)
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    for mode in args.modes:
        for repeat in range(args.repeats):
            with tempfile.TemporaryFile(mode="w+") as log:
                launch_workspace = json.loads(ctl("activeworkspace", "-j"))["id"]
                if launch_workspace == args.workspace:
                    raise RuntimeError("Test workspace is now active; defer launches while the user occupies it")
                process = subprocess.Popen([str(binary)], stdout=log, stderr=log, env=env)
                try:
                    deadline = time.monotonic() + 10
                    client = None
                    while time.monotonic() < deadline:
                        client = next((c for c in json.loads(ctl("clients", "-j")) if c["pid"] == process.pid), None)
                        if client:
                            break
                        if process.poll() is not None:
                            raise RuntimeError("Viewer exited before mapping")
                        time.sleep(0.05)
                    if not client:
                        raise RuntimeError("Viewer did not map")
                    if client["workspace"]["id"] != args.workspace:
                        raise RuntimeError(f'Viewer isolation rule missing: expected workspace {args.workspace}, got {client["workspace"]}')
                    if json.loads(ctl("activeworkspace", "-j"))["id"] != launch_workspace:
                        raise RuntimeError("Viewer launch changed the active workspace")
                    window = json.dumps("address:" + client["address"])
                    dispatch(f'hl.dsp.window.float({{window={window},action="set"}})')
                    dispatch(f'hl.dsp.window.resize({{window={window},x=1100,y=720,relative=false}})')
                    dispatch(f'hl.dsp.window.center({{window={window}}})')

                    def key(name):
                        dispatch(f'hl.dsp.send_shortcut({{window={window},mods="",key="{name}"}})')

                    time.sleep(1)  # warmup includes mapping/resize/compositor animation
                    client = next(c for c in json.loads(ctl("clients", "-j")) if c["pid"] == process.pid)
                    if any(abs(actual - requested) > 1 for actual, requested in zip(client["size"], [1100, 720])):
                        raise RuntimeError(f'Unexpected surface size {client["size"]}')
                    if mode == "play":
                        key("space")
                    cpu_start, start = cpu(process.pid), time.monotonic()
                    sent = 0
                    while time.monotonic() - start < args.seconds:
                        if mode in ("step", "pan"):
                            key("n" if mode == "step" else ("Right" if sent % 2 == 0 else "Left"))
                            sent += 1
                        time.sleep(0.25 if mode == "step" else 0.1)
                    elapsed = time.monotonic() - start
                    used = cpu(process.pid) - cpu_start
                    client = next(c for c in json.loads(ctl("clients", "-j")) if c["pid"] == process.pid)
                    key("Escape")
                    process.wait(timeout=10)
                    log.seek(0)
                    results.append(dict(mode=mode, repeat=repeat + 1, wall_seconds=elapsed,
                                        cpu_seconds=used, cpu_percent_one_core=used / elapsed * 100,
                                        input_events=sent, final_title=client["title"],
                                        window_size=client["size"], workspace=client["workspace"],
                                        visible=client["visible"], active_workspace_before_launch=launch_workspace,
                                        active_workspace_at_end=json.loads(ctl("activeworkspace", "-j"))["id"],
                                        exit_code=process.returncode, log=log.read()))
                    args.output.write_text(json.dumps(output, indent=2) + "\n")
                    if process.returncode:
                        raise RuntimeError("Viewer failed: " + results[-1]["log"])
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(f"Wrote {len(results)} samples to {args.output}")


if __name__ == "__main__":
    main()
