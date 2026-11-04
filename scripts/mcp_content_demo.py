#!/usr/bin/env python3
"""Official MCP SDK content smoke against an enabled disposable scene project.
Run: uv run --with mcp==2.3.0 scripts/mcp_content_demo.py
Set RUSTER_MCP_SOCKET. The project must contain meshes/pyramid.obj (override
RUSTER_MCP_CONTENT_SOURCE / RUSTER_MCP_CONTENT_ASSET_ID for another fixture).
RUSTER_MCP_DEMO_KEEP=1 retains the assignment for a subsequent native Undo.
Catalog imports persist immediately; the demo never saves the authored scene,
changes source/shader files or controls the editor's live preview.
"""
import asyncio
import json
import os
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/ruster-mcp"
    source = os.environ.get("RUSTER_MCP_CONTENT_SOURCE", "meshes/pyramid.obj")
    asset_id = os.environ.get("RUSTER_MCP_CONTENT_ASSET_ID", "pyramid")
    params = StdioServerParameters(command=str(binary), env={**os.environ})
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as client:
            info = await client.initialize()
            tools = await client.list_tools()
            assert {tool.name for tool in tools.tools} == {
                "inspect", "edit", "undo", "redo", "replay", "scene_inspect",
                "scene_transform", "scene_undo", "scene_redo", "scene_replay",
                "asset_inspect", "asset_import", "asset_refresh", "asset_rename",
                "scene_mesh", "shader_refresh",
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

            async def catalog():
                first = await call("asset_inspect")
                records = list(first["assets"])
                page = first
                while page["next_offset"] is not None:
                    page = await call("asset_inspect", {"offset": page["next_offset"]})
                    assert page["asset_revision"] == first["asset_revision"], page
                    assert page["session_token"] == first["session_token"], page
                    records.extend(page["assets"])
                assert len(records) == first["asset_count"], first
                return first, records

            before = await call("scene_inspect")
            assert not before["drafts_pending"] and not before["recovery_pending"], before
            assert before["objects"], "Use a disposable scene with at least one object"
            first = before["objects"][0]
            assert first.get("mesh") != asset_id, (
                "Use a disposable scene whose first object has another mesh/cube"
            )
            expected = {
                "expected_session_token": before["session_token"],
                "expected_revision": before["revision"],
            }
            baseline = await call("scene_replay", {**expected, "ticks": "10"})
            metadata, records = await catalog()
            assert metadata["asset_revision"] == before["asset_revision"], metadata
            assert metadata["shader_revision"] == before["shader_revision"], metadata
            registered = next((record for record in records if record["id"] == asset_id), None)
            imported = None
            if registered is None:
                imported = await call("asset_import", {
                    **expected, "expected_asset_revision": metadata["asset_revision"],
                    "asset_id": asset_id, "source": source,
                })
                assert imported["asset"]["id"] == asset_id, imported
                assert imported["scene_revision"] == before["revision"], imported
                metadata, records = await catalog()
                registered = next(record for record in records if record["id"] == asset_id)
                assert imported["asset_revision"] == metadata["asset_revision"], metadata
            assert registered["source"] == source and registered["status"] == "ready", registered
            # Catalog import must leave the old authored scene and isolated replay valid.
            assert await call("scene_replay", {**expected, "ticks": "10"}) == baseline
            ready = await call("scene_inspect")
            assert ready["objects"] == before["objects"], ready
            assert ready["revision"] == before["revision"], ready
            assert ready["preview_fingerprint"] == before["preview_fingerprint"], ready
            assigned = await call("scene_mesh", {
                **expected, "expected_asset_revision": metadata["asset_revision"],
                "object_id": first["id"], "asset_id": asset_id,
            })
            assert assigned["before"] == first.get("mesh"), assigned
            assert assigned["after"] == asset_id and assigned["object_id"] == first["id"], assigned
            assert assigned["asset_revision"] == metadata["asset_revision"], assigned
            current = await call("scene_inspect")
            assert current["objects"][0]["mesh"] == asset_id, current
            assert current["revision"] == assigned["revision"], current
            assert current["preview_tick"] == before["preview_tick"], current
            assert current["preview_fingerprint"] == before["preview_fingerprint"], current
            expected["expected_revision"] = current["revision"]
            assert int(metadata["asset_revision"]) > 0, metadata
            await call("asset_refresh", {
                **expected,
                "expected_asset_revision": str(int(metadata["asset_revision"]) - 1),
                "asset_id": asset_id,
            }, "stale_asset_revision")
            assert await call("scene_inspect") == current, "Stale catalog command changed scene"
            unchanged_catalog, _ = await catalog()
            assert unchanged_catalog == {**metadata, "revision": current["revision"]}, (
                "Stale command changed catalog/shader state"
            )
            print(json.dumps({
                "client": "official Python MCP SDK 2.3.0", "protocol": info.protocol_version,
                "baseline": baseline, "import": imported, "assignment": assigned,
                "asset_revision": metadata["asset_revision"],
                "shader_revision": metadata["shader_revision"],
                "stale_asset_revision_rejected": True,
            }, indent=2))
            if os.environ.get("RUSTER_MCP_DEMO_KEEP") != "1":
                undone = await call("scene_undo", expected)
                restored = await call("scene_inspect")
                assert restored["revision"] == undone["revision"], undone
                assert restored["objects"] == before["objects"], restored
                assert restored["preview_tick"] == before["preview_tick"], restored
                assert restored["preview_fingerprint"] == before["preview_fingerprint"], restored
                print("Shared authored Undo restored mesh assignment; catalog import remains persisted.")
            else:
                print("Mesh assignment retained for native Undo; authored scene remains unsaved.")


if __name__ == "__main__":
    asyncio.run(main())
