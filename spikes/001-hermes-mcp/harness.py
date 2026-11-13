#!/usr/bin/env python3
"""Throwaway real Hermes CLI experiment; never modifies production source.

Requires the reviewed local away_native helper and a fresh named profile.
Run only under the away/input-guard authorization documented in README.md.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import time

PROFILE = "e2-hermes-spike"
HOME = Path.home() / ".hermes/profiles" / PROFILE
HELPER = Path("/home/akilf/Projects/Ruster-Audits/e1-theme-logo-20261113/away_native.py")


def rpc(audit, method, params):
    """Use Hermes' installed authenticated gateway client, never copy secrets."""
    script = audit / "rpc.py"
    script.write_text("import asyncio,json,sys\nfrom hermes_cli.gateway_client import connect_gateway\n"
                      "async def main():\n async with connect_gateway() as c:\n"
                      "  result=await c.rpc(sys.argv[1],**json.loads(sys.argv[2]))\n"
                      "  if sys.argv[1]=='session.info': result={k:result.get(k) for k in ['model','provider','reasoning_effort','reasoning_effort_wire','tool_count','mcp_servers','tools','enabled_toolsets','profile_name','cwd','usage']}\n"
                      "  print(json.dumps(result))\n"
                      "asyncio.run(main())\n")
    env = {**os.environ, "HERMES_HOME": str(HOME)}
    runtime = json.loads(subprocess.check_output(["hermes", "--print-runtime-command"], text=True))
    runtime[-1] = runtime[-1].rsplit("runpy.run_module(", 1)[0] + "runpy.run_path(sys.argv.pop(1),run_name='__main__')"
    return subprocess.run(runtime + [str(script), method,
                           json.dumps(params if method == "runtime.describe" else {"profile": PROFILE, **params})], env=env,
                          text=True, capture_output=True, timeout=25, check=True).stdout


def sessions_in(path):
    ids = set()
    if path.exists():
        for line in path.read_text().splitlines():
            match = re.fullmatch(r"Session: (local-[a-zA-Z0-9-]+)", line.strip())
            if match:
                ids.add(match.group(1))
            try:
                data = json.loads(line)
            except ValueError:
                continue
            if isinstance(data, dict):
                for key in ("session_id", "sessionId"):
                    if isinstance(data.get(key), str) and data[key]:
                        ids.add(data[key])
    return sorted(ids)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("audit", type=Path)
    p.add_argument("--fixture", type=Path, required=True)
    args = p.parse_args()
    audit = args.audit.resolve(strict=True)
    assert HOME.is_dir() and not (HOME / "auth.json").exists(), "fresh un-cloned profile required"
    spec = importlib.util.spec_from_file_location("owned_away", HELPER)
    assert spec is not None and spec.loader is not None
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    # Audit-local in-process override only. Do not edit shared helper/infrastructure.
    def lease(argv, seconds):
        assert 0 < seconds <= 360
        return ["/usr/bin/timeout", "--signal=TERM", "--kill-after=2s", str(seconds) + "s", *argv]
    native.base.lease_command = lease
    fixture = audit / "fixture"
    fixture.mkdir(mode=0o700)
    scene = fixture / "hermes-spike.scene.json"
    data = json.loads(args.fixture.read_text())
    data["scene"]["id"] = "hermes-disposable-spike"
    scene.write_text(json.dumps(data, indent=2) + "\n")
    original = hashlib.sha256(scene.read_bytes()).hexdigest()
    run = audit / "native-opt-in"
    client = None
    all_ids = []
    registered = False
    receipts = {}
    try:
        r = native.launch(run, audit / "bin/ruster-editor", time.monotonic() + 360,
                          scene=scene, default=True, extra_env={"RUSTER_MCP_ENABLE": "1"})
        native.check(r)
        log = run / "native.log"
        end = time.monotonic() + 10
        sock = None
        while time.monotonic() < end:
            match = re.search(r"(/[^\s]*ruster-mcp-[^\s]*/[^\s]+)", log.read_text())
            if match:
                sock = match.group(1).rstrip("'\"")
                if Path(sock).is_socket():
                    break
            time.sleep(.1)
        assert sock and Path(sock).is_socket(), "no opt-in socket"
        receipts["socket"] = sock
        command = ["hermes", "-p", PROFILE, "mcp", "add", "rusterize", "--command",
                   str(audit / "bin/ruster-mcp"), "--env", "RUSTER_MCP_SOCKET=" + sock]
        installed = subprocess.run(command, input="a\n", capture_output=True, text=True, timeout=25)
        (audit / "registration.txt").write_text(installed.stdout + installed.stderr)
        assert installed.returncode == 0 and "Saved 'rusterize'" in installed.stdout
        registered = True
        receipts["registration_command"] = command
        # CLI-owned configuration readback; never write mcp_servers directly.
        readback = subprocess.run(["hermes", "-p", PROFILE, "mcp", "list"],
                                  text=True, capture_output=True, timeout=20, check=True)
        (audit / "registration-readback.txt").write_text(readback.stdout)
        assert "rusterize" in readback.stdout
        receipts["registration_reload"] = rpc(audit, "reload.mcp", {"confirm": True})
        prompts = [
            "Use ONLY rusterize MCP tools, no shell/files/delegation. This is an authorized disposable project. "
            "Call scene_inspect. STOP without mutations unless scene_id is hermes-disposable-spike and object left "
            "translation is [-2,0,0]. With the exact returned token/revision, change left translation to [-1,0,0], "
            "preserving rotation and scale. Inspect and confirm the edit. Attempt the SAME transform with the ORIGINAL "
            "revision to prove stale rejection. Inspect again, then scene_undo using CURRENT inspected token/revision. "
            "Finally scene_inspect and verify left translation restored to [-2,0,0]. Do not Save/export/create/redo. "
            "Report exact revision progression and stale error. Finite task, at most 8 tool calls.",
            "Use ONLY rusterize scene_inspect once. Verify scene_id hermes-disposable-spike and object left translation "
            "[-2,0,0]. Do not mutate. Report token/revision and this fresh-client readback."
        ]
        for i, prompt in enumerate(prompts, 1):
            native.check(r)
            query = audit / (f"query-{i}.txt")
            query.write_text(prompt)
            output = audit / (f"client-{i}.jsonl")
            command = ["hermes", "-p", PROFILE, "chat", "--provider", "openai-codex", "--model",
                       "gpt-6.1-sol", "--reasoning", "low", "--max-turns", "10", "--ignore-rules",
                       "--toolsets", "rusterize", "--in", str(fixture), "--oneshot", "--format",
                       "stream-json", "--query-file", str(query)]
            with output.open("w") as out:
                client = subprocess.Popen(command, stdout=out, stderr=subprocess.STDOUT, text=True)
                end = min(r["deadline"] - 30, time.monotonic() + 105)
                while client.poll() is None and time.monotonic() < end:
                    native.check(r)
                    time.sleep(.25)
                ids = sessions_in(output)
                all_ids.extend(ids)
                if client.poll() is None:
                    for sid in ids:
                        receipts[f"interrupt-{sid}"] = rpc(audit, "session.interrupt", {"session_id": sid})
                    client.terminate()
                    client.wait(timeout=10)
                    raise RuntimeError("bounded client deadline; interrupted server turn; no replay")
                assert client.returncode == 0, f"client {i} failed"
            receipts[f"client-{i}"] = {"command": command, "sessions": ids, "exit": client.returncode}
            assert ids, "session handle missing"
            for sid in ids:
                receipts[f"client-{i}-info"] = rpc(audit, "session.info", {"session_id": sid})
            if i == 1:
                # Profile-scoped supported reload, followed by a new CLI/session.
                receipts["reload"] = rpc(audit, "reload.mcp", {"session_id": ids[0], "confirm": True})
                assert '"status": "reloaded"' in receipts["reload"]
        native.check(r)
        assert hashlib.sha256(scene.read_bytes()).hexdigest() == original, "fixture was persisted unexpectedly"
        receipts["fixture_sha256"] = original
        native.close(None, run)
        receipts["normal_editor_close"] = True
    finally:
        for sid in sorted(set(all_ids)):
            try:
                deleted = subprocess.run(["hermes", "-p", PROFILE, "sessions", "delete", sid, "--yes"],
                                         text=True, capture_output=True, timeout=25, check=True)
                receipts[f"close-{sid}"] = deleted.stdout
            except Exception as exc:
                receipts[f"close-error-{sid}"] = str(exc)
        if registered:
            removed = subprocess.run(["hermes", "-p", PROFILE, "mcp", "remove", "rusterize"],
                                     input="y\n", text=True, capture_output=True, timeout=20)
            (audit / "removal.txt").write_text(removed.stdout + removed.stderr)
            listed = subprocess.run(["hermes", "-p", PROFILE, "mcp", "list"], text=True,
                                    capture_output=True, timeout=20)
            (audit / "removal-readback.txt").write_text(listed.stdout + listed.stderr)
            receipts["registration_removed"] = "rusterize" not in listed.stdout
        native.cleanup(run)
        (audit / "run-receipt.json").write_text(json.dumps(receipts, indent=2))
    print(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
