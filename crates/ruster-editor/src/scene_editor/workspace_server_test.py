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
    def companion(self, root, reply=None):
        import threading
        path=Path(root)/'workspace.sock'
        listener=socket.socket(socket.AF_UNIX);listener.bind(str(path));path.chmod(0o600);listener.listen(1);listener.settimeout(2)
        self.addCleanup(listener.close)
        child=subprocess.Popen([sys.executable,*(['-O'] if sys.flags.optimize else []),'-u',str(Path(__file__).with_name('workspace_server.py'))],stdin=subprocess.PIPE,stderr=subprocess.PIPE)
        def cleanup():
            if child.poll() is None: child.terminate();child.wait(timeout=3)
            child.stderr.close()
        self.addCleanup(cleanup)
        config=dict(socket=str(path),native_pid=os.getpid(),native_uid=os.getuid(),challenge='test',binding={},attachment='test',html='test')
        child.stdin.write(json.dumps(config).encode()+b'\n');child.stdin.close()
        bridge,_=listener.accept();bridge.settimeout(3);self.addCleanup(bridge.close)
        def exact(n):
            data=b''
            while len(data)<n:
                part=bridge.recv(n-len(data))
                if not part: raise EOFError()
                data+=part
            return data
        def receive(): return json.loads(exact(struct.unpack('!I',exact(4))[0]))
        def send(value):
            b=json.dumps(value).encode();bridge.sendall(struct.pack('!I',len(b))+b)
        receive();send({'authenticated':True});ready=receive();send({'published':True})
        requests=[]
        def native():
            try:
                while True:
                    request=receive();requests.append(request)
                    send({'state':'approved'} if request['op']=='browser_pair' else reply(request))
            except (OSError,EOFError): pass
        threading.Thread(target=native,daemon=True).start()
        host=ready['origin'].removeprefix('http://')
        headers={'Origin':ready['origin'],'X-Workspace-CSRF':ready['nonce']}
        def post(route,body=''):
            connection=http.client.HTTPConnection(host,timeout=3)
            connection.request('POST',route,body,headers)
            response=connection.getresponse();data=response.read();cookie=response.getheader('Set-Cookie');connection.close()
            return response.status,json.loads(data),cookie
        _,_,cookie=post('/pair');headers['Cookie']=cookie.split(';')[0]
        return child,bridge,host,headers,post,requests

    def test_trickling_requests_have_absolute_deadline_and_native_eof_cleanup(self):
        for body in (False,True):
            for native_eof in (False,True):
                with self.subTest(body=body,native_eof=native_eof), tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as root:
                    child,bridge,host,headers,_,_=self.companion(root)
                    client=socket.create_connection(tuple([host.split(':')[0],int(host.split(':')[1])]),timeout=1)
                    self.addCleanup(client.close)
                    if body:
                        wire='POST /operation HTTP/1.0\r\nHost: '+host+'\r\n'+''.join(k+': '+v+'\r\n' for k,v in headers.items())+'Content-Length: 4096\r\n\r\n{'
                    else: wire='GET / HTTP/1.0\r\nHost: '+host+'\r\nX-Unfinished: '
                    client.sendall(wire.encode());started=time.monotonic();closed=False
                    time.sleep(.15)
                    if native_eof: bridge.shutdown(socket.SHUT_RDWR);bridge.close()
                    until=started+(1.2 if native_eof else 2.8)
                    while time.monotonic()<until:
                        try:
                            client.sendall(b' ')
                            client.settimeout(.05)
                            if client.recv(8192)==b'': closed=True;break
                        except TimeoutError: pass
                        except OSError: closed=True;break
                        time.sleep(.05)
                    self.assertTrue(closed,'byte progress must not extend the absolute HTTP admission deadline or mask native EOF')
                    if native_eof:
                        self.assertEqual(child.wait(timeout=.5),0)
                        self.assertFalse(Path(root).exists(),'exact socket and empty owner root must retire while parsing')
                        with self.assertRaises(OSError): socket.create_connection((host.split(':')[0],int(host.split(':')[1])),timeout=.2)
                    else:
                        self.assertIsNone(child.poll(),'HTTP timeout alone must not kill a healthy attachment')
                        bridge.shutdown(socket.SHUT_RDWR);bridge.close();self.assertEqual(child.wait(timeout=1),0)

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
