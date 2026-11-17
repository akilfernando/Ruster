"""Native idle plus isolated hidden-tab recovery; no host desktop focus."""
import json,os,fcntl,subprocess,time,select,tempfile,urllib.request,shutil
from pathlib import Path
import psutil
from benchmark import ROOT,OUT,GATE,CHROME,CDP,save,snap,env_for,stop

def main():
    run=OUT/'supplement';run.mkdir(exist_ok=False)
    scene=OUT/'moderate-textured.json';record={}
    with open(GATE,'a') as gate,(run/'native.log').open('w') as log:
        fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB)
        p=subprocess.Popen([str(ROOT/'target/release/ruster-localhost-spike'),str(run),'--grant-disposable-edit-undo'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,env=env_for(scene))
        assert p.stdin and p.stdout
        try:
            p.stdin.write('native\n');p.stdin.flush();assert select.select([p.stdout],[],[],30)[0];first=json.loads(p.stdout.readline())
            before=snap({'native':p.pid});t=time.perf_counter();time.sleep(60)
            record['native_idle']={'seconds':time.perf_counter()-t,'before':before,'after':snap({'native':p.pid}),'first':first}
            p.stdin.close();p.wait(8);assert p.returncode==0
        finally:stop(p)
    run2=run/'browser';run2.mkdir();profile=Path(tempfile.mkdtemp(prefix='ruster-hidden-',dir=os.environ['TMPDIR']))
    with (run2/'server.log').open('w') as log,(run2/'chrome.log').open('w') as clog:
        host=subprocess.Popen(['python3',str(ROOT/'server.py'),'--disposable-fixture','--grant-disposable-edit-undo','--benchmark-output',str(run2)],stdout=log,stderr=log,env=env_for(scene));chrome=None;c=None;c2=None;root=None
        try:
            deadline=time.time()+35
            while not(run2/'server.json').exists():assert host.poll() is None and time.time()<deadline;time.sleep(.1)
            info=json.loads((run2/'server.json').read_text());token=(run2/'capability.secret').read_text();url=info['url']
            chrome=subprocess.Popen([CHROME,'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--disable-background-networking','--user-data-dir='+str(profile),url],stdout=clog,stderr=clog)
            while not(profile/'DevToolsActivePort').exists():assert chrome.poll() is None and time.time()<deadline;time.sleep(.1)
            lines=(profile/'DevToolsActivePort').read_text().splitlines();port=lines[0];root=CDP('ws://127.0.0.1:'+port+lines[1])
            tabs=json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json'));target=next(x for x in tabs if x.get('url')==url)
            c=CDP(target['webSocketDebuggerUrl']);c.call('Page.bringToFront')
            for _ in range(100):
                if c.js("typeof render==='function'"):break
                time.sleep(.1)
            c.js('capability='+json.dumps(token));c.js('render()')
            before=c.js('({samples:samples.length,revision:nativeState.metrics.revision,busy,visibility:document.visibilityState})')
            new=root.call('Target.createTarget',{'url':'about:blank'})['targetId']
            tabs=json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json'));c2=CDP(next(x for x in tabs if x['id']==new)['webSocketDebuggerUrl']);c2.call('Page.bringToFront');time.sleep(.2)
            hidden=c.js('document.visibilityState');c.js("void render('scene_transform')");time.sleep(1)
            during=c.js('({samples:samples.length,revision:nativeState.metrics.revision,busy,visibility:document.visibilityState,receipt:document.querySelector("#receipt").textContent,status:document.querySelector("#status").textContent})')
            t=time.perf_counter();c.call('Page.bringToFront')
            for _ in range(100):
                if c.js('!busy && !presentationState.pending'):break
                time.sleep(.02)
            after=c.js('({samples:samples.length,revision:nativeState.metrics.revision,busy,visibility:document.visibilityState,receipt:document.querySelector("#receipt").textContent,last:samples.at(-1)})')
            record['hidden_tab']={'before':before,'hidden_state':hidden,'after_one_second':during,'recovery_observation_ms':(time.perf_counter()-t)*1000,'after_foreground':after}
            ids=[host.pid,info['native_pid'],chrome.pid]+[x.pid for x in psutil.Process(chrome.pid).children(recursive=True)]
        finally:
            for client in [c,c2,root]:
                if client:client.ws.close()
            if chrome:stop(chrome)
            stop(host);shutil.rmtree(profile,ignore_errors=True)
        record['cleanup']={'pids_gone':{str(pid):not psutil.pid_exists(pid) for pid in ids},'capability_removed':not(run2/'capability.secret').exists()}
    with open(GATE,'a') as gate:fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB);record['cleanup']['gate_reacquired']=True
    save(run/'results.json',record);print(json.dumps(record,indent=2))
if __name__=='__main__':main()
