"""Owning HTTP boundary test: ordinary child, no native scene or consent fixture."""
import hashlib
import http.client
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time
import unittest


class WorkspaceHTTP(unittest.TestCase):
    def test_cleanup_preserves_replaced_endpoint(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('owned_workspace_server',Path(__file__).with_name('workspace_server.py'))
        module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root:
            path=Path(root)/'workspace.sock'
            with socket.socket(socket.AF_UNIX) as listener:
                listener.bind(str(path));path.chmod(0o600)
                identity=module.endpoint_identity(path)
                path.unlink();path.write_text('replacement must survive');path.chmod(0o600)
                module.cleanup_endpoint(path,identity)
                self.assertEqual(path.read_text(),'replacement must survive')

    def test_wrong_native_os_peer_gets_no_challenge_or_http_endpoint(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root:
            for field in ('native_pid', 'native_uid'):
                path = str(Path(root) / (field + '.sock'))
                with socket.socket(socket.AF_UNIX) as listener:
                    listener.bind(path); listener.listen(1); listener.settimeout(2)
                    config = {'socket':path, 'native_pid':os.getpid(), 'native_uid':os.getuid()}
                    config[field] += 1
                    child = subprocess.Popen([sys.executable, *(['-O'] if sys.flags.optimize else []), '-u', str(Path(__file__).with_name('workspace_server.py'))], stdin=subprocess.PIPE, stderr=subprocess.PIPE)
                    assert child.stdin is not None and child.stderr is not None
                    try:
                        child.stdin.write(json.dumps(config).encode() + b'\n'); child.stdin.close()
                        peer, _ = listener.accept()
                        with peer:
                            peer.settimeout(2)
                            self.assertEqual(peer.recv(1), b'', 'wrong native peer must receive no launch challenge')
                        self.assertNotEqual(child.wait(timeout=2), 0)
                        self.assertIn('native peer mismatch', child.stderr.read().decode())
                    finally:
                        if child.poll() is None: child.terminate(); child.wait(timeout=2)
                        child.stderr.close()

    def test_pending_cookie_origin_and_no_legacy_authority(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root:
            path = str(Path(root) / 'workspace.sock')
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(path)
            os.chmod(path,0o600)
            listener.listen(1)
            listener.settimeout(2)
            config = dict(socket=path, challenge='private-launch-challenge', native_pid=os.getpid(),
                          native_uid=os.getuid(), binding={'incarnation': 'native'}, attachment='one', html='<script nonce="NONCE"></script>')
            child = subprocess.Popen([sys.executable, *(['-O'] if sys.flags.optimize else []), '-u', str(Path(__file__).with_name('workspace_server.py'))], stdin=subprocess.PIPE, stderr=subprocess.PIPE)
            assert child.stdin is not None and child.stderr is not None
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
                for wrong in ({**headers,'Host':'127.0.0.1:1'}, {**headers,'X-Workspace-CSRF':'wrong'}, {'Origin':ready['origin']}):
                    self.assertEqual(post('/pair',wrong)[0],403)
                self.assertEqual(requests,[], 'HTTP boundary refusal cannot reach native dispatch')
                code,body,h=post('/pair',headers)
                self.assertEqual(code,200);self.assertEqual(json.loads(body)['state'],'pending')
                cookie=h['Set-Cookie'];self.assertIn('HttpOnly',cookie);self.assertIn('SameSite=Strict',cookie)
                self.assertEqual(post('/operation',{'Origin':ready['origin'],'X-Workspace-Capability':'anything'},json.dumps({'op':'refresh','args':{}}))[0],403)
                code,body,_=post('/status',{**headers,'Cookie':cookie.split(';')[0]})
                self.assertEqual(code,403)
                self.assertEqual(requests[0]['browser'],hashlib.sha256(cookie.split(';')[0].split('=',1)[1].encode()).hexdigest())
                bridge.shutdown(socket.SHUT_RDWR); bridge.close(); bridge=None
                self.assertEqual(child.wait(timeout=3),0)
                self.assertFalse(Path(path).exists(), 'native EOF must remove the exact workspace endpoint')
            finally:
                if bridge: bridge.close()
                listener.close()
                child.terminate();child.wait(timeout=3);child.stderr.close()


if __name__ == '__main__': unittest.main()
