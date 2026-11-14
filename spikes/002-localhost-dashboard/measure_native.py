"""Matched offscreen native baseline, NOT ordinary native-window performance."""
import fcntl,json,os,select,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def snapshot(pid):
    p=Path('/proc')/str(pid)
    stat=p.joinpath('stat').read_text().split(') ',1)[1].split()
    status=p.joinpath('status').read_text().splitlines()
    return {'cpu_ticks':int(stat[11])+int(stat[12]), 'rss_kib':int(next(s for s in status if s.startswith('VmRSS:')).split()[1])}

def main():
    run=ROOT/'evidence/native-baseline';run.mkdir(parents=True,exist_ok=True)
    with open('/home/akilf/.hermes/qa/rusterize/state/gate.lock','a') as gate:
        fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB)
        env={k:v for k,v in os.environ.items() if not k.startswith(('RUSTER_','EGUI_INSPECTION'))}
        env.update(VK_LAYER_PATH='/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer',VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation',VK_LAYER_VALIDATE_SYNC='1',VK_LOADER_DEBUG='layer',RUSTER_MSAA_SAMPLES='4')
        with (run/'native.log').open('w') as log:
            p=subprocess.Popen([str(ROOT/'target/release/ruster-localhost-spike'),str(run),'--grant-disposable-edit-undo'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,env=env)
            assert p.stdin is not None and p.stdout is not None
            try:
                rows=[]
                modes=['native','render']+['native','render','render','native']*5
                for i,mode in enumerate(modes):
                    start=time.perf_counter();p.stdin.write(mode+'\n');p.stdin.flush()
                    assert select.select([p.stdout],[],[],30)[0]
                    row=json.loads(p.stdout.readline());row['direct_roundtrip_ms']=(time.perf_counter()-start)*1000
                    row['warmup']=i<2;row['mode']=mode;rows.append(row)
                    time.sleep(max(0, .25-(time.perf_counter()-start)))
                a=snapshot(p.pid);start=time.perf_counter();time.sleep(3);b=snapshot(p.pid)
                idle={'seconds':time.perf_counter()-start,'before':a,'after':b,'hz':os.sysconf('SC_CLK_TCK')}
                (run/'results.json').write_text(json.dumps({'rows':rows,'idle':idle},indent=2))
            finally:
                p.stdin.close()
                try:p.wait(timeout=5)
                except subprocess.TimeoutExpired:p.kill();p.wait()
            assert p.returncode==0,p.returncode
            print(json.dumps({'samples':len(rows),'native_exit':p.returncode,'idle':idle}))
if __name__=='__main__':main()
