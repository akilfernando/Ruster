"""Real owned Chromium/native regressions; no desktop input or client configuration."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.request
from benchmark import ROOT, OUT, CHROME, CDP, env_for, stop, save


def wait_for(predicate, seconds=10):
    deadline = time.monotonic() + seconds
    while not predicate():
        assert time.monotonic() < deadline, 'condition timed out'
        time.sleep(.02)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('case', choices=['burst', 'hidden'])
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    run = args.output.resolve()
    run.mkdir(parents=True, exist_ok=False)
    profile = Path(tempfile.mkdtemp(prefix='ruster-command-', dir=os.environ['TMPDIR']))
    clients = []
    host = chrome = None
    record = {'case': args.case}
    try:
        with (run/'server.log').open('w') as log, (run/'chrome.log').open('w') as clog:
            host = subprocess.Popen(['python3', str(ROOT/'server.py'), '--disposable-fixture', '--grant-disposable-edit-undo', '--benchmark-output', str(run)], stdout=log, stderr=log, env=env_for(OUT/'small.json'))
            def host_ready():
                assert host.poll() is None, 'owned server exited before readiness; inspect server.log (possibly QA gate occupied)'
                return (run/'server.json').exists()
            wait_for(host_ready, 35)
            info = json.loads((run/'server.json').read_text())
            chrome = subprocess.Popen([CHROME, '--headless=new', '--ozone-platform=headless', '--remote-debugging-port=0', '--remote-allow-origins=*', '--no-first-run', '--disable-background-networking', '--user-data-dir='+str(profile), info['url']], stdout=clog, stderr=clog)
            wait_for(lambda: (profile/'DevToolsActivePort').exists())
            lines = (profile/'DevToolsActivePort').read_text().splitlines()
            endpoint = 'http://127.0.0.1:'+lines[0]+'/json'
            root = CDP('ws://127.0.0.1:'+lines[0]+lines[1]); clients.append(root)
            target = next(t for t in json.load(urllib.request.urlopen(endpoint)) if t.get('url') == info['url'])
            c = CDP(target['webSocketDebuggerUrl']); clients.append(c)
            c.call('Emulation.setDeviceMetricsOverride', {'width':1100,'height':1100,'deviceScaleFactor':1,'mobile':False})
            c.call('Page.bringToFront')
            wait_for(lambda: c.js("typeof render==='function'"))
            c.js('capability='+json.dumps((run/'capability.secret').read_text()))
            c.js('render()')
            wait_for(lambda: c.js('!busy && samples.length===1'))
            if args.case == 'burst':
                c.js("void (window.burst=Promise.all(Array.from({length:10},()=>render('scene_transform'))))")
                wait_for(lambda: c.js('!busy'))
                record['observed'] = c.js('({revision:nativeState.metrics.revision,outcomes:window.commandOutcomes||null,visible:document.querySelector("#outcomes")?.textContent})')
                rows = record['observed']['outcomes']
                assert rows is not None, 'rapid commands need explicit outcomes, not silent return'
                burst = rows[-10:]
                assert len(burst) == 10
                assert sum(r['status']=='completed' for r in burst) == 1
                assert sum(r['status']=='rejected' and r['reason']=='busy' for r in burst) == 9
                assert record['observed']['revision'] == 1
                assert 'rejected' in record['observed']['visible']
                result = c.js("render('scene_undo')")
                assert result['status'] == 'completed'
                assert c.js('nativeState.metrics.revision') == 2
                # Bound the diagnostic ledger as well as admission; mixed discrete
                # operations must be rejected, never coalesced with the active edit.
                bounded = c.js("(async()=>{const results=await Promise.all(Array.from({length:80},(_,i)=>render(['scene_transform','scene_undo','scene_save','reopen_fixture',null][i%5])));return {completed:results.filter(r=>r.status==='completed').length,rejected:results.filter(r=>r.status==='rejected'&&r.reason==='busy').length,retained:commandOutcomes.length,revision:nativeState.metrics.revision}})()")
                assert bounded == {'completed':1,'rejected':79,'retained':64,'revision':3}, bounded
                record['bounded_mixed'] = bounded
                # No implicit permission or stale-intent retry/rebase.
                refused = c.js("render('scene_save')")
                assert refused['status'] == 'failed'
                c.js("nativeState.authority.result.revision='0'")
                stale = c.js("render('scene_transform')")
                assert stale['status'] == 'failed'
                record['refusals'] = {'save':refused,'stale':stale}
                assert c.render_presented()['status'] == 'completed'
                assert c.js('nativeState.metrics.revision') == 3
            else:
                new = root.call('Target.createTarget', {'url':'about:blank'})['targetId']
                other = CDP(next(t for t in json.load(urllib.request.urlopen(endpoint)) if t['id']==new)['webSocketDebuggerUrl']); clients.append(other)
                other.call('Page.bringToFront')
                wait_for(lambda: c.js("document.visibilityState==='hidden'"))
                c.js("void render('scene_transform')")
                time.sleep(1)
                record['hidden'] = c.js('({busy,revision:nativeState.metrics.revision,samples:samples.length,receipt:document.querySelector("#receipt").textContent,presentation:window.presentationState||null})')
                assert not record['hidden']['busy'], 'hidden RAF must not hold operation admission busy'
                assert record['hidden']['revision'] == 1
                assert record['hidden']['samples'] == 2
                assert 'Revision 1' in record['hidden']['receipt']
                assert record['hidden']['presentation']['pending']
                # A second discrete command must complete while still hidden.
                result = c.js("render('scene_undo')")
                assert result['status'] == 'completed'
                assert c.js('nativeState.metrics.revision') == 2
                c.call('Page.bringToFront')
                wait_for(lambda: c.js('!presentationState.pending'))
                record['resumed'] = c.js('({revision:nativeState.metrics.revision,presentation:presentationState,samples})')
                assert record['resumed']['presentation']['revision'] == 2
                assert record['resumed']['samples'][-1]['request_to_raf_ms'] is not None
            import base64
            (run/'dashboard.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot', {'format':'png'})['data']))
            record['pass'] = True
    except Exception as error:
        record['error'] = str(error)
        raise
    finally:
        for c in clients:
            c.ws.close()
        if chrome: stop(chrome)
        if host: stop(host)
        shutil.rmtree(profile)
        record['cleanup'] = {'host_stopped':host is None or host.poll() is not None, 'chrome_stopped':chrome is None or chrome.poll() is not None, 'capability_removed':not (run/'capability.secret').exists(), 'profile_removed':not profile.exists()}
        save(run/'results.json', record)
        print(json.dumps({'case':args.case,'pass':record.get('pass',False),'error':record.get('error'),'cleanup':record['cleanup'],'evidence':str(run/'results.json')}, indent=2))


if __name__ == '__main__':
    main()
