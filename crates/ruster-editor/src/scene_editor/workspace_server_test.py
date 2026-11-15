"""Owning HTTP boundary test: ordinary child, no native scene or consent fixture."""
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time
import unittest


class WorkspaceHTTP(unittest.TestCase):
    def test_pending_cookie_origin_and_no_legacy_authority(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root:
            path = str(Path(root) / 'workspace.sock')
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(path)
            listener.listen(1)
            listener.settimeout(2)
            config = dict(socket=path, challenge='private-launch-challenge', native_pid=os.getpid(),
                          native_uid=os.getuid(), binding={'incarnation': 'native'}, attachment='one', html='<script nonce="NONCE"></script>')
            child = subprocess.Popen(['python3', '-u', str(Path(__file__).with_name('workspace_server.py'))], stdin=subprocess.PIPE, stderr=subprocess.PIPE)
            bridge = None
            try:
                child.stdin.write(json.dumps(config).encode() + b'\n')
                child.stdin.close()
                try:
                    bridge, _ = listener.accept()
                except TimeoutError:
                    self.fail('ordinary HTTP child did not authenticate its attachment: ' + child.stderr.read().decode())
                bridge.settimeout(2)
                def exact(n):
                    data=b''
                    while len(data)<n:
                        part=bridge.recv(n-len(data))
                        if not part: raise EOFError()
                        data+=part
                    return data
                def receive(): return json.loads(exact(struct.unpack('!I', exact(4))[0]))
                def send(v):
                    b=json.dumps(v).encode();bridge.sendall(struct.pack('!I',len(b))+b)
                hello=receive()
                self.assertEqual(hello['challenge'],config['challenge'])
                self.assertEqual(hello['binding'],config['binding'])
                send({'authenticated':True})
                ready=receive();self.assertEqual(ready['attachment'],'one')
                send({'published':True})
                host=ready['origin'].removeprefix('http://')
                import threading
                requests=[]
                def native():
                    try:
                        while True:
                            r=receive();requests.append(r)
                            send({'state':'pending','code':r.get('browser','')[:12]} if r['op']=='browser_pair' else {'live':False,'response':{'ok':False,'error':{'code':'browser_denied'}}})
                    except (EOFError,OSError): pass
                worker=threading.Thread(target=native,daemon=True);worker.start()
                def post(route, headers=None, body=''):
                    c=http.client.HTTPConnection(host,timeout=2)
                    c.request('POST',route,body,headers or {});r=c.getresponse();data=r.read();h=dict(r.getheaders());c.close();return r.status,data,h
                self.assertEqual(post('/pair',{'Origin':'http://wrong','X-Workspace-CSRF':ready['nonce']})[0],403)
                headers={'Origin':ready['origin'],'X-Workspace-CSRF':ready['nonce']}
                code,body,h=post('/pair',headers)
                self.assertEqual(code,200);self.assertEqual(json.loads(body)['state'],'pending')
                cookie=h['Set-Cookie'];self.assertIn('HttpOnly',cookie);self.assertIn('SameSite=Strict',cookie)
                self.assertEqual(post('/operation',{'Origin':ready['origin'],'X-Workspace-Capability':'anything'},json.dumps({'op':'refresh','args':{}}))[0],403)
                code,body,_=post('/status',{**headers,'Cookie':cookie.split(';')[0]})
                self.assertEqual(code,403)
                self.assertEqual(requests[0]['browser'],hashlib.sha256(cookie.split(';')[0].split('=',1)[1].encode()).hexdigest())
            finally:
                if bridge: bridge.close()
                listener.close()
                child.terminate();child.wait(timeout=3);child.stderr.close()


if __name__ == '__main__': unittest.main()
