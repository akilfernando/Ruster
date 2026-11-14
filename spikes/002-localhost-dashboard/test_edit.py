"""Live acceptance: the old read-only host must fail this new contract."""
import http.client,json
from pathlib import Path
from urllib.parse import urlparse
ROOT=Path(__file__).resolve().parent

def request(path, body=None, overrides=None):
    r=json.loads((ROOT/'evidence/guarded-edit/server.json').read_text());u=urlparse(r['url'])
    h={'Origin':'http://'+u.netloc,'X-Spike-Capability':(ROOT/'evidence/guarded-edit/capability.secret').read_text(),'Content-Type':'application/json'}
    h.update(overrides or {})
    c=http.client.HTTPConnection(u.netloc,timeout=30)
    c.request('POST',path,body=json.dumps(body).encode() if body is not None else None,headers=h)
    r=c.getresponse();data=json.loads(r.read());c.close();return r.status,data

def main():
    _,before=request('/render')
    assert before['metrics']['read_only'] is False,'guarded editing not implemented'
    state=before['authority']['result'];base={'expected_session_token':state['session_token'],'expected_revision':state['revision']}
    t=before['scene']['objects'][0]['transform'].copy();t['translation']=t['translation'].copy();t['translation'][0]+=2
    args={**base,'object_id':before['scene']['objects'][0]['id'],'transform':t}
    _,edit=request('/command',{'operation':'scene_transform','arguments':args})
    assert edit['command']['ok'],edit
    revision=before['scene']['revision']
    assert edit['scene']['revision']==revision+1 and edit['metrics']['revision']==revision+1
    assert edit['scene']['objects'][0]['transform']==t
    assert edit['png']!=before['png']
    results=[]
    for name,body,headers,code in [('stale',{'operation':'scene_transform','arguments':args},{},200),('wrong_session',{'operation':'scene_undo','arguments':{**base,'expected_session_token':'wrong','expected_revision':str(revision+1)}},{},200),('wrong_origin',{'operation':'scene_undo','arguments':{**base,'expected_revision':str(revision+1)}},{'Origin':'http://evil.test'},403),('unsupported',{'operation':'scene_save','arguments':{}},{},200)]:
        status,value=request('/command',body,headers);assert status==code
        if status==200:assert not value['command']['ok'],value
        _,check=request('/render');assert check['scene']==edit['scene'];assert check['authority']['result']['undo_len']==1
        results.append({'case':name,'status':status,'response':value.get('command',value)})
    _,undo=request('/command',{'operation':'scene_undo','arguments':{**base,'expected_revision':str(revision+1)}})
    assert undo['command']['ok'] and undo['scene']['revision']==revision+2
    assert undo['scene']['objects']==before['scene']['objects']
    assert undo['png']==before['png'],'Undo must restore exact native pixels'
    assert undo['authority']['result']['undo_len']==0
    out={'negative_cases':results,'before':before,'edit':edit,'undo':undo}
    (ROOT/'evidence/guarded-edit/edit-journey.json').write_text(json.dumps(out,indent=2))
    print('PASS guarded edit -> native frame -> shared Undo; four negative cases unchanged')
if __name__=='__main__':main()
