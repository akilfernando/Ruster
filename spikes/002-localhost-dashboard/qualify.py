"""Real wire denial checks + own-process idle/active observations."""
import http.client,json,os,time
from pathlib import Path
from urllib.parse import urlparse
from measure_native import snapshot
ROOT=Path(__file__).resolve().parent

def main():
    r=json.loads((ROOT/'evidence/server.json').read_text());url=urlparse(r['url']);authority=url.netloc
    token=(ROOT/'evidence/capability.secret').read_text()
    good={'Host':authority,'Origin':'http://'+authority,'X-Spike-Capability':token,'Content-Length':'0'}
    cases=[]
    for name,headers,path,expected in [
        ('missing_secret',{k:v for k,v in good.items() if k!='X-Spike-Capability'},'/render',403),
        ('wrong_secret',{**good,'X-Spike-Capability':'wrong'},'/render',403),
        ('cross_origin',{**good,'Origin':'http://evil.test'},'/render',403),
        ('wrong_host',{**good,'Host':'evil.test'},'/render',403),
        ('missing_origin',{k:v for k,v in good.items() if k!='Origin'},'/render',403),
        ('no_edit_endpoint',good,'/edit',404),('no_undo_endpoint',good,'/undo',404),
        ('no_save_endpoint',good,'/save',404)]:
        conn=http.client.HTTPConnection(url.hostname,url.port,timeout=30);conn.request('POST',path,headers=headers)
        response=conn.getresponse();response.read();conn.close();assert response.status==expected,(name,response.status)
        cases.append({'case':name,'status':response.status})
    pids={'native':r['native_pid'],'host':r['pid']}
    before={name:snapshot(pid) for name,pid in pids.items()};start=time.perf_counter();time.sleep(3)
    after={name:snapshot(pid) for name,pid in pids.items()};idle={'seconds':time.perf_counter()-start,'before':before,'after':after}
    before={name:snapshot(pid) for name,pid in pids.items()};start=time.perf_counter();rows=[]
    for i in range(10):
        conn=http.client.HTTPConnection(url.hostname,url.port,timeout=30);conn.request('POST','/render',headers=good)
        response=conn.getresponse();data=json.loads(response.read());conn.close();assert response.status==200
        assert data['metrics']['passes']==1 and data['metrics']['revision']==0
        rows.append(data['metrics'])
    after={name:snapshot(pid) for name,pid in pids.items()};active={'seconds':time.perf_counter()-start,'before':before,'after':after,'rows':rows}
    assert all(b['frames']==a['frames']+1 for a,b in zip(rows,rows[1:]))
    result={'denials':cases,'idle':idle,'active':active,'hz':os.sysconf('SC_CLK_TCK')}
    (ROOT/'evidence/wire-and-resources.json').write_text(json.dumps(result,indent=2));print(json.dumps({'denials':cases,'idle':idle,'active_seconds':active['seconds']}))
if __name__=='__main__':main()
