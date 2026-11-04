#!/usr/bin/env python3
"""Official MCP SDK scene smoke; requires an enabled disposable scene editor.
Run: uv run --with mcp==2.3.0 scripts/mcp_scene_demo.py
Set RUSTER_MCP_SOCKET; RUSTER_MCP_DEMO_KEEP=1 retains the edit for native Undo.
The demo does not save or change the editor preview.
"""
import asyncio
import copy
import json
import os
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/ruster-mcp"
    params = StdioServerParameters(command=str(binary), env={**os.environ})
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as client:
            info = await client.initialize()
            tools = await client.list_tools()
            assert {tool.name for tool in tools.tools} >= {
                "inspect", "edit", "undo", "redo", "replay", "scene_inspect",
                "scene_transform_many", "scene_duplicate_many", "scene_delete_many", "scene_transform", "scene_create", "scene_rename", "scene_duplicate", "scene_delete", "scene_undo", "scene_redo", "scene_replay",
            }

            async def call(name, arguments=None, error=None):
                response = await client.call_tool(name, arguments or {})
                value = response.structured_content
                assert value is not None, response
                assert response.is_error == (error is not None), value
                if error:
                    assert value["error"]["code"] == error, value
                    return value
                assert value["ok"], value
                return value["result"]

            before = await call("scene_inspect")
            assert not before["drafts_pending"] and not before["recovery_pending"], before
            assert 0 < len(before["objects"]) <= 16, before
            expected = {
                "expected_session_token": before["session_token"],
                "expected_revision": before["revision"],
            }
            baseline = await call("scene_replay", {**expected, "ticks": "0"})
            replayed = await call("scene_replay", {**expected, "ticks": "10"})
            assert baseline["ticks"] == "0" and replayed["ticks"] == "10"
            assert {k: v for k, v in baseline.items() if k != "ticks"} == {
                k: v for k, v in replayed.items() if k != "ticks"
            }
            assert baseline["object_count"] == before["object_count"]
            assert baseline["scene_id"] == before["scene_id"]
            assert await call("scene_inspect") == before, "Replay changed live editor state"

            first = before["objects"][0]
            proposed = copy.deepcopy(first["transform"])
            proposed["translation"][0] += 1
            arguments = {**expected, "object_id": first["id"], "transform": proposed}
            edited = await call("scene_transform", arguments)
            assert edited["changed"] and edited["diff"] == [{
                "object_id": first["id"], "before": first["transform"], "after": proposed, "before_mesh":first.get("mesh"), "after_mesh":first.get("mesh"),
            }], edited
            current = await call("scene_inspect")
            assert current["objects"][0]["transform"] == proposed, current
            assert current["preview_tick"] == before["preview_tick"], current
            assert current["preview_fingerprint"] == before["preview_fingerprint"], current
            await call("scene_transform", arguments, "stale_revision")
            assert await call("scene_inspect") == current, "Stale command changed state"
            expected["expected_revision"] = current["revision"]
            invalid = copy.deepcopy(proposed)
            invalid["scale"][0] = 0
            await call("scene_transform", {
                **expected, "object_id": first["id"], "transform": invalid,
            }, "invalid_arguments")
            await call("scene_replay", {**expected, "ticks": "10001"}, "work_limit")
            assert await call("scene_inspect") == current, "Invalid command changed state"
            print(json.dumps({
                "client": "official Python MCP SDK 2.3.0", "protocol": info.protocol_version,
                "baseline": baseline, "replayed": replayed, "edit": edited,
                "preview_tick_unchanged": current["preview_tick"],
            }, indent=2))
            if os.environ.get("RUSTER_MCP_DEMO_KEEP") != "1":
                undone = await call("scene_undo", expected)
                assert undone["diff"] == [{
                    "object_id": first["id"], "before": proposed, "after": first["transform"], "before_mesh":first.get("mesh"), "after_mesh":first.get("mesh"),
                }], undone
                restored = await call("scene_inspect")
                assert restored["objects"] == before["objects"], restored
                assert restored["preview_tick"] == before["preview_tick"], restored
                assert restored["preview_fingerprint"] == before["preview_fingerprint"], restored
                print("Shared authored Undo restored the original transforms; no save performed.")
            else:
                print("Authored edit retained for native Undo; no save performed.")


if __name__ == "__main__":
    asyncio.run(main())
