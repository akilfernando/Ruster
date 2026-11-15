#!/usr/bin/env python3
"""Owned native/Chromium sustained camera comparison, not ChatGPT acceptance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.request
from qualify import CDP, CHROME, ROOT, wait, stop

def main():
    p=argparse.ArgumentParser();p.add_argument('--host-binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--dispatch-delay-ms',type=int);p.add_argument('--require-target',action='store_true');a=p.parse_args()
    a.output.mkdir(parents=True,exist_ok=False)
    profile=Path(tempfile.mkdtemp(prefix='camera-profile-',dir=os.environ['TMPDIR']))
    server=chrome=c=None;run=None;record={'host_sha256':hashlib.sha256(a.host_binary.read_bytes()).hexdigest(),'actual_chatgpt':False}
    try:
        with (a.output/'server.log').open('w') as log,(a.output/'chrome.log').open('w') as clog:
            server=subprocess.Popen(['python3',str(ROOT/'server.py'),'--host-binary',str(a.host_binary),'--output-root',str(a.output),'--grant-disposable-local-and-first-mcp-owner'],stdout=log,stderr=log)
            def ready():
                assert server.poll() is None,'server startup failed'
                return list(a.output.glob('interactive-*/server.json'))
            wait(ready);run=ready()[0].parent;info=json.loads((run/'server.json').read_text())
            chrome=subprocess.Popen([str(CHROME),'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--disable-background-networking','--user-data-dir='+str(profile),info['url']],stdout=clog,stderr=clog)
            wait(lambda:(profile/'DevToolsActivePort').exists());port=(profile/'DevToolsActivePort').read_text().splitlines()[0]
            target=next(t for t in json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json')) if t.get('url')==info['url']);c=CDP(target['webSocketDebuggerUrl'])
            c.call('Emulation.setDeviceMetricsOverride',{'width':1250,'height':1000,'deviceScaleFactor':1,'mobile':False});c.call('Page.bringToFront');wait(lambda:c.js("typeof operate==='function'"))
            c.js("document.querySelector('#capability').value="+json.dumps((run/'capability.secret').read_text()));c.click('#connect')
            base=c.snapshot();record['initial_png_sha256']=hashlib.sha256(__import__('base64').b64decode(base['png'])).hexdigest()
            if a.dispatch_delay_ms is not None:
                assert 0 <= a.dispatch_delay_ms <= 1000
                c.js('scheduleCamera=()=>{if(cameraTimer===null&&!cameraFlight&&cameraIntent)cameraTimer=setTimeout(flushCamera,'+str(a.dispatch_delay_ms)+')}')
            record['dispatch_delay_ms']=a.dispatch_delay_ms
            record['resolution']=c.js('({width:frame.naturalWidth,height:frame.naturalHeight,cssWidth:frame.clientWidth})')
            c.js("window.profileReplies=[];window.profileSamples=[];const samplePush=samples.push.bind(samples);samples.push=(...rows)=>{profileSamples.push(...rows);return samplePush(...rows)};const pf=fetch;window.fetch=async(...args)=>{const r=await pf(...args);if(args[0]==='/operation'){const v=await r.clone().json();profileReplies.push({phases:v.phases_ms,request_ms:v.request_ms,native_frame:v.native_frame,png:v.png,view:v.view})}return r};window.presented=[];window.lastInput=0;window.lastNative=null;frame.addEventListener('pointermove',()=>lastInput=performance.now());window.watchFrame=()=>{if(current&&current.native_frame!==lastNative){lastNative=current.native_frame;presented.push({time:performance.now(),native_frame:lastNative,last_input:lastInput,view:current.view,decoded:samples.at(-1)?.request_to_decoded_ms})}window.watchId=requestAnimationFrame(watchFrame)};watchFrame()")
            xy=c.js('(()=>{const r=frame.getBoundingClientRect();return {x:r.x+r.width*.5,y:r.y+r.height*.5}})()')
            c.call('Input.dispatchMouseEvent',{'type':'mousePressed',**xy,'button':'right','buttons':2,'clickCount':1})
            c.js('window.dragStart=performance.now()')
            start=time.monotonic()
            for i in range(240):
                due=start+i/60
                if due>time.monotonic():time.sleep(due-time.monotonic())
                c.call('Input.dispatchMouseEvent',{'type':'mouseMoved','x':xy['x']+i*.5,'y':xy['y']+i*.08,'button':'right','buttons':2})
            c.js('window.dragEnd=performance.now()')
            c.call('Input.dispatchMouseEvent',{'type':'mouseReleased','x':xy['x']+119.5,'y':xy['y']+19.12,'button':'right','buttons':0,'clickCount':1})
            wait(lambda:c.js('!busy&&!cameraFlight&&!cameraIntent&&cameraTimer===null'))
            record['drag_interval']=c.js('({start:dragStart,end:dragEnd})')
            record['presented']=c.js('presented');record['samples']=c.js("profileSamples.filter(x=>x.op==='viewport_camera')");replies=c.js('profileReplies')
            for r in replies:r['png_sha256']=hashlib.sha256(__import__('base64').b64decode(r.pop('png'))).hexdigest()
            # Native render identities can legitimately repeat pixels (e.g. a no-op
            # settled intent). Count distinct decoded PNGs observed at paint opportunities,
            # not HTTP replies or every new native frame ID.
            record['duplicate_png_replies']=len(replies)-len({r['png_sha256'] for r in replies})
            record['replies']=replies;record['scene_unchanged']=c.snapshot()['scene']==base['scene'];assert record['scene_unchanged']
            import statistics
            active=[r for r in record['presented'] if record['drag_interval']['start']<=r['time']<=record['drag_interval']['end'] and r['native_frame'] in {x['native_frame'] for x in replies}]
            hashes={r['native_frame']:r['png_sha256'] for r in replies}
            seen=set();unique=[]
            for row in active:
                digest=hashes[row['native_frame']]
                if digest not in seen:
                    seen.add(digest);unique.append(row)
            record['paint_opportunities_with_native_ids']=len(active)
            active=unique
            duration=(record['drag_interval']['end']-record['drag_interval']['start'])/1000
            record['summary']={'committed_changing_frames':len(set(r['png_sha256'] for r in replies)),'presented_frames':len(active),'fps_during_drag':len(active)/duration,'drag_seconds':duration,'median_request_to_decoded_ms':statistics.median(r['request_to_decoded_ms'] for r in record['samples']),'median_first_input_to_decoded_ms':statistics.median(r.get('first_input_to_decoded_ms',r['request_to_decoded_ms']) for r in record['samples']),'median_latest_input_to_decoded_ms':statistics.median(r.get('latest_input_to_decoded_ms',r['request_to_decoded_ms']) for r in record['samples'])}
            phases=[r['phases'] for r in replies if r.get('phases')]
            record['phase_medians_ms']={k:statistics.median(r[k] for r in phases) for k in phases[0]} if phases else {}
            final=c.snapshot();assert __import__('math').isclose(final['view']['camera']['yaw'],base['view']['camera']['yaw']-119.5*.008,abs_tol=1e-5),'final pointer delta lost'
            record['final_delta_settled']=True
            (a.output/'settled.png').write_bytes(__import__('base64').b64decode(final['png']))
            (a.output/'browser.png').write_bytes(__import__('base64').b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            record['target_met']=record['summary']['fps_during_drag']>=15 and record['summary']['median_first_input_to_decoded_ms']<100
            if a.require_target:assert record['target_met'],record['summary']
            print(json.dumps({k:record[k] for k in ['summary','phase_medians_ms','resolution']},indent=2))
    finally:
        if c:c.ws.close()
        stop(chrome);stop(server);shutil.rmtree(profile)
        if run and (run/'server-cleanup.json').exists():record['cleanup']=json.loads((run/'server-cleanup.json').read_text())
        (a.output/'result.json').write_text(json.dumps(record,indent=2))
if __name__=='__main__':main()
