#!/usr/bin/env python3
"""Owned headless measurement HUD proof. No ChatGPT/physical timing claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from qualify import CHROME, CDP, stop, wait

ROOT = Path(__file__).resolve().parent

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--host-binary', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=False)
    profile = Path(tempfile.mkdtemp(prefix='metrics-chrome-', dir=os.environ['TMPDIR']))
    server = chrome = c = None
    run = None
    record = {'actual_chatgpt': False, 'shared_host_load': True, 'native_sha256': hashlib.sha256(a.host_binary.read_bytes()).hexdigest()}
    try:
        with (a.output/'server.log').open('w') as log, (a.output/'chrome.log').open('w') as clog:
            server = subprocess.Popen(['python3', str(ROOT/'server.py'), '--host-binary', str(a.host_binary), '--output-root', str(a.output), '--grant-disposable-local-and-first-mcp-owner'], stdout=log, stderr=log)
            def ready():
                assert server.poll() is None, 'server failed; inspect log'
                return list(a.output.glob('interactive-*/server.json'))
            wait(ready)
            run = ready()[0].parent
            info = json.loads((run/'server.json').read_text())
            token = (run/'capability.secret').read_text()
            authority = info['url'].split('/')[2]
            for headers in [{}, {'Origin':'http://'+authority,'X-Workspace-Capability':'wrong'}, {'Origin':'http://wrong','X-Workspace-Capability':token}, {'Origin':'http://'+authority,'X-Workspace-Capability':token,'Host':'wrong'}]:
                try:
                    urllib.request.urlopen(urllib.request.Request(info['url']+'metrics',data=b'',headers=headers))
                    raise AssertionError('auth negative accepted')
                except urllib.error.HTTPError as e:
                    assert e.code == 403
            record['metrics_auth_negative_cases'] = 4
            chrome = subprocess.Popen([str(CHROME),'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--disable-background-networking','--user-data-dir='+str(profile),info['url']], stdout=clog, stderr=clog)
            wait(lambda:(profile/'DevToolsActivePort').exists())
            port = (profile/'DevToolsActivePort').read_text().splitlines()[0]
            target = next(t for t in json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json')) if t.get('url') == info['url'])
            c = CDP(target['webSocketDebuggerUrl'])
            c.call('Emulation.setDeviceMetricsOverride',{'width':1250,'height':1100,'deviceScaleFactor':1,'mobile':False})
            c.call('Page.bringToFront')
            wait(lambda:c.js("typeof viewportMeasurement!=='undefined'"))
            c.js("document.getElementById('capability').value="+json.dumps(token))
            c.click('#connect')
            c.call('Browser.setDownloadBehavior',{'behavior':'allow','downloadPath':str(a.output)})
            c.js("document.getElementById('measureIdle').click()")
            wait(lambda:c.js('!!viewportMeasurement.capture.report'),25)
            idle = c.js('viewportMeasurement.capture.report')
            assert idle['changed_native_pngs_at_paint_opportunities'] == 0, idle
            assert idle['window_ms'] >= 19900
            assert len(idle['resources']) >= 15
            (a.output/'idle-report.json').write_text(json.dumps(idle,indent=2))
            c.js("document.getElementById('measureMovement').click()")
            xy = c.js("(()=>{const r=frame.getBoundingClientRect();return {x:r.x+r.width*.5,y:r.y+r.height*.5}})()")
            c.call('Input.dispatchMouseEvent',{'type':'mousePressed',**xy,'button':'right','buttons':2,'clickCount':1})
            start = time.monotonic()
            i = 0
            while time.monotonic()-start < 18:
                # Back-and-forth genuine CDP pointer input. No host input/focus.
                import math
                c.call('Input.dispatchMouseEvent',{'type':'mouseMoved','x':xy['x']+80*math.sin(i*.025),'y':xy['y']+15*math.sin(i*.02),'button':'right','buttons':2})
                i += 1
                time.sleep(.012)
            c.call('Input.dispatchMouseEvent',{'type':'mouseReleased','x':xy['x'],'y':xy['y'],'button':'right','buttons':0,'clickCount':1})
            wait(lambda:c.js('!busy&&!cameraFlight&&!cameraIntent&&gesture===null'))
            wait(lambda:c.js('!!viewportMeasurement.capture.report'),6)
            movement = c.js('viewportMeasurement.capture.report')
            assert movement['changed_native_pngs_at_paint_opportunities'] > 64, movement
            assert movement['distributions']['first_input_to_decoded_ms']['count'] > 64
            assert movement['stop_reason'] == 'expired'
            encoded = json.dumps(movement)
            for forbidden in [token, info['locator'], info['incarnation'], 'session_token', 'scene_id', 'object_ids', 'data:image', 'png:']:
                assert forbidden not in encoded, 'private data in report'
            (a.output/'movement-report.json').write_text(json.dumps(movement,indent=2))
            c.js("document.getElementById('measureExport').click()")
            exported = a.output/'rusterize-movement-measurement.json'
            wait(exported.exists)
            assert json.loads(exported.read_text()) == movement
            # Stop path on hidden state, with no mutations/replay.
            c.js("viewportMeasurement.start('idle');Object.defineProperty(document,'hidden',{configurable:true,value:true});dispatchEvent(new Event('visibilitychange'))")
            assert c.js('viewportMeasurement.capture.report.stop_reason') == 'hidden'
            record.update({'idle_updates_per_second':idle['changed_updates_per_second'],'movement_updates_per_second':movement['changed_updates_per_second'],'movement_count':len(movement['frames']),'input_to_decoded_ms':movement['distributions']['first_input_to_decoded_ms'],'exports_verified':True,'final_drag_settled':True,'hidden_stops':True,'credential_free':True})
    finally:
        if c:
            c.ws.close()
        stop(chrome)
        stop(server)
        shutil.rmtree(profile)
        if run and (run/'server-cleanup.json').exists():
            record['cleanup'] = json.loads((run/'server-cleanup.json').read_text())
        record['server_stopped'] = server is None or server.poll() is not None
        record['chrome_stopped'] = chrome is None or chrome.poll() is not None
        record['source_sha256'] = {f:hashlib.sha256((ROOT/f).read_bytes()).hexdigest() for f in ['server.py','workspace.html','measurement.js','metrics.py','qualify_measurement.py']}
        (a.output/'result.json').write_text(json.dumps(record,indent=2))
    assert record['cleanup']['capability_absent'] and record['cleanup']['locator_absent'] and record['cleanup']['host_exit']==0
    print(json.dumps(record,indent=2))

if __name__ == '__main__':
    main()
