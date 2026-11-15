#!/usr/bin/env python3
"""Owned Chromium pointer -> genuine native gizmo/preview/history -> same-owner MCP."""
import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile
import time
import urllib.request
from qualify import CDP, CHROME, ROOT, wait, stop


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--host-binary',type=Path,required=True)
    p.add_argument('--adapter',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--probe',action='store_true')
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    profile=Path(tempfile.mkdtemp(prefix='translate-chromium-',dir=os.environ['TMPDIR']))
    server=chrome=adapter=c=None;run=None;seq=0
    result={'passed':False,'actual_chatgpt':False,'host_sha256':hashlib.sha256(a.host_binary.read_bytes()).hexdigest()}
    try:
        with (a.output/'server.log').open('w') as sl,(a.output/'chrome.log').open('w') as cl,(a.output/'mcp.log').open('w') as ml,(a.output/'protocol.jsonl').open('w') as protocol:
            server=subprocess.Popen(['python3',str(ROOT/'server.py'),'--host-binary',str(a.host_binary),'--output-root',str(a.output),'--grant-disposable-local-and-first-mcp-owner','--validation-diagnostics'],stdout=sl,stderr=sl)
            def ready():
                assert server.poll() is None,'server stopped'
                return bool(list(a.output.glob('interactive-*/server.json')))
            wait(ready);run=next(a.output.glob('interactive-*/server.json')).parent;info=json.loads((run/'server.json').read_text())
            result['owner']=info
            env={k:v for k,v in os.environ.items() if not k.startswith('RUSTER_')};env['RUSTER_MCP_LOCATOR']=info['locator']
            adapter=subprocess.Popen([str(a.adapter)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=ml,text=True,bufsize=1,env=env)
            def rpc(method,params=None,notify=False):
                nonlocal seq
                assert adapter and adapter.stdin and adapter.stdout
                seq+=1;req={'jsonrpc':'2.0','method':method}
                if not notify:req['id']=seq
                if params is not None:req['params']=params
                protocol.write(json.dumps({'request':req})+'\n');protocol.flush();adapter.stdin.write(json.dumps(req)+'\n');adapter.stdin.flush()
                if notify:return
                assert select.select([adapter.stdout],[],[],10)[0],'MCP deadline'
                reply=json.loads(adapter.stdout.readline());protocol.write(json.dumps({'reply':reply})+'\n');protocol.flush();return reply
            def tool(name,args=None):
                reply=rpc('tools/call',{'name':name,'arguments':args or {}})
                assert reply is not None
                return reply['result']['structuredContent']
            rpc('initialize',{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'translate-qualification-not-chatgpt','version':'1'}});rpc('notifications/initialized',notify=True)
            assert tool('scene_inspect')['error']['code']=='approval_required';time.sleep(.3);assert tool('scene_inspect')['ok']
            chrome=subprocess.Popen([str(CHROME),'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--disable-background-networking','--user-data-dir='+str(profile),info['url']],stdout=cl,stderr=cl)
            wait(lambda:(profile/'DevToolsActivePort').exists());port=(profile/'DevToolsActivePort').read_text().splitlines()[0]
            target=next(t for t in json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json')) if t.get('url')==info['url']);c=CDP(target['webSocketDebuggerUrl'])
            c.call('Emulation.setDeviceMetricsOverride',{'width':1250,'height':1000,'deviceScaleFactor':1,'mobile':False});c.call('Page.bringToFront');wait(lambda:c.js("typeof operate==='function'"))
            c.js("$('capability').value="+json.dumps((run/'capability.secret').read_text()));c.click('#connect');c.click('[data-object="center"]')
            c.js("$('translate').checked=true")
            c.js("window.admission={active:0,max:0};window.originalFetch=fetch;window.fetch=async(...args)=>{admission.active++;admission.max=Math.max(admission.max,admission.active);try{if(window.delayPreview&&args[1]?.body&&JSON.parse(args[1].body).op==='translate_move')await new Promise(r=>setTimeout(r,150));return await originalFetch(...args)}finally{admission.active--}}")
            def save(name):
                snap=c.js('current');(a.output/(name+'.png')).write_bytes(base64.b64decode(snap.pop('png')));(a.output/(name+'.json')).write_text(json.dumps(snap,indent=2));return snap
            before=save('before')
            (a.output/'workspace.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            if a.probe:result['probe_only']=True;return
            # Pixel coordinate chosen from the actually inspected native handle image.
            x,y=HANDLE
            def mouse(kind,x,y,buttons=1):
                xy=c.js("(()=>{const r=frame.getBoundingClientRect();return {x:r.left+frame.clientLeft+frame.clientWidth*"+str(x)+"/frame.naturalWidth,y:r.top+frame.clientTop+frame.clientHeight*"+str(y)+"/frame.naturalHeight}})()")
                c.call('Input.dispatchMouseEvent',{'type':kind,**xy,'button':'left','buttons':buttons,'clickCount':1})
            mouse('mouseMoved',x,y,0);mouse('mousePressed',x,y)
            wait(lambda:c.js('!busy && translation && !translation.flight'))
            assert c.js('current.translate_active'),c.js('outcomes')
            wrong=c.js("operate('translate_move',{gesture_id:'wrong',x:477,y:290,image_width:frame.naturalWidth,image_height:frame.naturalHeight})")
            assert wrong['status']=='failed' and c.js('current.translate_active')
            pressed=save('pressed');assert pressed['scene']['revision']==before['scene']['revision']
            for i in range(1,7):
                mouse('mouseMoved',x+8*i,y);time.sleep(.035)
            wait(lambda:c.js('!busy && translation && !translation.flight && !translation.latest'))
            drag=save('drag');assert drag['scene']['objects']==before['scene']['objects'];assert drag['scene']['revision']==before['scene']['revision']
            assert drag['undo_len']==before['undo_len']==0
            assert drag['preview_objects']!=before['scene']['objects']
            assert (a.output/'drag.png').read_bytes()!=(a.output/'before.png').read_bytes(),'no native preview pixels'
            observed=tool('scene_inspect');assert observed['ok'] and observed['result']['objects']==before['scene']['objects']
            blocked=tool('scene_transform',{'expected_session_token':before['view']['session_token'],'expected_revision':before['scene']['revision'],'object_id':'center','transform':before['scene']['objects'][1]['transform']})
            assert blocked['error']['code']=='drafts_pending',blocked
            mouse('mouseReleased',x+72,y,0);wait(lambda:c.js('!busy && !translation'))
            released=save('released');assert released['scene']['objects']!=before['scene']['objects'];assert int(released['scene']['revision'])==int(before['scene']['revision'])+1
            assert released['undo_len']==1 and released['redo_len']==0
            def center(objects):return next(o for o in objects if o['id']=='center')['transform']
            assert center(released['scene']['objects'])['translation'][0]>center(drag['preview_objects'])['translation'][0],'last release point not settled'
            assert tool('scene_inspect')['result']['objects']==released['scene']['objects']
            c.click('#undo');undone=save('undo');assert undone['scene']['objects']==before['scene']['objects']
            assert (a.output/'undo.png').read_bytes()==(a.output/'before.png').read_bytes(),'Undo native pixels mismatch'
            assert undone['undo_len']==0 and undone['redo_len']==1
            # Same native handle through a differently sized CSS viewport at DPR2.
            c.call('Emulation.setDeviceMetricsOverride',{'width':980,'height':1100,'deviceScaleFactor':2,'mobile':False})
            mouse('mouseMoved',x,y,0);mouse('mousePressed',x,y);wait(lambda:c.js('!busy && translation && !translation.flight'))
            mouse('mouseMoved',x+36,y);wait(lambda:c.js('!busy && translation && !translation.flight && !translation.latest'))
            # Release during an in-flight preview; post-release hover must not edit.
            c.js('window.delayPreview=true')
            mouse('mouseMoved',x+40,y);mouse('mouseReleased',x+72,y,0);mouse('mouseMoved',x+120,y,0)
            wait(lambda:c.js('!busy && !translation'));c.js('window.delayPreview=false')
            dpr2=save('dpr2-release');assert dpr2['undo_len']==1
            assert abs(center(dpr2['scene']['objects'])['translation'][0]-center(released['scene']['objects'])['translation'][0])<.02,'post-release hover changed terminal intent'
            mcp_undo=tool('scene_undo',{'expected_session_token':dpr2['view']['session_token'],'expected_revision':dpr2['scene']['revision']});assert mcp_undo['ok'],mcp_undo
            c.click('#connect');mcp_restored=save('mcp-undo');assert mcp_restored['scene']['objects']==before['scene']['objects'] and mcp_restored['undo_len']==0
            assert (a.output/'mcp-undo.png').read_bytes()==(a.output/'before.png').read_bytes()
            cancels=[]
            for mode in ['escape','pointercancel','lostcapture','lease','offline']:
                initial=c.js('current');mouse('mouseMoved',x,y,0);mouse('mousePressed',x,y);wait(lambda:c.js('!busy && translation && !translation.flight'))
                mouse('mouseMoved',x+24,y);wait(lambda:c.js('!busy && translation && !translation.flight && !translation.latest'))
                assert c.js('current.translate_active && current.preview_objects!==null')
                if mode=='escape':
                    c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Escape','code':'Escape','windowsVirtualKeyCode':27});c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Escape','code':'Escape','windowsVirtualKeyCode':27})
                elif mode=='pointercancel':c.js("frame.dispatchEvent(new PointerEvent('pointercancel',{pointerId:translation.pointer}))")
                elif mode=='lostcapture':c.js('frame.releasePointerCapture(translation.pointer)')
                elif mode=='offline':
                    c.call('Network.enable');c.call('Network.emulateNetworkConditions',{'offline':True,'latency':0,'downloadThroughput':0,'uploadThroughput':0})
                    time.sleep(5.4)
                    c.call('Network.emulateNetworkConditions',{'offline':False,'latency':0,'downloadThroughput':-1,'uploadThroughput':-1})
                    wait(lambda:c.js('!busy && !translation'));c.click('#connect')
                else:
                    # Abandoned browser gesture: no heartbeat/input. Native lease cancels.
                    time.sleep(5.4);c.js('cancelTranslation()')
                wait(lambda:c.js('!busy && !translation'));mouse('mouseReleased',x+24,y,0)
                now=save('cancel-'+mode)
                assert now['scene']['objects']==initial['scene']['objects'] and now['scene']['revision']==initial['scene']['revision'] and now['undo_len']==initial['undo_len'] and now['redo_len']==initial['redo_len']
                assert now['view']['selected_object_ids']==['center'] and not now['translate_active']
                assert (a.output/('cancel-'+mode+'.png')).read_bytes()==(a.output/'before.png').read_bytes()
                cancels.append({'mode':mode,'revision':now['scene']['revision'],'undo_len':now['undo_len']})
            # Stale pins and pending numeric draft never take native pointer ownership.
            stale=[]
            for field,value in [('expected_session_token','wrong'),('expected_revision','0'),('expected_view_revision','0'),('expected_native_frame',0),('image_width',0)]:
                r=c.js("operate('translate_begin',{...pins(true),expected_native_frame:current.native_frame,gesture_id:'stale',x:477,y:290,image_width:frame.naturalWidth,image_height:frame.naturalHeight,"+json.dumps(field)+":"+json.dumps(value)+"})")
                assert r['status']=='failed',r;stale.append(field)
            c.click('#begin');r=c.js("operate('translate_begin',{...pins(true),expected_native_frame:current.native_frame,gesture_id:'draft',x:477,y:290,image_width:frame.naturalWidth,image_height:frame.naturalHeight})")
            assert r['status']=='failed' and tool('scene_inspect')['result']['drafts_pending'];c.click('#cancel')
            # A miss cannot select scene content behind the handle or latch a press.
            mouse('mouseMoved',20,20,0);mouse('mousePressed',20,20);mouse('mouseReleased',20,20,0);wait(lambda:c.js('!busy && !translation'))
            assert c.js("current.view.active_object_id==='center' && !current.translate_active")
            # Revoke while previewing: cancellation, never a release commit.
            mouse('mouseMoved',x,y,0);mouse('mousePressed',x,y);wait(lambda:c.js('!busy && translation && !translation.flight'))
            mouse('mouseMoved',x+24,y);wait(lambda:c.js('!busy && translation && !translation.flight && !translation.latest'))
            final_before_revoke=c.js('current');result['revoke_expected_revision']=final_before_revoke['scene']['revision']
            (run/'revoke').touch();wait(lambda:not (run/'revoke').exists());assert not tool('scene_inspect')['ok']
            c.js('cancelTranslation()');wait(lambda:c.js('!busy && !translation'))
            assert c.js('admission.max')==1
            result['admission_max']=c.js('admission.max')
            result.update(cancels=cancels,stale=stale,mcp_undo=mcp_undo,dpr2_undo_len=dpr2['undo_len'])
            result.update(passed=True,before=before['scene'],drag=drag['scene'],released=released['scene'],undo=undone['scene'],blocked=blocked)
    except Exception as exc:
        result['error']=repr(exc)
        if c:
            try:
                (a.output/'failure.json').write_text(json.dumps(c.js('({outcomes,translation,current})'),indent=2))
                (a.output/'failure.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            except Exception:pass
        raise
    finally:
        if c:c.ws.close()
        stop(chrome)
        if adapter:
            assert adapter.stdin
            adapter.stdin.close()
            try:adapter.wait(timeout=6)
            except subprocess.TimeoutExpired:stop(adapter)
        stop(server);shutil.rmtree(profile)
        result['cleanup']={'server_stopped':server is None or server.poll() is not None,'chrome_stopped':chrome is None or chrome.poll() is not None,'adapter_stopped':adapter is None or adapter.poll() is not None,'profile_absent':not profile.exists()}
        if run and (run/'server-cleanup.json').exists():result['cleanup'].update(json.loads((run/'server-cleanup.json').read_text()))
        if run and (run/'cleanup.json').exists():
            native_cleanup=json.loads((run/'cleanup.json').read_text());result['native_cleanup']=native_cleanup
            if result.get('passed'):
                assert not native_cleanup['translate_active'] and native_cleanup['undo_len']==0
                assert str(native_cleanup['revision'])==result['revoke_expected_revision']
                assert native_cleanup['objects']==result['before']['objects']
        try:
            with open('/home/akilf/.hermes/qa/rusterize/state/gate.lock','a') as gate:fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB);result['cleanup']['gate_reacquired']=True
        except BlockingIOError:result['cleanup']['gate_reacquired']=False
        (a.output/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k not in ['before','drag','released','undo']},indent=2))

HANDLE=(477,290)
if __name__=='__main__':main()
