#!/usr/bin/env python3
"""Focused real Chromium + native Vulkan + stdio MCP journey, not ChatGPT UI."""
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
import urllib.error
import urllib.request
from websockets.sync.client import connect

ROOT=Path(__file__).resolve().parent
CHROME=Path('/home/akilf/.hermes/tools/chromium-1208/chrome-linux64/chrome')

def wait(predicate, seconds=20):
    until=time.monotonic()+seconds
    while not predicate():
        assert time.monotonic()<until,'deadline (no mutation replay)'
        time.sleep(.02)

class CDP:
    def __init__(self,url): self.ws=connect(url,max_size=32*1024*1024); self.i=0
    def call(self,method,params=None):
        self.i+=1; self.ws.send(json.dumps({'id':self.i,'method':method,'params':params or {}}))
        while True:
            r=json.loads(self.ws.recv(timeout=25))
            if r.get('id')==self.i:
                assert 'error' not in r,r
                return r.get('result',{})
    def js(self,expr):
        r=self.call('Runtime.evaluate',{'expression':expr,'awaitPromise':True,'returnByValue':True})
        assert 'exceptionDetails' not in r,r
        return r['result'].get('value')
    def click(self,selector):
        xy=self.js("(()=>{const r=document.querySelector("+json.dumps(selector)+").getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()")
        for kind in ['mousePressed','mouseReleased']:
            self.call('Input.dispatchMouseEvent',{'type':kind,**xy,'button':'left','clickCount':1})
        wait(lambda:self.js('!busy'))
        assert self.js("outcomes.at(-1).status")=='completed',self.js('outcomes.at(-1)')
    def snapshot(self):return self.js('({scene:current.scene,view:current.view,png:current.png,draft:!!draft,native_frame:current.native_frame})')

def stop(p):
    if p and p.poll() is None:
        p.terminate()
        try:p.wait(timeout=10)
        except subprocess.TimeoutExpired:p.kill();p.wait()

def main():
    p=argparse.ArgumentParser();p.add_argument('--host-binary',type=Path,required=True);p.add_argument('--adapter',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--validation-diagnostics',action='store_true');a=p.parse_args()
    a.output.mkdir(exist_ok=False,parents=True)
    profile=Path(tempfile.mkdtemp(prefix='interactive-chromium-',dir=os.environ['TMPDIR']))
    server=chrome=adapter=c=None; run=None; record: dict[str, object]={'actual_chatgpt':False,'production_consent':False,'host_sha256':hashlib.sha256(a.host_binary.read_bytes()).hexdigest(),'adapter_sha256':hashlib.sha256(a.adapter.read_bytes()).hexdigest()}; seq=0
    try:
        with (a.output/'server.log').open('w') as log,(a.output/'chrome.log').open('w') as clog,(a.output/'mcp.log').open('w') as mlog,(a.output/'protocol.jsonl').open('w') as protocol:
            server=subprocess.Popen(['python3',str(ROOT/'server.py'),'--host-binary',str(a.host_binary),'--output-root',str(a.output),'--grant-disposable-local-and-first-mcp-owner']+(['--validation-diagnostics'] if a.validation_diagnostics else []),stdout=log,stderr=log)
            def ready():
                assert server.poll() is None,'owned server startup failed'
                return bool(list(a.output.glob('interactive-*/server.json')))
            wait(ready); infofile=next(a.output.glob('interactive-*/server.json'));run=infofile.parent;info=json.loads(infofile.read_text());token=(run/'capability.secret').read_text()
            clients=json.loads(subprocess.check_output(['hyprctl','-j','clients'],text=True))
            record['native_owned_window_count']=sum(client.get('pid')==info['native_pid'] for client in clients)
            assert record['native_owned_window_count']==0
            env={k:v for k,v in os.environ.items() if not k.startswith('RUSTER_')};env['RUSTER_MCP_LOCATOR']=info['locator']
            adapter=subprocess.Popen([str(a.adapter)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=mlog,text=True,bufsize=1,env=env)
            def rpc(method,params=None,notify=False):
                nonlocal seq
                assert adapter is not None and adapter.stdin is not None and adapter.stdout is not None
                seq+=1;req={'jsonrpc':'2.0','method':method}
                if not notify:req['id']=seq
                if params is not None:req['params']=params
                protocol.write(json.dumps({'request':req})+'\n');protocol.flush();adapter.stdin.write(json.dumps(req)+'\n');adapter.stdin.flush()
                if notify:return
                assert select.select([adapter.stdout],[],[],10)[0],'MCP deadline; no retry'
                reply=json.loads(adapter.stdout.readline());protocol.write(json.dumps({'reply':reply})+'\n');protocol.flush();return reply
            def tool(name,args=None):
                reply=rpc('tools/call',{'name':name,'arguments':args or {}})
                assert reply is not None
                return reply['result']['structuredContent']
            rpc('initialize',{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'interactive-localhost-qualification-NOT-ChatGPT','version':'1'}});rpc('notifications/initialized',notify=True)
            assert tool('scene_inspect')['error']['code']=='approval_required'
            wait(lambda:not (run/'approve.json').exists() and json.loads((run/'state.json').read_text())['owner'] is not None)
            time.sleep(.3)
            assert tool('scene_inspect')['ok']
            chrome=subprocess.Popen([str(CHROME),'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--disable-background-networking','--user-data-dir='+str(profile),info['url']],stdout=clog,stderr=clog)
            wait(lambda:(profile/'DevToolsActivePort').exists());port=(profile/'DevToolsActivePort').read_text().splitlines()[0]
            targets=json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json'));target=next(t for t in targets if t.get('url')==info['url']);c=CDP(target['webSocketDebuggerUrl'])
            c.call('Emulation.setDeviceMetricsOverride',{'width':1250,'height':1000,'deviceScaleFactor':1,'mobile':False});c.call('Page.bringToFront');wait(lambda:c.js("typeof operate==='function'"))
            c.js("document.querySelector('#capability').value="+json.dumps(token));c.click('#connect');base=c.snapshot()
            assert base['scene']['session_token']==tool('scene_inspect')['result']['session_token']
            for action in ['right','panright','in']:c.click('[data-camera="'+action+'"]')
            navigated=c.snapshot();assert navigated['view']['camera']!=base['view']['camera'];assert tool('viewport_inspect')['result']['camera']==navigated['view']['camera']
            c.click('[data-object="center"]');assert tool('viewport_inspect')['result']['active_object_id']=='center'
            c.click('#clear');assert tool('viewport_inspect')['result']['selected_object_ids']==[]
            c.click('[data-object="center"]');original=c.snapshot()
            c.click('#begin');assert tool('scene_inspect')['result']['drafts_pending']
            guarded={'expected_session_token':original['scene']['session_token'],'expected_revision':original['scene']['revision'],'object_id':'center','transform':original['scene']['objects'][1]['transform']}
            refused=tool('scene_transform',guarded);assert refused['error']['code']=='drafts_pending',refused
            c.js("document.querySelector('#translation0').value='1.25'")
            c.click('#connect');assert c.js("document.querySelector('#translation0').value")=='1.25'
            # Actual keyboard Enter submission, not invoking the JS authoring function.
            c.js("document.querySelector('#translation0').focus()")
            c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Enter','code':'Enter','windowsVirtualKeyCode':13});c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Enter','code':'Enter','windowsVirtualKeyCode':13});wait(lambda:c.js('!busy && !draft'))
            manual=c.snapshot();assert manual['scene']['revision']=='1';assert tool('scene_inspect')['result']['objects']==manual['scene']['objects'];assert manual['png']!=original['png']
            guarded['expected_revision']='1';guarded['transform']=json.loads(json.dumps(manual['scene']['objects'][1]['transform']));guarded['transform']['translation'][0]=2.5
            changed=tool('scene_transform',guarded);assert changed['ok'],changed
            c.click('#connect');agent=c.snapshot();assert agent['scene']['revision']=='2';assert agent['scene']['objects'][1]['transform']['translation'][0]==2.5;assert agent['view']['active_object_id']=='center';assert agent['png']!=manual['png']
            c.click('#undo');undone=c.snapshot();assert undone['scene']['revision']=='3';assert undone['scene']['objects']==manual['scene']['objects'];assert undone['png']==manual['png']
            assert tool('scene_undo',{'expected_session_token':agent['scene']['session_token'],'expected_revision':'3'})['ok']
            c.click('#connect');restored=c.snapshot();assert restored['scene']['revision']=='4';assert restored['scene']['objects']==original['scene']['objects'];assert restored['png']==original['png']
            # Stale local writes are not silently rebased; native draft still blocks agent.
            c.click('#begin');c.js("document.querySelector('#translation0').value='7'")
            stale=c.js("operate('scene_transform',{...pins(false,draft.source),expected_revision:'0',object_id:'center',transform:current.scene.objects[1].transform})")
            assert stale['status']=='failed';assert c.js("document.querySelector('#translation0').value")=='7';assert tool('scene_inspect')['result']['drafts_pending']
            c.js("document.querySelector('#translation0').focus()");c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Escape','code':'Escape','windowsVirtualKeyCode':27});c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Escape','code':'Escape','windowsVirtualKeyCode':27});wait(lambda:c.js('!busy && !draft'))
            assert not tool('scene_inspect')['result']['drafts_pending']
            negatives=[]
            for override in [{'Origin':'http://evil.invalid'},{'X-Workspace-Capability':'wrong'},{'Host':'evil.invalid'}]:
                headers={'Origin':info['url'].rstrip('/'),'Content-Type':'application/json','X-Workspace-Capability':token,**override}
                try:urllib.request.urlopen(urllib.request.Request(info['url']+'operation',data=b'{"op":"refresh","args":{}}',headers=headers));raise AssertionError('boundary accepted')
                except urllib.error.HTTPError as e:assert e.code==403;negatives.append(override.keys().__iter__().__next__())
            denied=c.js("operate('scene_save',{})");assert denied['status']=='failed' and denied['error']=='Error: forbidden_operation'
            burst=c.js("Promise.all(Array.from({length:80},()=>operate('refresh')))");assert sum(r['status']=='completed' for r in burst)==1;assert sum(r['status']=='busy-refused' for r in burst)==79;assert c.js('outcomes.length')<=64
            preserved_samples=c.js('samples')
            c.click('#begin');c.call('Page.reload');wait(lambda:c.js("typeof operate==='function' && current===null"));c.js("document.querySelector('#capability').value="+json.dumps(token));c.click('#connect')
            assert c.js('current.draft_reserved && !draft && !document.querySelector("#cancel").disabled')
            assert tool('scene_inspect')['result']['drafts_pending'];c.click('#cancel');assert not tool('scene_inspect')['result']['drafts_pending']
            final=c.snapshot();assert final['scene']['objects']==original['scene']['objects']
            (a.output/'workspace.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            for name,snapshot in [('original',original),('manual',manual),('agent',agent),('undo-manual',undone),('restored',restored)]:
                (a.output/(name+'.png')).write_bytes(base64.b64decode(snapshot.pop('png')));(a.output/(name+'.json')).write_text(json.dumps(snapshot,indent=2))
            record.update(samples=preserved_samples+c.js('samples'),negatives=negatives,revisions=['0','1','2','3','4'],same_object='center',native_pid=info['native_pid'],url=info['url'])
            (run/'revoke').touch();wait(lambda:not (run/'revoke').exists());assert not tool('scene_inspect')['ok']
            revoked=c.js("operate('refresh')");assert revoked['status']=='failed' and revoked['error']=='Error: revoked'
            record['passed']=True
    except Exception as exc:
        if c:
            try:
                (a.output/'browser-failure.json').write_text(json.dumps(c.js("({status:document.querySelector('#status').textContent,outcomes,busy,draft,current:current?{scene:current.scene,view:current.view}:null,valid:document.querySelector('#inspector').checkValidity(),inputs:[...document.querySelectorAll('input[type=number]')].map(i=>({id:i.id,value:i.value,valid:i.validity.valid})),active:document.activeElement?.id})"),indent=2))
                (a.output/'failure.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            except Exception as capture_error: record['failure_capture_error']=repr(capture_error)
        record.update(passed=False,error=repr(exc));raise
    finally:
        if c:c.ws.close()
        stop(chrome)
        if adapter:
            adapter.stdin.close()
            try:adapter.wait(timeout=6)
            except subprocess.TimeoutExpired:stop(adapter)
        stop(server);shutil.rmtree(profile)
        record['cleanup']={'server_stopped':server is None or server.poll() is not None,'chrome_stopped':chrome is None or chrome.poll() is not None,'adapter_stopped':adapter is None or adapter.poll() is not None,'profile_absent':not profile.exists()}
        if run and (run/'server-cleanup.json').exists():record['cleanup'].update(json.loads((run/'server-cleanup.json').read_text()))
        with open('/home/akilf/.hermes/qa/rusterize/state/gate.lock','a') as gate:
            fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB);record['cleanup']['gate_reacquired']=True
        (a.output/'result.json').write_text(json.dumps(record,indent=2));print(json.dumps({k:v for k,v in record.items() if k!='samples'},indent=2))

if __name__=='__main__':main()
