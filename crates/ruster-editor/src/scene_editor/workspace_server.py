"""Host-owned loopback HTTP companion. No MCP client, consent or native owner claim."""
import atexit
import hashlib
import io
from http.cookies import SimpleCookie
from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import os
from pathlib import Path
import stat
import secrets
import select
import socket
import struct
import sys
import time

LIMIT = 65536
# One absolute budget from accept through request line, headers and body. Native
# operations retain their separate 2s send / 8s reply / 6s capture-poll budgets.
HTTP_ADMISSION_SECONDS = 2


def receive(sock, deadline, limit=LIMIT):
    def exact(size):
        result = bytearray()
        while len(result) < size:
            left = deadline-time.monotonic()
            if left <= 0: raise TimeoutError('absolute frame deadline')
            sock.settimeout(left)
            chunk = sock.recv(size-len(result))
            if not chunk: raise EOFError('native EOF; never reconnect or replay')
            result.extend(chunk)
        return result
    size = struct.unpack('!I', exact(4))[0]
    if not 0 < size <= limit: raise ValueError('frame length')
    return json.loads(exact(size))


def send(sock, value, deadline):
    body = json.dumps(value, separators=(',', ':')).encode()
    if not 0 < len(body) <= LIMIT: raise ValueError('frame length')
    data = memoryview(struct.pack('!I', len(body))+body)
    while data:
        left = deadline-time.monotonic()
        if left <= 0: raise TimeoutError('absolute frame deadline')
        sock.settimeout(left)
        n = sock.send(data)
        if not n: raise EOFError('native EOF')
        data = data[n:]


def endpoint_identity(path):
    directory = path.parent.lstat()
    endpoint = path.lstat()
    if not stat.S_ISDIR(directory.st_mode) or stat.S_IMODE(directory.st_mode) != 0o700 or directory.st_uid != os.getuid() or not stat.S_ISSOCK(endpoint.st_mode) or stat.S_IMODE(endpoint.st_mode) != 0o600 or endpoint.st_uid != os.getuid():
        raise ValueError('private native workspace endpoint required')
    return directory.st_dev, directory.st_ino, endpoint.st_dev, endpoint.st_ino


def cleanup_endpoint(path, identity):
    # Native EOF also occurs on Host death, when Rust destructors cannot run.
    # Remove only the original private socket and an otherwise empty owner root.
    try:
        if endpoint_identity(path) != identity: return
        path.unlink()
        path.parent.rmdir()
    except (OSError, ValueError):
        pass


def main():
    config = json.loads(sys.stdin.buffer.readline(256*1024))
    bridge = socket.socket(socket.AF_UNIX)
    bridge.connect(config['socket'])
    pid, uid, _ = struct.unpack('3i', bridge.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
    if (pid, uid) != (config['native_pid'], config['native_uid']): raise ValueError('native peer mismatch')
    endpoint = Path(config['socket'])
    atexit.register(cleanup_endpoint, endpoint, endpoint_identity(endpoint))
    send(bridge, {'challenge':config['challenge'], 'binding':config['binding'], 'attachment':config['attachment']}, time.monotonic()+2)
    if receive(bridge, time.monotonic()+2) != {'authenticated':True}: raise ValueError('attachment refused')
    nonce = secrets.token_urlsafe(20)
    browser_sessions = set()
    cookie_name = 'ruster_browser_'+secrets.token_hex(8)
    broken = False

    def local(request):
        nonlocal broken
        if broken: raise EOFError('attachment retired')
        try:
            send(bridge, request, time.monotonic()+2)
            return receive(bridge, time.monotonic()+8)
        except BaseException:
            broken = True
            bridge.close()
            raise

    def refresh(binding):
        scene = local({'op':'scene_inspect','args':{},**binding})
        if not scene.get('ok'): return {'response':scene}
        view = local({'op':'viewport_inspect','args':{},**binding})
        if not view.get('ok'): return {'response':view}
        v = view['result']
        stale = {'response':{'ok':False,'error':{'code':'stale_observation'}}}
        if any(scene['result'].get(key) != v.get(key) or v.get(key) is None for key in ('session_token','revision')):
            return stale
        started = local({'op':'viewport_capture_start','args':{'expected_session_token':v['session_token'],'expected_revision':v['revision'],'expected_view_revision':v['view_revision']},**binding})
        if not started.get('ok'): return {'response':started}
        deadline = time.monotonic()+6
        while True:
            captured = local({'op':'viewport_capture_inspect','args':{'capture_id':started['result']['capture_id']},**binding})
            if not captured.get('ok'): return {'response':captured}
            capture = captured['result']
            if capture['status'] != 'pending': break
            if time.monotonic() >= deadline: raise TimeoutError('capture deadline; never replay')
            time.sleep(.04)
        if capture['status'] != 'ready': return {'response':{'ok':False,'error':{'code':capture.get('reason','capture_failed')}}}
        if any(capture.get(key) != v.get(key) or v.get(key) is None for key in ('session_token','revision','view_revision')) or capture.get('capture_id') != started['result']['capture_id']:
            return stale
        return {'response':{'ok':True},'scene':scene['result'],'view':v,'png':capture['image_base64'],'native_frame':capture['capture_id'],'capture':{k:v for k,v in capture.items() if k!='image_base64'},'draft_reserved':False}

    class AdmissionReader(io.RawIOBase):
        def __init__(self, sock):
            self.sock = sock
            self.deadline = time.monotonic()+HTTP_ADMISSION_SECONDS
        def readable(self): return True
        def readinto(self, buffer):
            nonlocal broken
            left = self.deadline-time.monotonic()
            if left <= 0: raise TimeoutError('absolute HTTP admission deadline')
            ready, _, _ = select.select([self.sock, bridge], [], [], left)
            if bridge in ready:
                # No native command is in flight during HTTP admission. Readability
                # is EOF or unsolicited data: either retires this exact attachment.
                broken = True
                raise ConnectionResetError('native attachment retired during HTTP admission')
            if self.sock not in ready: raise TimeoutError('absolute HTTP admission deadline')
            return self.sock.recv_into(buffer)

    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.rfile.close()
            self.rfile = io.BufferedReader(AdmissionReader(self.connection))
        def log_message(self, *_): pass
        def reply(self, code, body, kind='application/json', cookie=None):
            self.send_response(code)
            headers = {'Content-Type':kind,'Content-Length':str(len(body)),'Cache-Control':'no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff','Content-Security-Policy':"default-src 'none'; img-src data:; connect-src 'self'; style-src 'unsafe-inline'; script-src 'nonce-"+nonce+"'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"}
            for k,v in headers.items(): self.send_header(k,v)
            if cookie: self.send_header('Set-Cookie',cookie)
            self.end_headers();self.wfile.write(body)
        def do_GET(self):
            if self.path != '/' or self.headers.get('Host') != authority: return self.reply(403,b'{}')
            self.reply(200,config['html'].replace('NONCE',nonce).encode(),'text/html; charset=utf-8')
        def do_POST(self):
            if self.headers.get('Host') != authority or self.headers.get('Origin') != origin or self.headers.get('X-Workspace-CSRF') != nonce: return self.reply(403,b'{"error":"denied"}')
            try:
                cookies=SimpleCookie();cookies.load(self.headers.get('Cookie',''))
                credential=cookies[cookie_name].value if cookie_name in cookies else ''
                browser_id=hashlib.sha256(credential.encode()).hexdigest() if credential in browser_sessions else None
                if self.path=='/pair':
                    cookie=None
                    if browser_id is None:
                        if len(browser_sessions)>=8: return self.reply(403,b'{}')
                        credential=secrets.token_urlsafe(32);browser_sessions.add(credential)
                        browser_id=hashlib.sha256(credential.encode()).hexdigest()
                        cookie=cookie_name+'='+credential+'; HttpOnly; SameSite=Strict; Path=/'
                    paired=local({'op':'browser_pair','args':{},'browser':browser_id,'origin':origin})
                    return self.reply(200,json.dumps(paired).encode(),cookie=cookie)
                if browser_id is None: return self.reply(403,b'{"error":"denied"}')
                binding={'browser':browser_id,'origin':origin}
                if self.path=='/status':
                    observed=local({'op':'status','args':{},**binding})
                    return self.reply(200 if observed.get('live') else 403,json.dumps(observed).encode())
                if self.path!='/operation': return self.reply(404,b'{}')
                size=int(self.headers.get('Content-Length','0'))
                if self.headers.get('Transfer-Encoding') or not 0<size<=4096: return self.reply(400,b'{}')
                request=json.loads(self.rfile.read(size))
                if not isinstance(request,dict) or set(request)!={'op','args'}: return self.reply(400,b'{}')
                if request['op']=='refresh': reply=refresh(binding)
                elif request['op'] in ('viewport_camera','viewport_fit','viewport_pick','scene_select') and isinstance(request['args'],dict):
                    result=local({**request,**binding})
                    reply=refresh(binding) if result.get('ok') else {'response':result}
                else: reply={'response':{'ok':False,'error':{'code':'forbidden_operation'}}}
                self.reply(200,json.dumps(reply).encode())
            except (OSError,ValueError,KeyError,EOFError,TimeoutError):
                self.reply(503,b'{"error":"attachment unavailable; no replay"}')

    class BoundedServer(HTTPServer):
        def get_request(self):
            sock,address=super().get_request();sock.settimeout(2);return sock,address
    server=BoundedServer(('127.0.0.1',0),Handler);server.timeout=.1
    authority='127.0.0.1:'+str(server.server_port);origin='http://'+authority
    send(bridge,{'attachment':config['attachment'],'origin':origin,'nonce':nonce},time.monotonic()+2)
    if receive(bridge,time.monotonic()+2)!={'published':True}: raise ValueError('publication refused')
    try:
        while not broken:
            # No commands are in flight here. Native EOF retires HTTP even idle.
            if select.select([bridge],[],[],0)[0]: break
            server.handle_request()
    finally:
        server.server_close();bridge.close()


if __name__=='__main__': main()
