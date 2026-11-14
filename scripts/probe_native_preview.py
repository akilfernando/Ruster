#!/usr/bin/env python3
"""Developer-only GPU/authority/PNG qualification. NOT actual ChatGPT or human consent.
Build the opt-in editor test fixture first; this runs only owned disposable state.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time

def qualify(host_binary, adapter, output_root):
    run=Path(tempfile.mkdtemp(prefix="native-preview-",dir=output_root))
    env={k:v for k,v in os.environ.items() if not k.startswith(("RUSTER_","VK_","EGUI_INSPECTION"))}
    env.update(PREVIEW_FIXTURE_DIR=str(run),RUSTER_RECOVERY_DIR=str(run/"recovery"),RUSTER_EDITOR_STATE=str(run/"editor-state.json"))
    protocol=run/"protocol.jsonl"
    result={"actual_chatgpt":False,"human_consent":False,"fixture_authority_only":True,"run":str(run),"host_sha256":hashlib.sha256(host_binary.read_bytes()).hexdigest(),"adapter_sha256":hashlib.sha256(adapter.read_bytes()).hexdigest()}
    host=None
    mcp=None
    seq=0
    try:
        with (run/"host.log").open("w") as log, protocol.open("w") as journal, (run/"adapter.log").open("w") as err:
            host=subprocess.Popen([str(host_binary),"--ignored","--exact","scene_editor::preview_probe::direct_image_host_fixture","--nocapture"],cwd=run,env=env,stdout=log,stderr=subprocess.STDOUT)
            deadline=time.monotonic()+15
            while not (run/"state.json").exists():
                assert host.poll() is None and time.monotonic()<deadline,"host startup failed"
                time.sleep(.03)
            state=json.loads((run/"state.json").read_text())
            env["RUSTER_MCP_LOCATOR"]=state["locator"]
            mcp=subprocess.Popen([str(adapter)],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,bufsize=1)
            assert mcp.stdin is not None and mcp.stdout is not None
            def rpc(method,params=None,notify=False):
                nonlocal seq
                assert mcp is not None and mcp.stdin is not None and mcp.stdout is not None
                seq+=1
                req={"jsonrpc":"2.0","method":method}
                if not notify: req["id"]=seq
                if params is not None:req["params"]=params
                line=json.dumps(req)
                journal.write(json.dumps({"direction":"request","raw":line})+"\n");journal.flush()
                mcp.stdin.write(line+"\n");mcp.stdin.flush()
                if notify:return None
                assert select.select([mcp.stdout],[],[],8)[0],"MCP response deadline; do NOT replay"
                raw=mcp.stdout.readline()
                journal.write(json.dumps({"direction":"response","raw":raw})+"\n");journal.flush()
                return json.loads(raw)
            def tool(name,args=None):
                reply=rpc("tools/call",{"name":name,"arguments":args or {}})
                assert reply is not None
                return reply["result"]
            def data(reply):return reply["structuredContent"]
            def pins(value,view=False):
                v=value["result"]
                return {"expected_session_token":v["session_token"],"expected_revision":v["revision"],**({"expected_view_revision":v["view_revision"]} if view else {})}
            def capture(label):
                v=data(tool("viewport_inspect"))
                started=data(tool("viewport_capture_start",pins(v,True)))
                assert started["ok"],started
                cid=started["result"]["capture_id"]
                deadline=time.monotonic()+6
                while True:
                    reply=tool("viewport_capture_inspect",{"capture_id":cid})
                    v=data(reply)
                    if v.get("result",{}).get("status")!="pending":break
                    assert time.monotonic()<deadline,"capture deadline; no restart"
                    time.sleep(.05)
                assert v["ok"] and v["result"]["status"]=="ready",v
                images=[c for c in reply["content"] if c["type"]=="image"]
                assert len(images)==1 and images[0]["mimeType"]=="image/png"
                png=base64.b64decode(images[0]["data"],validate=True)
                assert png.startswith(b"\x89PNG\r\n\x1a\n") and len(png)<=40000
                assert "image_base64" not in json.dumps(v)
                (run/(label+".json")).write_text(json.dumps(reply,indent=2))
                (run/(label+".png")).write_bytes(png)
                return v["result"],png
            rpc("initialize",{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"native-preview-qualification-NOT-ChatGPT","version":"1"}})
            rpc("notifications/initialized",notify=True)
            (run/"tools.json").write_text(json.dumps(rpc("tools/list"),indent=2))
            refusal=data(tool("scene_inspect"))
            assert refusal["error"]["code"]=="approval_required"
            deadline=time.monotonic()+3
            while True:
                state=json.loads((run/"state.json").read_text())
                if state["owner"]:break
                assert time.monotonic()<deadline
                time.sleep(.02)
            # Explicit test-fixture grant, never exposed as a tool or client consent.
            (run/"approve.json").write_text(json.dumps(state["owner"]))
            deadline=time.monotonic()+3
            while (run/"approve.json").exists():
                assert host.poll() is None and time.monotonic()<deadline,"fixture grant deadline"
                time.sleep(.01)
            time.sleep(.2)
            base=data(tool("scene_inspect"));assert base["ok"] and base["result"]["objects"]==[]
            before,beforepng=capture("before")
            created=data(tool("scene_create",{**pins(base),"object_id":"preview-sphere","name":"Sphere","primitive":"sphere"}))
            assert created["ok"],created
            post=data(tool("scene_inspect"))
            assert post["result"]["object_count"]==1 and post["result"]["objects"][0]["primitive"]=="sphere"
            stale=tool("viewport_capture_inspect",{"capture_id":before["capture_id"]})
            assert data(stale)["error"]["code"]=="stale_capture" and not any(c["type"]=="image" for c in stale["content"])
            sphere,spherepng=capture("sphere")
            undone=data(tool("scene_undo",pins(post)));assert undone["ok"],undone
            restored=data(tool("scene_inspect"))
            assert restored["result"]["objects"]==base["result"]["objects"]
            undo,undopng=capture("undo")
            assert before["camera"]==sphere["camera"]==undo["camera"]
            assert before["session_token"]==sphere["session_token"]==undo["session_token"]
            assert [before["revision"],sphere["revision"],undo["revision"]]==["0","1","2"]
            assert beforepng==undopng and beforepng!=spherepng,"matched authored restoration must restore viewport bytes"
            (run/"revoke").touch()
            deadline=time.monotonic()+3
            while (run/"revoke").exists():
                assert host.poll() is None and time.monotonic()<deadline,"fixture revocation deadline"
                time.sleep(.01)
            denied=tool("viewport_capture_inspect",{"capture_id":undo["capture_id"]})
            assert denied["isError"] and not any(c["type"]=="image" for c in denied["content"])
            fresh=tool("viewport_capture_start",pins(restored)|{"expected_view_revision":undo["view_revision"]})
            assert fresh["isError"] and not any(c["type"]=="image" for c in fresh["content"])
            result.update(passed=True,revisions=["0","1","2"],camera=before["camera"],session_token=before["session_token"],png_sha256={name:hashlib.sha256(png).hexdigest() for name,png in [("before",beforepng),("sphere",spherepng),("undo",undopng)]},post_revoke_error=data(denied)["error"]["code"])
    except Exception as exc:
        result.update(passed=False,error=repr(exc))
        raise
    finally:
        (run/"stop").touch()
        if mcp:
            assert mcp.stdin is not None
            try:mcp.stdin.close()
            except OSError:pass
            try:mcp.wait(timeout=5)
            except subprocess.TimeoutExpired:
                mcp.terminate()
                try:mcp.wait(timeout=5)
                except subprocess.TimeoutExpired:mcp.kill();mcp.wait(timeout=5)
        if host:
            try:host.wait(timeout=8)
            except subprocess.TimeoutExpired:
                host.terminate()
                try:host.wait(timeout=5)
                except subprocess.TimeoutExpired:host.kill();host.wait(timeout=5)
        result.update(host_exit=host.returncode if host else None,adapter_exit=mcp.returncode if mcp else None)
        try:
            cleanup=json.loads((run/"cleanup.json").read_text())
            assert cleanup["retired"] and cleanup["locator_absent"] and cleanup["objects"]==[]
            assert result["host_exit"]==0 and result["adapter_exit"]==0
            result["cleanup_verified"]=True
        except Exception as exc:
            result.update(passed=False,cleanup_verified=False,cleanup_error=repr(exc))
        (run/"result.json").write_text(json.dumps(result,indent=2))
        print(json.dumps(result,indent=2))
    assert result.get("passed") and result["cleanup_verified"],result
    return run

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host-binary",type=Path,required=True)
    p.add_argument("--adapter",type=Path,required=True)
    p.add_argument("--output-root",type=Path,required=True)
    a=p.parse_args()
    for path in (a.host_binary,a.adapter,a.output_root):assert path.is_absolute()
    qualify(a.host_binary,a.adapter,a.output_root)
