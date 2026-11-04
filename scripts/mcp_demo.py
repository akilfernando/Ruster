#!/usr/bin/env python3
"""Real MCP SDK client: inspect, compare bounded replays, reject stale state, undo.
Run with uv run --with mcp==2.3.0 scripts/mcp_demo.py; see editor README.
Does not save; use a disposable project. Requires RUSTER_MCP_SOCKET.
"""
import asyncio
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
            assert {"inspect", "edit", "undo", "redo", "replay"} <= {tool.name for tool in tools.tools}

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

            before = await call("inspect")
            assert not before["drafts_pending"] and not before["mutation_blocked"], before
            expected = {"expected_session_token": before["session_token"], "expected_revision": before["revision"]}
            baseline = await call("replay", {**expected, "ticks": "10"})
            proposed = {**before["config"], "transfer_per_tick": "4"}
            edit = await call("edit", {**expected, "config": proposed})
            assert edit["changed"] and edit["before"] == before["config"] and edit["after"] == proposed
            await call("edit", {**expected, "config": proposed}, "stale_revision")
            current = await call("inspect")
            assert current["preview"] == {**before["preview"], "stale": True}, current
            expected["expected_revision"] = current["revision"]
            await call("edit", {**expected, "config": {**proposed, "transfer_per_tick": "0"}}, "invalid_arguments")
            await call("replay", {**expected, "ticks": "10001"}, "work_limit")
            after = await call("replay", {**expected, "ticks": "10"})
            print(json.dumps({"client": "official Python MCP SDK 2.3.0", "protocol": info.protocol_version,
                              "baseline": baseline, "edit": edit, "comparison": after}, indent=2))
            if os.environ.get("RUSTER_MCP_DEMO_KEEP") != "1":
                undone = await call("undo", expected)
                assert undone["after"] == before["config"]
                print("Shared authored undo restored original configuration; no save performed.")


if __name__ == "__main__":
    asyncio.run(main())
