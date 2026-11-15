#!/usr/bin/env python3
"""Disposable localhost interaction fixture. NOT production consent/onboarding."""
import argparse
import fcntl
import hmac
import json
import os
from pathlib import Path
import secrets
import signal
import socket

import subprocess
import tempfile
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from bridge import send, receive, authenticate
from metrics import ProcessMetrics

ROOT = Path(__file__).resolve().parent

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--host-binary', type=Path, required=True)
    p.add_argument('--output-root', type=Path, required=True)
    p.add_argument('--grant-disposable-local-and-first-mcp-owner', action='store_true', required=True)
    p.add_argument('--validation-diagnostics', action='store_true')
    a = p.parse_args()
    assert a.host_binary.is_absolute() and a.host_binary.is_file()
    assert a.output_root.is_absolute() and a.output_root.is_dir()
    gate = open('/home/akilf/.hermes/qa/rusterize/state/gate.lock', 'a')
    fcntl.flock(gate, fcntl.LOCK_EX | fcntl.LOCK_NB)
    run = Path(tempfile.mkdtemp(prefix='interactive-', dir=a.output_root))
    os.chmod(run, 0o700)
    secret = run/'capability.secret'
    token = secrets.token_urlsafe(32)
    secret.write_text(token); secret.chmod(0o600)
    nonce = secrets.token_urlsafe(20)
    socket_root = Path(tempfile.mkdtemp(prefix='rb-', dir=os.environ['TMPDIR']))
    socket_root.chmod(0o700)
    socket_path = socket_root/'bridge.sock'
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(str(socket_path)); socket_path.chmod(0o600)
    listener.listen(1); listener.settimeout(20)
    bridge_token = secrets.token_urlsafe(32)
    bridge = None
    env = {k:v for k,v in os.environ.items() if not k.startswith(('RUSTER_', 'VK_', 'EGUI_INSPECTION'))}
    env.update(PREVIEW_FIXTURE_DIR=str(run), INTERACTIVE_LOCALHOST_FIXTURE='1',
               RUSTER_RECOVERY_DIR=str(run/'recovery'), RUSTER_EDITOR_STATE=str(run/'settings.json'),
               LOCAL_BRIDGE_SOCKET=str(socket_path), LOCAL_BRIDGE_TOKEN=bridge_token,
               LOCAL_BRIDGE_FIXTURE_FIRST_OWNER='1')
    if a.validation_diagnostics:
        layer = Path('/home/akilf/.local/opt/vulkan-validation-layers-1.4.363.1-1/scoped-layer')
        assert layer.is_dir(), 'scoped validation layer required; no global fallback'
        env.update(VK_LAYER_PATH=str(layer), VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation', VK_LAYER_VALIDATE_SYNC='1', VK_LOADER_DEBUG='layer', VK_LAYER_REPORT_FLAGS='error,warn,info,perf,verbose', VK_LAYER_DEBUG_ACTION='VK_DBG_LAYER_ACTION_LOG_MSG')
    log = (run/'native.log').open('w')
    host = subprocess.Popen([str(a.host_binary), '--ignored', '--exact', 'scene_editor::preview_probe::direct_image_host_fixture', '--nocapture'], env=env, cwd=run, stdout=log, stderr=subprocess.STDOUT)
    server = None
    def state():
        return json.loads((run/'state.json').read_text())
    def local(request):
        nonlocal bridge
        started = time.perf_counter()
        assert host.poll() is None, 'native owner stopped'
        assert bridge is not None, 'bridge permanently closed; no reconnect/replay'
        try:
            deadline = time.monotonic()+8
            send(bridge, request, deadline)
            reply = receive(bridge, deadline)
        except Exception:
            bridge.close(); bridge = None
            raise
        if request['op'] == 'status': return reply
        reply['request_ms'] = (time.perf_counter()-started)*1000
        if reply['response'].get('error', {}).get('code') == 'revoked':
            return {'response':reply['response']}
        return reply
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format, *args): pass
        def reply(self, code, body, kind='application/json'):
            self.send_response(code)
            for k,v in {'Content-Type':kind, 'Content-Length':str(len(body)), 'Cache-Control':'no-store', 'Referrer-Policy':'no-referrer', 'X-Content-Type-Options':'nosniff', 'Content-Security-Policy':"default-src 'none'; img-src data:; connect-src 'self'; style-src 'unsafe-inline'; script-src 'nonce-"+nonce+"'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"}.items(): self.send_header(k,v)
            self.end_headers(); self.wfile.write(body)
        def do_GET(self):
            if self.path != '/' or self.headers.get('Host') != authority: return self.reply(403,b'{}')
            self.reply(200, (ROOT/'workspace.html').read_text().replace('/* MEASUREMENT_SCRIPT */', (ROOT/'measurement.js').read_text()).replace('NONCE',nonce).encode(), 'text/html; charset=utf-8')
        def do_POST(self):
            if self.headers.get('Host') != authority or self.headers.get('Origin') != 'http://'+authority or not hmac.compare_digest(self.headers.get('X-Workspace-Capability',''),token): return self.reply(403,b'{"error":"denied"}')
            if self.path == '/metrics':
                if host.poll() is not None or bridge is None:
                    return self.reply(503, b'{"error":"owner stopped"}')
                try:
                    return self.reply(200, json.dumps(metrics.sample()).encode())
                except (OSError, ValueError):
                    return self.reply(503, b'{"error":"owned metrics unavailable"}')
            if self.path == '/status':
                # Same capability/origin boundary; observe only this exact native owner.
                # Never dispatch a render or consume an MCP adapter owner for polling.
                if host.poll() is not None: return self.reply(503,b'{"error":"native owner stopped"}')
                try:
                    observed = local({'op':'status','args':{}})
                    if not observed['live']: return self.reply(403,b'{"error":"revoked"}')
                    return self.reply(200,json.dumps(observed).encode())
                except (OSError, ValueError, KeyError, EOFError, AssertionError):
                    return self.reply(503,b'{"error":"status unavailable"}')
            if self.path != '/operation': return self.reply(404,b'{}')
            try:
                size = int(self.headers.get('Content-Length','0'))
                if self.headers.get('Transfer-Encoding') or not 0 < size <= 4096: return self.reply(400,b'{}')
                request = json.loads(self.rfile.read(size))
                if not isinstance(request,dict) or set(request) != {'op','args'}: return self.reply(400,b'{}')
                reply = local(request)
                self.reply(200,json.dumps(reply).encode())
            except Exception as exc:
                (run/'failure.txt').write_text(repr(exc))
                self.reply(503,b'{"error":"unknown outcome; do not retry; inspect owned evidence"}')
    class BoundedServer(HTTPServer):
        def get_request(self):
            sock, address = super().get_request(); sock.settimeout(2); return sock,address
    def stop(*args): raise KeyboardInterrupt
    signal.signal(signal.SIGTERM,stop)
    try:
        deadline=time.monotonic()+20
        while not (run/'state.json').exists():
            assert host.poll() is None and time.monotonic()<deadline, 'native startup failed'
            time.sleep(.02)
        bridge, _ = listener.accept()
        incarnation = json.loads(Path(state()['locator']).read_text())['incarnation']
        peer_pid = authenticate(bridge, host.pid, bridge_token, incarnation)
        listener.close()
        metrics = ProcessMetrics(host.pid)
        server=BoundedServer(('127.0.0.1',0),Handler); server.timeout=.1
        authority='127.0.0.1:'+str(server.server_port)
        info={'url':'http://'+authority+'/', 'run':str(run), 'pid':os.getpid(), 'native_pid':host.pid,'locator':state()['locator'], 'fixture_authority_only':True, 'transport':'persistent-unix-stream', 'socket_path':str(socket_path), 'peer_pid':peer_pid, 'incarnation':incarnation}
        (run/'server.json').write_text(json.dumps(info,indent=2)); print(json.dumps(info),flush=True)
        while host.poll() is None:
            server.handle_request()
    except KeyboardInterrupt: pass
    finally:
        if server: server.server_close()
        if bridge: bridge.close()
        listener.close()
        (run/'stop').touch()
        try: host.wait(timeout=8)
        except subprocess.TimeoutExpired: host.terminate(); host.wait(timeout=5)
        socket_path.unlink(missing_ok=True); socket_root.rmdir()
        secret.unlink(missing_ok=True); log.close(); gate.close()
        locator=Path(state()['locator']) if (run/'state.json').exists() else None
        (run/'server-cleanup.json').write_text(json.dumps({'host_exit':host.returncode,'capability_absent':not secret.exists(),'locator_absent':locator is None or not locator.exists(),'http_closed':True,'socket_absent':not socket_path.exists(),'socket_root_absent':not socket_root.exists()}))

if __name__ == '__main__': main()
