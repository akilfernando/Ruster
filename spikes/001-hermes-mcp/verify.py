#!/usr/bin/env python3
"""Verify actual Hermes stream-json tool receipts; no SDK or model required."""
import argparse
import hashlib
import json
from pathlib import Path


def events(path):
    out = []
    for line in path.read_text().splitlines():
        try:
            out.append(json.loads(line))
        except ValueError:
            pass  # CLI also prints a human-readable Session: line.
    return out


def unpack(value):
    while isinstance(value, str):
        value = json.loads(value)
    for key in ("result", "error"):
        if key in value and isinstance(value[key], str):
            return unpack(value[key])
    return value


def verify(audit):
    first = events(audit / "client-1.jsonl")
    second = events(audit / "client-2.jsonl")
    calls = [e for e in first if e.get("type") == "tool_use"]
    results = [e for e in first if e.get("type") == "tool_result" and e.get("name", "").startswith("mcp__rusterize__")]
    inspections = [unpack(e["output"])["result"] for e in results if e["name"].endswith("__scene_inspect")]
    assert len(inspections) == 4
    before, edited, stale_readback, restored = inspections
    transforms = [e["input"] for e in calls if e["name"] == "mcp__rusterize__scene_transform"]
    undos = [e["input"] for e in calls if e["name"] == "mcp__rusterize__scene_undo"]
    assert len(transforms) == 2 and transforms[0] == transforms[1]
    assert transforms[0]["expected_session_token"] == before["session_token"]
    assert transforms[0]["expected_revision"] == before["revision"]
    assert len(undos) == 1 and undos[0] == {
        "expected_session_token": stale_readback["session_token"],
        "expected_revision": stale_readback["revision"],
    }
    assert before["scene_id"] == "hermes-disposable-spike"
    assert [x["revision"] for x in inspections] == ["0", "1", "1", "2"]
    assert before["objects"][0]["id"] == "left"
    assert before["objects"][0]["transform"]["translation"] == [-2, 0, 0]
    assert edited["objects"][0]["transform"]["translation"] == [-1, 0, 0]
    assert edited == stale_readback
    assert restored["objects"] == before["objects"]
    assert restored["preview_fingerprint"] == before["preview_fingerprint"]
    assert restored["preview_tick"] == before["preview_tick"]
    errors = [unpack(e["output"])["error"] for e in results if not unpack(e["output"]).get("ok")]
    assert len(errors) == 1 and errors[0]["code"] == "stale_revision"
    fresh = [unpack(e["output"])["result"] for e in second
             if e.get("type") == "tool_result" and e.get("name") == "mcp__rusterize__scene_inspect"]
    assert len(fresh) == 1 and fresh[0] == restored
    session_results = [next(e for e in data if e.get("type") == "result") for data in (first, second)]
    assert len({e["session_id"] for e in session_results}) == 2
    assert all(e["exit_code"] == 0 for e in session_results)
    token_hash = hashlib.sha256(before["session_token"].encode()).hexdigest()
    return {
        "verdict": "PARTIAL",
        "transport_workflow_verified": True,
        "inspection_revisions": [x["revision"] for x in inspections],
        "objects_restored_exactly": True,
        "preview_unchanged": True,
        "stale_error": errors[0]["code"],
        "editor_token_sha256": token_hash,
        "same_editor_identity_after_client_restart": fresh[0]["session_token"] == before["session_token"],
        "normal_terminal_free_startup_proven": False,
        "secure_project_binding_proven": False,
        "final_dirty": restored["dirty"],
        "tool_calls": [{"name": e["name"], "handle": e["tool_call_id"]} for e in calls],
        "sessions": [{"id": e["session_id"], "duration_ms": e["duration_ms"], "tokens": e["tokens"]}
                     for e in session_results],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("audit", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.audit), indent=2))
