"""Bounded actual-render pipeline comparison; not native-window/client UX parity."""
import base64, fcntl, hashlib, json, math, os, select, shutil, statistics, struct, subprocess, tempfile, time, urllib.request, zlib
from pathlib import Path
import psutil
from websockets.sync.client import connect
ROOT=Path(__file__).resolve().parent
OUT=ROOT/'evidence/representative-performance'
CHROME='/home/akilf/.hermes/tools/chromium-1208/chrome-linux64/chrome'
GATE='/home/akilf/.hermes/qa/rusterize/state/gate.lock'

def save(path,value): path.write_text(json.dumps(value,indent=2))
def snap(pids):
    result={}
    for name,pid in pids.items():
        try:
            p=psutil.Process(pid); c=p.cpu_times(); result[name]={'pid':pid,'cpu_s':c.user+c.system,'rss':p.memory_info().rss}
        except psutil.NoSuchProcess: pass
    return result

def env_for(scene):
    env={k:v for k,v in os.environ.items() if not k.startswith(('RUSTER_','EGUI_INSPECTION','SPIKE_BENCH_'))}
    env.update(VK_LAYER_PATH='/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer',VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation',VK_LAYER_VALIDATE_SYNC='1',VK_LOADER_DEBUG='layer',RUSTER_MSAA_SAMPLES='4',SPIKE_BENCH_SCENE=str(scene))
    if 'textured' in scene.stem: env['SPIKE_BENCH_TEXTURE']=str(OUT/'checker.png')
    return env

def stop(p):
    if p.poll() is None:
        p.terminate()
        try:p.wait(8)
        except subprocess.TimeoutExpired:p.kill();p.wait()

def fixtures():
    OUT.mkdir(parents=True,exist_ok=True)
    seed=json.loads((ROOT/'evidence/scoped-save/scene.json').read_text()); seed['revision']=0
    paths=[]
    for name,n,textured in [('small',3,False),('moderate',64,False),('moderate-textured',256,True)]:
        scene=json.loads(json.dumps(seed)); scene['id']='benchmark-'+name
        if n>3:
            side=math.isqrt(n); scene['objects']=[]
            for i in range(n):
                o=json.loads(json.dumps(seed['objects'][i%3])); o['id']='object-'+str(i)
                o['transform']['translation']=[(i%side-(side-1)/2)*5/side,0,(i//side-(side-1)/2)*5/side]
                o['transform']['scale']=[3/side]*3
                if i%3: o['primitive']=['sphere','capsule'][i%3-1]
                if textured:
                    o['material']='checker'
                    o['primitive']=['sphere','capsule'][i%2]
                scene['objects'].append(o)
        if textured:scene['materials']={'checker':{'tint':[1,1,1],'base_color_texture':'bench-texture','filter':'linear','address':'repeat'}}
        p=OUT/(name+'.json'); save(p,scene); paths.append(p)
    def chunk(tag,data):return struct.pack('>I',len(data))+tag+data+struct.pack('>I',zlib.crc32(tag+data)&0xffffffff)
    raw=b''.join(b'\0'+b''.join(bytes((230,150,40,255)) if (x//16+y//16)%2 else bytes((30,80,220,255)) for x in range(256)) for y in range(256))
    (OUT/'checker.png').write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',256,256,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b''))
    return paths

class CDP:
    def __init__(self,url): self.ws=connect(url,max_size=32*1024*1024); self.i=0
    def call(self,method,params=None):
        self.i+=1; self.ws.send(json.dumps({'id':self.i,'method':method,'params':params or {}}))
        while True:
            r=json.loads(self.ws.recv(timeout=40))
            if r.get('id')==self.i:
                if 'error' in r: raise RuntimeError(r)
                return r.get('result',{})
    def render_presented(self, operation=None):
        """Measurement-only wait; production admission does not await RAF."""
        result=self.js('render('+json.dumps(operation)+')')
        if result['status']!='completed':raise AssertionError(result)
        deadline=time.monotonic()+10
        while self.js('presentationState.pending'):
            if time.monotonic()>deadline:raise TimeoutError('paint opportunity pending; not a foreground timing sample')
            time.sleep(.005)
        return result
    def js(self,expr):
        r=self.call('Runtime.evaluate',{'expression':expr,'awaitPromise':True,'returnByValue':True})
        if 'exceptionDetails' in r:raise RuntimeError(r)
        return r['result'].get('value')


def direct(scene):
    run=OUT/(scene.stem+'-direct');run.mkdir(exist_ok=True)
    with open(GATE,'a') as gate, (run/'native.log').open('w') as log:
        fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB)
        p=subprocess.Popen([str(ROOT/'target/release/ruster-localhost-spike'),str(run),'--grant-disposable-edit-undo'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,env=env_for(scene))
        assert p.stdin is not None and p.stdout is not None
        rows=[]; resources=[]
        try:
            a=snap({'native':p.pid}); start=time.perf_counter()
            for i,mode in enumerate(['native','render']+['native','render','render','native']*10):
                t=time.perf_counter();p.stdin.write(mode+'\n');p.stdin.flush()
                assert select.select([p.stdout],[],[],30)[0]
                line=p.stdout.readline(); assert line, 'native exited'
                row=json.loads(line);row.update(mode=mode,warmup=i<2,roundtrip_ms=(time.perf_counter()-t)*1000);rows.append(row)
                resources.append(snap({'native':p.pid}))
                time.sleep(max(0,.25-(time.perf_counter()-t)))
            active={'seconds':time.perf_counter()-start,'before':a,'after':snap({'native':p.pid})}
            png_hash=hashlib.sha256((run/'preview.png').read_bytes()).hexdigest()
            p.stdin.close();p.wait(8)
            assert p.returncode==0
            save(run/'results.json',{'rows':rows,'resources':resources,'active':active,'png_sha256':png_hash})
        finally: stop(p)
    return png_hash


def browser(scene,expected_hash):
    run=OUT/(scene.stem+'-browser');run.mkdir(exist_ok=True)
    profile=Path(tempfile.mkdtemp(prefix='ruster-perf-',dir=os.environ['TMPDIR']))
    with (run/'server.log').open('w') as log, (run/'chrome.log').open('w') as clog:
        host=subprocess.Popen(['python3',str(ROOT/'server.py'),'--disposable-fixture','--grant-disposable-edit-undo','--benchmark-output',str(run)],stdout=log,stderr=log,env=env_for(scene))
        chrome=None; c=None
        try:
            deadline=time.time()+35
            while not (run/'server.json').exists():
                assert host.poll() is None, 'server failed'
                assert time.time()<deadline
                time.sleep(.1)
            info=json.loads((run/'server.json').read_text()); url=info['url'];token=(run/'capability.secret').read_text()
            chrome=subprocess.Popen([CHROME,'--headless=new','--ozone-platform=headless','--remote-debugging-port=0','--remote-allow-origins=*','--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-sync','--password-store=basic','--user-data-dir='+str(profile),url],stdout=clog,stderr=clog)
            while not (profile/'DevToolsActivePort').exists():
                assert chrome.poll() is None;time.sleep(.1)
            port=(profile/'DevToolsActivePort').read_text().splitlines()[0]
            target=None
            for _ in range(100):
                tabs=json.load(urllib.request.urlopen('http://127.0.0.1:'+port+'/json'))
                target=next((x for x in tabs if x.get('type')=='page' and x.get('url')==url),None)
                if target:break
                time.sleep(.1)
            assert target is not None
            c=CDP(target['webSocketDebuggerUrl']);c.call('Emulation.setDeviceMetricsOverride',{'width':1100,'height':1000,'deviceScaleFactor':1,'mobile':False});c.call('Page.bringToFront');c.call('Performance.enable')
            for _ in range(100):
                if c.js("typeof render==='function'"):break
                time.sleep(.1)
            c.js('capability='+json.dumps(token))
            c.render_presented()
            assert c.js('window.samples.length')==1
            pixels=c.js("document.querySelector('#preview').src.split(',')[1]")
            actual_hash=hashlib.sha256(base64.b64decode(pixels)).hexdigest();assert actual_hash==expected_hash
            pids={'native':info['native_pid'],'http':host.pid,'browser':chrome.pid}
            for child in psutil.Process(chrome.pid).children(recursive=True):
                args=' '.join(child.cmdline())
                if '--type=renderer' in args:pids['renderer-'+str(child.pid)]=child.pid
                elif '--type=gpu-process' in args:pids['gpu-process']=child.pid
            before=snap(pids);t=time.perf_counter(); rows=[];resources=[]
            for _ in range(20):
                start=time.perf_counter();c.render_presented(); rows.append(c.js('window.samples.at(-1)'));resources.append(snap(pids));time.sleep(max(0,.25-(time.perf_counter()-start)))
            active={'seconds':time.perf_counter()-t,'before':before,'after':snap(pids)}
            edits=[]
            for i in range(20):
                start=time.perf_counter();c.render_presented('scene_transform' if i%2==0 else 'scene_undo');edits.append(c.js('window.samples.at(-1)'));time.sleep(max(0,.25-(time.perf_counter()-start)))
            burst=c.js("(async()=>{let before={samples:window.samples.length,revision:nativeState.metrics.revision,frames:nativeState.metrics.frames};await Promise.all(Array.from({length:10},()=>render('scene_transform')));return {before,after:{samples:window.samples.length,revision:nativeState.metrics.revision,frames:nativeState.metrics.frames},busy}})()")
            # All ten intents receive outcomes; nine are explicitly rejected while busy.
            fresh=c.js("({revision:nativeState.metrics.revision,receipt:document.querySelector('#receipt').textContent,status:document.querySelector('#status').textContent,visibility:document.visibilityState})")
            c.render_presented(); drain=c.js('window.samples.at(-1)')
            idle=None
            if scene.stem=='moderate-textured':
                before=snap(pids); browser_before=c.call('Performance.getMetrics');t=time.perf_counter();time.sleep(60)
                idle={'seconds':time.perf_counter()-t,'before':before,'after':snap(pids),'browser_before':browser_before,'browser_after':c.call('Performance.getMetrics'),'frames_unchanged':c.js('nativeState.metrics.frames')==drain['frames']}
            (run/'browser.png').write_bytes(base64.b64decode(c.call('Page.captureScreenshot',{'format':'png'})['data']))
            save(run/'results.json',{'rows':rows,'edit_rows':edits,'resources':resources,'active':active,'idle':idle,'burst':burst,'freshness':fresh,'drain':drain,'png_sha256':actual_hash,'pids':pids,'browser_version':c.call('Browser.getVersion')})
        finally:
            if c:c.ws.close()
            if chrome:stop(chrome)
            stop(host)
            shutil.rmtree(profile,ignore_errors=True)
            ids=[host.pid]+([chrome.pid] if chrome else [])
            if (run/'server.json').exists():ids.append(json.loads((run/'server.json').read_text())['native_pid'])
            save(run/'cleanup.json',{'pids_gone':{str(pid):not psutil.pid_exists(pid) for pid in ids},'capability_removed':not(run/'capability.secret').exists()})
    with open(GATE,'a') as gate:fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB)


def summary():
    result={}
    for scene in ['small','moderate','moderate-textured']:
        d=json.loads((OUT/(scene+'-direct/results.json')).read_text());b=json.loads((OUT/(scene+'-browser/results.json')).read_text())
        cohorts={'native-only':[r for r in d['rows'] if not r['warmup'] and r['mode']=='native'],'native-png':[r for r in d['rows'] if not r['warmup'] and r['mode']=='render'],'browser':b['rows'],'edit-undo':b['edit_rows']}
        result[scene]={}
        for name,rows in cohorts.items():
            result[scene][name]={}
            for key in ['command_to_commit_ms','render_cpu_ms','submit_to_mapped_ms','png_encode_ms','native_total_ms','host_roundtrip_ms','http_body_received_ms','image_decode_ms','request_to_raf_ms']:
                values=sorted(r[key] for r in rows if key in r)
                if values:result[scene][name][key]={'n':len(values),'median':statistics.median(values),'p95':values[math.ceil(.95*len(values))-1]}
    save(OUT/'summary.json',result)
    lines=['# Matched pipeline timing (ms, median / nearest-rank p95)','', '| Workload | Native render-only | Native PNG | Browser native | Browser body received | Browser decode + 2 RAF | Edit commit | Edit request → RAF |','| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for name,r in result.items():
        def val(c,k):s=r[c][k];return f"{s['median']:.2f} / {s['p95']:.2f}"
        lines.append('| '+' | '.join([name,val('native-only','native_total_ms'),val('native-png','native_total_ms'),val('browser','native_total_ms'),val('browser','http_body_received_ms'),val('browser','request_to_raf_ms'),val('edit-undo','command_to_commit_ms'),val('edit-undo','request_to_raf_ms')])+' |')
    (OUT/'table.md').write_text('\n'.join(lines)+'\n')
    print('\n'.join(lines))

if __name__=='__main__':
    if OUT.exists():
        raise SystemExit('Refusing to overwrite retained evidence; archive evidence/representative-performance first.')
    paths=fixtures()
    save(OUT/'identity.json',{'binary_sha256':hashlib.sha256((ROOT/'target/release/ruster-localhost-spike').read_bytes()).hexdigest(),'started':time.strftime('%Y-%m-%dT%H:%M:%S%z'),'resolution':[768,512],'msaa':4,'cadence_s':.25,'fixtures':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}})
    for p in paths:
        print('START',p.stem,flush=True);h=direct(p);browser(p,h);print('DONE',p.stem,flush=True)
    summary()
