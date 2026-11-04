#!/usr/bin/env python3
"""Official MCP SDK behavior-data smoke on a disposable enabled scene editor.
Run: uv run --with mcp==2.3.0 scripts/mcp_behavior_demo.py
Requires RUSTER_MCP_SOCKET, clean human drafts and one unattached object. Does not
save or run scripts. Three shared Undo operations restore authored data on success.
A failed/timeout assertion leaves reviewable unsaved work: inspect before retrying.
"""
import asyncio
import base64
import hashlib
import json
import os
from pathlib import Path
import uuid

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/ruster-mcp"
    params = StdioServerParameters(command=str(binary), env={**os.environ})
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as client:
            info = await client.initialize()
            names = {tool.name for tool in (await client.list_tools()).tools}
            assert {"behavior_inspect", "behavior_source", "behavior_source_set",
                    "behavior_create", "behavior_binding", "behavior_property",
                    "behavior_property_set", "scene_undo"} <= names

            async def call(name, arguments=None, error=None):
                response = await client.call_tool(name, arguments or {})
                value = response.structured_content
                assert value is not None and response.is_error == (error is not None), response
                if error:
                    assert value["error"]["code"] == error, value
                    return value
                assert value["ok"], value
                return value["result"]

            before = await call("scene_inspect")
            assert not before["drafts_pending"] and not before["recovery_pending"], before
            token, revision = before["session_token"], before["revision"]

            async def pages(kind):
                items, offset = [], "0"
                while True:
                    result = await call("behavior_inspect", {"kind": kind, "offset": offset})
                    assert (result["session_token"], result["revision"]) == (token, revision)
                    items.extend(result["page"]["items"])
                    offset = result["page"]["next_offset"]
                    if offset is None:
                        return items
                    offset = str(offset)

            bindings = await pages("bindings")
            sources = await pages("sources")
            owners = {binding["object_id"] for binding in bindings}
            owner = next((obj["id"] for obj in before["objects"] if obj["id"] not in owners), None)
            assert owner is not None, "Fixture needs an unattached object in its first16-object page"
            source_id, binding_id = "wire-" + uuid.uuid4().hex, "wire-" + uuid.uuid4().hex
            expected = {"expected_session_token": token, "expected_revision": revision}
            body = b"return {\n" + b"\0" * (16384 - len(b"return {\n"))
            encoded = base64.b64encode(body).decode("ascii")
            create = {**expected, "source_id": source_id, "binding_id": binding_id,
                      "object_id": owner, "label": "Protocol smoke", "language_id": "lua54",
                      "host_api_version": 1, "text_base64": encoded}
            created = await call("behavior_create", create)
            await call("behavior_create", create, "stale_revision")
            expected["expected_revision"] = created["revision"]
            source = await call("behavior_source", {**expected, "source_id": source_id})
            assert base64.b64decode(source["text_base64"], validate=True) == body
            assert source["text_sha256"] == hashlib.sha256(body).hexdigest()
            text = "\0" * 1024
            changed = await call("behavior_property_set", {**expected, "binding_id": binding_id,
                                "name": "notes", "value": {"type": "text", "value": text}})
            expected["expected_revision"] = changed["revision"]
            property_value = await call("behavior_property", {**expected, "binding_id": binding_id, "name": "notes"})
            assert property_value["value"] == {"type": "text", "value": text}
            invalid = {**create, **expected, "text_base64": "/w=="}
            invalid.pop("binding_id")
            invalid.pop("object_id")
            await call("behavior_source_set", invalid, "invalid_arguments")
            replacement = {**invalid, "text_base64": base64.b64encode(body[:-1] + b"x").decode("ascii")}
            changed = await call("behavior_source_set", replacement)
            expected["expected_revision"] = changed["revision"]
            for _ in range(3):
                undone = await call("scene_undo", expected)
                expected["expected_revision"] = undone["revision"]
            revision = expected["expected_revision"]
            assert await pages("sources") == sources
            assert await pages("bindings") == bindings
            after = await call("scene_inspect")
            assert after["objects"] == before["objects"]
            print(json.dumps({"client": "official Python MCP SDK 2.3.0", "protocol": info.protocol_version,
                              "tools": len(names), "source_bytes": len(body), "source_sha256": source["text_sha256"],
                              "property_bytes": len(text), "final_revision": revision,
                              "authored_restored_by_three_undo": True, "save_performed": False}, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
