"""Real HTTP -> deferred SceneEditor Save -> fresh native Session::open."""
import base64, copy, fcntl, hashlib, http.client, json, os, subprocess, time
from pathlib import Path
ROOT = Path(__file__).resolve().parent

def run(granted):
    evidence = ROOT/'evidence'/('scoped-save' if granted else 'scoped-save-denied')
    evidence.mkdir(exist_ok=True)
    argv = ['python3', 'server.py', '--disposable-fixture', '--grant-disposable-edit-undo', '--save-qualification']
    if granted: argv += ['--grant-disposable-fixed-save']
    log = (evidence/'host.log').open('w')
    host = subprocess.Popen(argv, cwd=ROOT, stdout=subprocess.PIPE, stderr=log, text=True)
    try:
        line = host.stdout.readline()
        assert line.startswith('Dashboard:'), (line,host.poll())
        info = json.loads((evidence/'server.json').read_text())
        authority = info['url'].split('/')[2]
        secret = (evidence/'capability.secret').read_text()
        def req(path='/render', body=None, origin=None):
            c = http.client.HTTPConnection(authority, timeout=30)
            c.request('POST',path,body=None if body is None else json.dumps(body),headers={'Origin':origin or 'http://'+authority,'X-Spike-Capability':secret,'Content-Type':'application/json'})
            r=c.getresponse(); data=json.loads(r.read()); status=r.status;c.close()
            return status,data
        def args(value):
            s=value['authority']['result']; return {'expected_session_token':s['session_token'],'expected_revision':s['revision'],'expected_persistence_token':s['persistence']['persistence_token']}
        def save(value, extra=None): return req('/command',{'operation':'scene_save','arguments':{**args(value),**(extra or {})}})[1]
        _,before=req()
        if not granted:
            denied=save(before)
            assert denied['command']['error']['code']=='save_permission_required',denied['command']
            assert denied['scene']==before['scene'] and not (evidence/'disposable-destination').exists()
            (evidence/'result.json').write_text(json.dumps({'denied':denied['command'],'scene_unchanged':True,'destination_absent':True},indent=2))
            return
        state=before['authority']['result']; t=copy.deepcopy(before['scene']['objects'][0]['transform']);t['translation'][0]+=2
        _,edit=req('/command',{'operation':'scene_transform','arguments':{'expected_session_token':state['session_token'],'expected_revision':state['revision'],'object_id':before['scene']['objects'][0]['id'],'transform':t}})
        assert edit['command']['ok'] and edit['png']!=before['png']
        destination=evidence/'disposable-destination'/'fixture.json'
        sentinel=b'disposable occupied destination; must not be overwritten\n'
        destination.write_bytes(sentinel)
        occupied=save(edit)
        assert occupied['command']['error']['code']=='destination_exists', occupied['command']
        assert destination.read_bytes()==sentinel and occupied['scene']==edit['scene']
        destination.unlink()
        saved=save(edit);assert saved['command']['ok'],saved['command']
        destination=evidence/'disposable-destination'/'fixture.json'
        committed=destination.read_bytes(); digest=hashlib.sha256(committed).hexdigest()
        assert saved['scene']==edit['scene'] and saved['authority']['result']['undo_len']==1
        negatives=[]
        for label,extra in [('stale_revision',{'expected_revision':'0'}),('wrong_session',{'expected_session_token':'wrong'}),('stale_persistence',{'expected_persistence_token':'wrong'}),('arbitrary_destination',{'filename':'../../outside.json'}),('unknown_path',{'path':'/outside.json'})]:
            result=save(saved,extra);assert not result['command']['ok'];assert destination.read_bytes()==committed;assert result['scene']==saved['scene'];negatives.append({'case':label,'error':result['command']['error']})
        code,_=req('/command',{'operation':'scene_save','arguments':args(saved)},'http://evil.test');assert code==403 and destination.read_bytes()==committed
        _,undo=req('/command',{'operation':'scene_undo','arguments':{k:v for k,v in args(saved).items() if k!='expected_persistence_token'}})
        assert undo['command']['ok'] and undo['scene']['objects']==before['scene']['objects'] and undo['png']==before['png']
        assert destination.read_bytes()==committed,'Undo must not silently Save'
        _,reopened=req('/reopen-fixture')
        assert reopened['scene']==saved['scene'], 'Fresh process must load disk, not unsaved Undo state'
        (evidence/'saved.png').write_bytes(base64.b64decode(saved['png']))
        (evidence/'reopened.png').write_bytes(base64.b64decode(reopened['png']))
        assert reopened['png']==saved['png'],'fresh-process native pixels must match'
        assert reopened['authority']['result']['undo_len']==saved['authority']['result']['undo_len'], 'real persistence restores committed history'
        _,reopened_undo=req('/command',{'operation':'scene_undo','arguments':{k:v for k,v in args(reopened).items() if k!='expected_persistence_token'}})
        assert reopened_undo['command']['ok'] and reopened_undo['scene']['objects']==before['scene']['objects'] and reopened_undo['png']==before['png'], reopened_undo['command']
        assert destination.read_bytes()==committed
        processes=json.loads((evidence/'reopen-process.json').read_text());assert processes['old_pid']!=processes['new_pid'];assert not Path('/proc',str(processes['old_pid'])).exists()
        out={'occupied_destination_failure':occupied['command'],'saved_command':saved['command'],'negative_cases':negatives,'wrong_origin_status':code,'committed_sha256':digest,'png_sha256':hashlib.sha256(base64.b64decode(saved['png'])).hexdigest(),'scene_exact':True,'png_exact':True,'undo_exact':True,'fresh_process':processes,'scene':saved['scene']}
        (evidence/'result.json').write_text(json.dumps(out,indent=2))
    finally:
        host.terminate();host.wait(timeout=10);log.close()
        assert not (evidence/'capability.secret').exists()
        assert not (evidence/'disposable-destination').exists()
        assert not Path('/proc',str(info['native_pid'])).exists()
        if (evidence/'reopen-process.json').exists(): assert not Path('/proc',str(json.loads((evidence/'reopen-process.json').read_text())['new_pid'])).exists()
        gate=open('/home/akilf/.hermes/qa/rusterize/state/gate.lock','a');fcntl.flock(gate,fcntl.LOCK_EX|fcntl.LOCK_NB);gate.close()
        (evidence/'cleanup.json').write_text(json.dumps({'host_reaped':True,'native_reaped':True,'capability_removed':True,'destination_removed':True,'gate_reacquired':True},indent=2))

if __name__=='__main__':
    run(False);run(True)
    print('PASS denied Save; fixed granted Save; six unchanged-destination refusals; shared Undo; fresh-process exact scene/PNG; cleanup')
