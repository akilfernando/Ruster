"""Explicit disposable guarded edit/Undo host; no Save or production consent."""
import argparse
import base64
import fcntl
import hmac
import json
import os
from pathlib import Path
import secrets
import select
import signal
import time
import subprocess
from http.server import HTTPServer, BaseHTTPRequestHandler

ROOT = Path(__file__).resolve().parent

def authorized(headers, authority, token):
    return (headers.get('Host') == authority and
            headers.get('Origin') == 'http://' + authority and
            hmac.compare_digest(headers.get('X-Spike-Capability', ''), token))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--disposable-fixture', action='store_true', required=True)
    parser.add_argument('--grant-disposable-edit-undo', action='store_true', required=True)
    args = parser.parse_args()
    gate = open('/home/akilf/.hermes/qa/rusterize/state/gate.lock', 'a')
    fcntl.flock(gate, fcntl.LOCK_EX | fcntl.LOCK_NB)  # Refuse overlapping native QA.
    run = ROOT / 'evidence' / 'guarded-edit'
    run.mkdir(parents=True, exist_ok=True)
    token = secrets.token_urlsafe(32)
    secret = run / 'capability.secret'
    fd = os.open(secret, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as file: file.write(token)
    nonce = secrets.token_urlsafe(20)
    env = {k:v for k,v in os.environ.items() if not k.startswith(('RUSTER_', 'EGUI_INSPECTION'))}
    # Scoped installed layer, explicitly selected; activation is separately verified.
    env.update(VK_LAYER_PATH='/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer',
               VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation', VK_LAYER_VALIDATE_SYNC='1',
               VK_LOADER_DEBUG='layer', RUSTER_MSAA_SAMPLES='4')
    log = (run / 'native.log').open('w')
    native = subprocess.Popen([str(ROOT/'target/release/ruster-localhost-spike'), str(run), '--grant-disposable-edit-undo'],
                              stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, env=env, text=True)
    assert native.stdin is not None and native.stdout is not None
    def stop(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, stop)
    def render(request=None):
        started = time.perf_counter()
        native.stdin.write((json.dumps(request) if request is not None else 'render')+'\n'); native.stdin.flush()
        if not select.select([native.stdout], [], [], 30)[0]: raise TimeoutError('native render timeout')
        line = native.stdout.readline()
        if not line: raise RuntimeError('native host exited; see native.log')
        result = json.loads(line)
        result['host_roundtrip_ms'] = (time.perf_counter()-started)*1000
        (run/'metrics.json').write_text(json.dumps(result, indent=2))
        return result
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format, *args): pass  # Never log credentials/requests.
        def reply(self, code, body, kind='application/json'):
            self.send_response(code)
            self.send_header('Content-Type', kind)
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Cache-Control', 'no-store')
            self.send_header('X-Content-Type-Options','nosniff')
            self.send_header('Referrer-Policy','no-referrer')
            self.send_header('Content-Security-Policy', "default-src 'none'; img-src data:; connect-src 'self'; style-src 'unsafe-inline'; script-src 'nonce-"+nonce+"'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'")
            self.end_headers(); self.wfile.write(body)
        def do_GET(self):
            if self.headers.get('Host') != authority or self.path != '/':
                return self.reply(403,b'{}')
            self.reply(200, (ROOT/'dashboard.html').read_text().replace('NONCE',nonce).encode(), 'text/html; charset=utf-8')
        def do_POST(self):
            if not authorized(self.headers,authority,token): return self.reply(403,b'{"error":"denied"}')
            if self.headers.get('Transfer-Encoding'): return self.reply(400,b'{}')
            if self.path not in ('/render','/command'): return self.reply(404,b'{}')
            try:
                length = int(self.headers.get('Content-Length','0'))
                if length < 0 or length > 4096 or (self.path == '/render' and length):
                    return self.reply(400,b'{}')
                request = json.loads(self.rfile.read(length)) if self.path == '/command' else None
                if self.path == '/command' and not isinstance(request,dict): return self.reply(400,b'{}')
                metrics = render(request)
                self.reply(200,json.dumps({'metrics':metrics,'authority':metrics['authority'],'command':metrics['command'],'scene':json.loads((run/'scene.json').read_text()),
                    'png':base64.b64encode((run/'preview.png').read_bytes()).decode()}).encode())
            except Exception:
                self.reply(503,b'{"error":"native host failed; inspect local evidence"}')
    class BoundedHTTPServer(HTTPServer):
        def get_request(self):
            connection, address = super().get_request()
            connection.settimeout(2)  # Browser idle preconnect cannot hold owner forever.
            return connection, address
    server = BoundedHTTPServer(('127.0.0.1',0),Handler)
    server.timeout=1
    authority = '127.0.0.1:' + str(server.server_port)
    try:
        initial = render()
        (run/'server.json').write_text(json.dumps({'url':'http://'+authority+'/', 'pid':os.getpid(), 'native_pid':native.pid,'mode':'explicit startup edit/Undo disposable fixture','initial':initial},indent=2))
        print('Dashboard: http://'+authority+'/',flush=True)
        server.serve_forever()
    finally:
        server.server_close()
        if native.poll() is None:
            native.stdin.close()
            try: native.wait(timeout=5)
            except subprocess.TimeoutExpired: native.kill(); native.wait()
        secret.unlink(missing_ok=True)
        log.close(); gate.close()

if __name__ == '__main__': main()
