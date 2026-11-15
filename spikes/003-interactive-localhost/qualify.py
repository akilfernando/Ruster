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
        wait(lambda:self.js("!busy"))
        xy=self.js("(()=>{const r=document.querySelector("+json.dumps(selector)+").getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()")
        for kind in ['mousePressed','mouseReleased']:
            self.call('Input.dispatchMouseEvent',{'type':kind,**xy,'button':'left','clickCount':1})
        wait(lambda:self.js('!busy'))
        assert self.js("outcomes.at(-1).status")=='completed',self.js('outcomes.at(-1)')
    def point(self,x,y):
        wait(lambda:self.js("!busy"))
        xy=self.js("(()=>{const i=document.querySelector('#frame'),r=i.getBoundingClientRect();return {x:r.left+i.clientLeft+i.clientWidth*"+str(x)+",y:r.top+i.clientTop+i.clientHeight*"+str(y)+"}})()")
        for kind in ['mouseMoved','mousePressed','mouseReleased']:
            self.call('Input.dispatchMouseEvent',{'type':kind,**xy,'button':'left','clickCount':1})
        wait(lambda:self.js('!busy'))
        return self.js('outcomes.at(-1)')
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
            c.js("window.observation={active:0,max:0,status:0,frames:0};window.realFetch=fetch;window.fetch=async(...args)=>{const o=observation;o.active++;o.max=Math.max(o.max,o.active);if(args[0]==='/status')o.status++;else o.frames++;try{if(args[0]==='/status'&&window.injectStatusFailure){await new Promise(r=>setTimeout(r,1200));return new Response('{}',{status:503})}return await realFetch(...args)}finally{o.active--}}")
            c.js("document.querySelector('#capability').value="+json.dumps(token));c.click('#connect');base=c.snapshot()
            time.sleep(2.6)
            idle=c.js('({...observation})');assert idle['status']>=2 and idle['frames']==1 and idle['max']==1,idle
            record['idle_observation']=idle
            assert base['scene']['session_token']==tool('scene_inspect')['result']['session_token']
            # Exercise the full native ColorImage callback path as well as local PNGs.
            capture=tool('viewport_capture_start',{'expected_session_token':base['scene']['session_token'],'expected_revision':base['scene']['revision'],'expected_view_revision':base['view']['view_revision']})
            assert capture['ok'],capture
            capture_id=capture['result']['capture_id']
            def captured():
                envelope=rpc('tools/call',{'name':'viewport_capture_inspect','arguments':{'capture_id':capture_id}})
                assert envelope is not None
                reply=envelope['result']['structuredContent']
                assert reply['ok'],reply
                if reply['result']['status']=='pending':return None
                assert reply['result']['status']=='ready',reply
                images=[item for item in envelope['result']['content'] if item['type']=='image']
                assert len(images)==1 and images[0]['mimeType']=='image/png'
                return images[0]
            wait(captured);callback_image=captured()
            assert callback_image is not None
            assert base64.b64decode(callback_image['data']).startswith(b'\x89PNG\r\n\x1a\n')
            record['native_capture_callback_ready']=True
            # Actual mouse navigation, no DOM-dispatched movement or browser scene.
            def settled():
                wait(lambda:c.js('!busy && !cameraFlight && !cameraIntent && cameraTimer===null'))
            def drag(button,dx,dy,modifiers=0,cancel=None):
                settled();wait(lambda:c.js('!busy'))
                xy=c.js("(()=>{const r=frame.getBoundingClientRect();return {x:r.x+r.width*.5,y:r.y+r.height*.5}})()")
                c.call('Input.dispatchMouseEvent',{'type':'mousePressed',**xy,'button':button,'buttons':{'left':1,'right':2,'middle':4}[button],'clickCount':1,'modifiers':modifiers})
                for i in range(1,9):
                    c.call('Input.dispatchMouseEvent',{'type':'mouseMoved','x':xy['x']+dx*i/8,'y':xy['y']+dy*i/8,'button':button,'buttons':{'left':1,'right':2,'middle':4}[button],'modifiers':modifiers})
                if cancel=='escape':
                    c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Escape','code':'Escape','windowsVirtualKeyCode':27})
                    c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Escape','code':'Escape','windowsVirtualKeyCode':27})
                elif cancel=='capture':c.js('frame.releasePointerCapture(gesture.id)')
                elif cancel=='pointercancel':c.js("frame.dispatchEvent(new PointerEvent('pointercancel',{pointerId:gesture.id}))")
                elif cancel=='blur':c.call('Emulation.setFocusEmulationEnabled',{'enabled':False});c.js("dispatchEvent(new Event('blur'))")
                c.call('Input.dispatchMouseEvent',{'type':'mouseReleased','x':xy['x']+dx,'y':xy['y']+dy,'button':button,'buttons':0,'clickCount':1,'modifiers':modifiers})
                settled()
            # Sustained genuine pointer stream, settle, then native click and MCP readback.
            xy=c.js("(()=>{const r=frame.getBoundingClientRect();return {x:r.x+r.width*.5,y:r.y+r.height*.5}})()")
            c.call('Input.dispatchMouseEvent',{'type':'mousePressed',**xy,'button':'right','buttons':2,'clickCount':1})
            started=time.monotonic()
            for i in range(180):
                due=started+i/60
                if due>time.monotonic():time.sleep(due-time.monotonic())
                c.call('Input.dispatchMouseEvent',{'type':'mouseMoved','x':xy['x']+i*.3,'y':xy['y']+i*.05,'button':'right','buttons':2})
            c.call('Input.dispatchMouseEvent',{'type':'mouseReleased','x':xy['x']+53.7,'y':xy['y']+8.95,'button':'right','buttons':0,'clickCount':1})
            settled();moved=c.snapshot()
            assert moved['scene']==base['scene']
            assert moved['view']['camera']==tool('viewport_inspect')['result']['camera']
            assert moved['view']['selected_object_ids']==base['view']['selected_object_ids']
            assert c.point(.03,.9)['status']=='completed'
            assert c.snapshot()['view']['selected_object_ids']==tool('viewport_inspect')['result']['selected_object_ids']==[]
            clicked=c.snapshot();c.click('#connect');assert c.snapshot()['png']==clicked['png']
            record['sustained_drag_settle_pick_mcp']=True
            assert c.js("operate('viewport_camera',{...pins(true),camera:"+json.dumps(base['view']['camera'])+"})")['status']=='completed'
            import math
            navigation=[]
            for width,dpr in [(1250,1),(980,2)]:
                c.call('Emulation.setDeviceMetricsOverride',{'width':width,'height':1000,'deviceScaleFactor':dpr,'mobile':False})
                for button,mods in [('right',0),('middle',0),('right',8)]:
                    before=c.snapshot();drag(button,30,15,mods);after=c.snapshot()
                    assert after['view']['camera']!=before['view']['camera'],(button,after['view'])
                    assert after['view']['selected_object_ids']==before['view']['selected_object_ids'],'camera drag picked'
                    assert after['scene']==before['scene']
                    assert after['view']['camera']==tool('viewport_inspect')['result']['camera']
                    c.click('#connect');assert c.snapshot()['png']==after['png'],'camera reply pixels lag native refresh'
                    if button=='right' and not mods:
                        assert math.isclose(after['view']['camera']['yaw'],before['view']['camera']['yaw']-.24,abs_tol=1e-5)
                        assert math.isclose(after['view']['camera']['pitch'],before['view']['camera']['pitch']+.12,abs_tol=1e-5)
                    else:assert after['view']['camera']['target']!=before['view']['camera']['target']
                    navigation.append({'width':width,'dpr':dpr,'button':button,'shift':bool(mods),'native_mcp_equal':True})
                before=c.snapshot();drag('left',30,15);assert c.snapshot()==before,'primary drag caused pick'
                for delta in [120,-120]:
                    wait(lambda:c.js('!busy'));before=c.snapshot();xy=c.js("(()=>{const r=frame.getBoundingClientRect();return {x:r.x+r.width*.5,y:r.y+r.height*.5}})()")
                    c.js("window.lastWheel=null;frame.addEventListener('wheel',e=>{window.lastWheel={deltaY:e.deltaY,deltaMode:e.deltaMode}},{once:true})")
                    prior_sequence=c.js('sequence')
                    c.call('Input.dispatchMouseEvent',{'type':'mouseWheel',**xy,'deltaX':0,'deltaY':delta});wait(lambda:c.js('sequence')>prior_sequence);settled();after=c.snapshot()
                    wheel=c.js('lastWheel');assert wheel and wheel['deltaMode']==0,wheel
                    assert math.isclose(after['view']['camera']['distance'],before['view']['camera']['distance']*math.exp(wheel['deltaY']*.002),rel_tol=1e-5)
                    navigation.append({'width':width,'dpr':dpr,'injected_wheel':delta,'browser_wheel':wheel,'native_mcp_equal':True})
                    assert after['view']['camera']==tool('viewport_inspect')['result']['camera']
                    assert after['view']['selected_object_ids']==before['view']['selected_object_ids']
                for reason in ['escape','capture','pointercancel','blur']:
                    drag('right',24,12,cancel=reason);assert c.js('gesture===null && cameraIntent===null')
                reset=c.js("operate('viewport_camera',{...pins(true),camera:"+json.dumps(base['view']['camera'])+"})");assert reset['status']=='completed'
            record['direct_camera_checks']=navigation
            record['camera_samples']=c.js("samples.filter(r=>r.op==='viewport_camera')")
            record['cancellation_checks']=['Escape key','lost capture','app pointercancel event','app blur event (not OS focus)']
            # Stale native camera change must refuse, not rebase/replay a queued intent.
            before=c.snapshot();c.js('clearTimeout(autoTimer);autoTimer=null')
            native_camera=dict(before['view']['camera']);native_camera['yaw']+=.1
            assert tool('viewport_camera',{'expected_session_token':before['view']['session_token'],'expected_revision':before['view']['revision'],'expected_view_revision':before['view']['view_revision'],'camera':native_camera})['ok']
            drag('right',16,8);assert c.js("outcomes.at(-1).status==='failed' && outcomes.at(-1).error==='Error: stale_view'")
            assert c.js('cameraIntent===null && gesture===null');c.click('#connect')
            assert c.js("operate('viewport_camera',{...pins(true),camera:"+json.dumps(base['view']['camera'])+"})")['status']=='completed'
            record['stale_camera_refusal']=True
            # Actual pointer → native picking; scaled image at two CSS/DPR combinations.
            selection_checks=[]
            for width,dpr in [(1250,1),(980,2)]:
                c.call('Emulation.setDeviceMetricsOverride',{'width':width,'height':1000,'deviceScaleFactor':dpr,'mobile':False})
                c.point(.03,.9); cleared=c.snapshot()
                assert cleared['view']['selected_object_ids']==[],cleared['view']
                assert tool('viewport_inspect')['result']['selected_object_ids']==[]
                c.click('#connect');assert c.snapshot()['png']==cleared['png'],'background reply contains old selected pixels'
                result=c.point(.5,.5); picked=c.snapshot()
                assert result['status']=='completed',result
                assert picked['view']['active_object_id']=='center',picked['view']
                assert tool('viewport_inspect')['result']['active_object_id']=='center'
                assert 'center' in c.js("document.querySelector('#identity').textContent")
                assert picked['png']!=cleared['png'],'native outline did not change pixels'
                c.click('#connect');assert c.snapshot()['png']==picked['png'],'pick reply contains old unselected pixels'
                selection_checks.append({'css_width':width,'dpr':dpr,'picked':'center','background_clear':True})
            record['viewport_pointer_checks']=selection_checks
            for label,snapshot in [('pointer-selected',picked),('pointer-background',cleared)]:
                (a.output/(label+'.png')).write_bytes(base64.b64decode(snapshot['png']))
                (a.output/(label+'.json')).write_text(json.dumps({k:v for k,v in snapshot.items() if k!='png'},indent=2))
            stale_pick=c.js("({...pins(true),x:1,y:1,image_width:document.querySelector('#frame').naturalWidth,image_height:document.querySelector('#frame').naturalHeight})")
            for action in ['right','panright','in']:c.click('[data-camera="'+action+'"]')
            stale=c.js("operate('viewport_pick',"+json.dumps(stale_pick)+")")
            assert stale['status']=='failed' and stale['error']=='Error: stale_view',stale
            for key,value,code in [('expected_session_token','retired','stale_session'),('expected_revision','999','stale_revision'),('image_width',1,'stale_frame'),('x',-1,'invalid_coordinates')]:
                refused=c.js("operate('viewport_pick',{...pins(true),x:1,y:1,image_width:document.querySelector('#frame').naturalWidth,image_height:document.querySelector('#frame').naturalHeight,"+json.dumps(key)+":"+json.dumps(value)+"})")
                assert refused['error']=='Error: '+code,refused
            navigated=c.snapshot();assert navigated['view']['camera']!=base['view']['camera'];assert tool('viewport_inspect')['result']['camera']==navigated['view']['camera']
            # Different surfaces in the changed native camera; no hierarchy fallback.
            for x,y,expected in [(.35,.39,'left'),(.67,.64,'right'),(.48,.50,'center')]:
                result=c.point(x,y)
                assert result['status']=='completed',result
                assert tool('viewport_inspect')['result']['active_object_id']==expected,(expected,c.snapshot()['view'])
                assert c.snapshot()['view']['active_object_id']==expected
                assert c.snapshot()['scene']['objects']==base['scene']['objects']
                assert c.snapshot()['scene']['revision']=='0'
            record['camera_changed_pointer_targets']=['left','right','center']
            c.click('[data-object="center"]');assert tool('viewport_inspect')['result']['active_object_id']=='center'
            c.click('#clear');assert tool('viewport_inspect')['result']['selected_object_ids']==[]
            c.click('[data-object="center"]');original=c.snapshot()
            c.click('#begin');assert tool('scene_inspect')['result']['drafts_pending']
            guarded={'expected_session_token':original['scene']['session_token'],'expected_revision':original['scene']['revision'],'object_id':'center','transform':original['scene']['objects'][1]['transform']}
            refused=tool('scene_transform',guarded);assert refused['error']['code']=='drafts_pending',refused
            c.js("document.querySelector('#translation0').value='1.25'")
            draft_camera=c.snapshot()['view']['camera'];drag('right',16,8)
            assert c.js("outcomes.at(-1).status==='failed' && outcomes.at(-1).error==='Error: drafts_pending'")
            assert tool('viewport_inspect')['result']['camera']==draft_camera
            assert c.js("document.querySelector('#translation0').value")=='1.25'
            record['draft_camera_refusal']=True
            draft_pick=c.point(.03,.9)
            assert draft_pick['status']=='failed' and draft_pick['error']=='Error: drafts_pending',draft_pick
            assert c.js("document.querySelector('#translation0').value")=='1.25'
            assert tool('viewport_inspect')['result']['active_object_id']=='center'
            assert tool('scene_inspect')['result']['drafts_pending']
            c.click('#connect');assert c.js("document.querySelector('#translation0').value")=='1.25'
            # Actual keyboard Enter submission, not invoking the JS authoring function.
            c.js("document.querySelector('#translation0').focus()")
            c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Enter','code':'Enter','windowsVirtualKeyCode':13});c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Enter','code':'Enter','windowsVirtualKeyCode':13});wait(lambda:c.js('!busy && !draft'))
            manual=c.snapshot();assert manual['scene']['revision']=='1';assert tool('scene_inspect')['result']['objects']==manual['scene']['objects'];assert manual['png']!=original['png']
            guarded['expected_revision']='1';guarded['transform']=json.loads(json.dumps(manual['scene']['objects'][1]['transform']));guarded['transform']['translation'][0]=2.5
            changed=tool('scene_transform',guarded);assert changed['ok'],changed
            wait(lambda:c.js("current.scene.revision==='2' && !busy"));agent=c.snapshot();assert agent['scene']['revision']=='2';assert agent['scene']['objects'][1]['transform']['translation'][0]==2.5;assert agent['view']['active_object_id']=='center';assert agent['png']!=manual['png']
            c.click('#undo');undone=c.snapshot();assert undone['scene']['revision']=='3';assert undone['scene']['objects']==manual['scene']['objects'];assert undone['png']==manual['png']
            assert tool('scene_undo',{'expected_session_token':agent['scene']['session_token'],'expected_revision':'3'})['ok']
            wait(lambda:c.js("current.scene.revision==='4' && !busy"));restored=c.snapshot();assert restored['scene']['revision']=='4';assert restored['scene']['objects']==original['scene']['objects'];assert restored['png']==original['png']
            # Stale local writes are not silently rebased; native draft still blocks agent.
            c.click('#begin');c.js("document.querySelector('#translation0').value='7'")
            time.sleep(2)
            assert c.js("document.querySelector('#translation0').value")=='7'
            assert tool('scene_inspect')['result']['drafts_pending']
            refreshed=c.js("operate('refresh',{},true)");assert refreshed['status']=='completed'
            assert c.js("document.querySelector('#translation0').value")=='7'
            # Offline pauses automatic observation, then resumes without a manual refresh.
            c.call('Network.enable')
            c.call('Network.emulateNetworkConditions',{'offline':True,'latency':0,'downloadThroughput':-1,'uploadThroughput':-1})
            time.sleep(.3);before=c.js('observation.status');time.sleep(1.7);assert c.js('observation.status')==before
            c.call('Network.emulateNetworkConditions',{'offline':False,'latency':0,'downloadThroughput':-1,'uploadThroughput':-1})
            wait(lambda:c.js('observation.status')>before)
            # A separate owned headless tab hides this page, no desktop input.
            other=c.call('Target.createTarget',{'url':'about:blank'})['targetId']
            targets=json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json'));other_url=next(t['webSocketDebuggerUrl'] for t in targets if t['id']==other)
            other_c=CDP(other_url);other_c.call('Page.bringToFront');time.sleep(.3)
            hidden=c.js('document.hidden');record['hidden_page_observed']=hidden
            assert hidden,'headless tab switch did not hide qualification page'
            before=c.js('observation.status');time.sleep(1.7);assert c.js('observation.status')==before
            c.call('Page.bringToFront');other_c.ws.close();c.call('Target.closeTarget',{'targetId':other});wait(lambda:c.js('observation.status')>before)
            assert c.js("document.querySelector('#translation0').value")=='7'
            # Inject a delayed read-only transport failure, not a native scene result.
            c.js('window.injectStatusFailure=true');before_frames=c.js('observation.frames')
            wait(lambda:c.js('busy && observation.active===1'))
            refused_busy=c.js("operate('refresh')");assert refused_busy['status']=='busy-refused'
            c.js("document.querySelector('#connect').click()");assert c.js("autoEnabled && outcomes.at(-1).status==='busy-refused'")
            wait(lambda:c.js('!busy && autoDelay===1500'))
            assert c.js('observation.frames')==before_frames
            assert c.js("document.querySelector('#translation0').value")=='7'
            record['injected_status_failure_backoff_ms']=c.js('autoDelay')
            c.js('window.injectStatusFailure=false');wait(lambda:c.js('!busy && autoDelay===750'))
            stale=c.js("operate('scene_transform',{...pins(false,draft.source),expected_revision:'0',object_id:'center',transform:current.scene.objects[1].transform})")
            assert stale['status']=='failed';assert c.js("document.querySelector('#translation0').value")=='7';assert tool('scene_inspect')['result']['drafts_pending']
            c.js("document.querySelector('#translation0').focus()");c.call('Input.dispatchKeyEvent',{'type':'keyDown','key':'Escape','code':'Escape','windowsVirtualKeyCode':27});c.call('Input.dispatchKeyEvent',{'type':'keyUp','key':'Escape','code':'Escape','windowsVirtualKeyCode':27});wait(lambda:c.js('!busy && !draft'))
            assert not tool('scene_inspect')['result']['drafts_pending']
            negatives=[]
            for override in [{'Origin':'http://evil.invalid'},{'X-Workspace-Capability':'wrong'},{'Host':'evil.invalid'}]:
                headers={'Origin':info['url'].rstrip('/'),'Content-Type':'application/json','X-Workspace-Capability':token,**override}
                try:urllib.request.urlopen(urllib.request.Request(info['url']+'operation',data=b'{"op":"refresh","args":{}}',headers=headers));raise AssertionError('boundary accepted')
                except urllib.error.HTTPError as e:assert e.code==403;negatives.append(override.keys().__iter__().__next__())
            for override in [{'Origin':'http://evil.invalid'},{'X-Workspace-Capability':'wrong'},{'Host':'evil.invalid'}]:
                headers={'Origin':info['url'].rstrip('/'),'X-Workspace-Capability':token,**override}
                try:urllib.request.urlopen(urllib.request.Request(info['url']+'status',data=b'',headers=headers));raise AssertionError('status boundary accepted')
                except urllib.error.HTTPError as e:assert e.code==403
            denied=c.js("operate('scene_save',{})");assert denied['status']=='failed' and denied['error']=='Error: forbidden_operation'
            burst=c.js("Promise.all(Array.from({length:80},()=>operate('refresh')))");assert sum(r['status']=='completed' for r in burst)==1;assert sum(r['status']=='busy-refused' for r in burst)==79;assert c.js('outcomes.length')<=64
            preserved_samples=c.js('samples');record['observation']=c.js('({...observation})');assert record['observation']['max']==1
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
