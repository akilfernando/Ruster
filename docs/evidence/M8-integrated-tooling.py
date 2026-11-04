#!/usr/bin/env python3
"""Probe actual generated working-copy types through a real LuaLS stdio process.

No source/config writes, installers, GUI input or global settings. Output must be
outside the workspace; raw protocol messages and server diagnostics are retained.
This verifies LuaLS semantics, not focused keyboard interaction in Zed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
from urllib.parse import unquote, urlparse


class Client:
    def __init__(self, server, workspace, output, config):
        self.workspace, self.config, self.trace = workspace, config, []
        self.messages, self.serial = queue.Queue(), 0
        self.stderr = (output / 'server-stderr.log').open('wb')
        self.process = subprocess.Popen([str(server), '--logpath=' + str(output / 'logs'),
                                        '--metapath=' + str(output / 'meta')],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=self.stderr, cwd=workspace,
                                       env={**os.environ, 'XDG_CACHE_HOME': str(output / 'cache'),
                                            'XDG_CONFIG_HOME': str(output / 'config'),
                                            'XDG_DATA_HOME': str(output / 'data')})
        threading.Thread(target=self.reader, daemon=True).start()

    def reader(self):
        try:
            while True:
                headers, length = {}, 0
                while True:
                    line = self.process.stdout.readline(8193)
                    if not line:
                        return
                    length += len(line)
                    assert length <= 8192, 'Oversized server headers'
                    if line in (b'\r\n', b'\n'):
                        break
                    key, value = line.decode().split(':', 1)
                    headers[key.lower()] = value.strip()
                size = int(headers['content-length'])
                assert 0 < size <= 4 * 1024 * 1024
                body = self.process.stdout.read(size)
                assert len(body) == size
                self.messages.put(json.loads(body))
        except Exception as error:
            self.messages.put({'reader_error': repr(error)})

    def send(self, message):
        assert len(self.trace) < 5000, 'Transcript limit'
        self.trace.append({'direction': 'send', 'message': message})
        body = json.dumps({'jsonrpc': '2.0', **message}).encode()
        self.process.stdin.write(b'Content-Length: ' + str(len(body)).encode() + b'\r\n\r\n' + body)
        self.process.stdin.flush()

    def receive(self, timeout):
        message = self.messages.get(timeout=timeout)
        self.trace.append({'direction': 'recv', 'message': message})
        assert 'reader_error' not in message, message
        if 'method' in message and 'id' in message:
            if message['method'] == 'workspace/configuration':
                values = []
                for item in message['params']['items']:
                    value = {'Lua': self.config}
                    for part in item.get('section', '').split('.'):
                        if part:
                            value = value.get(part, {}) if isinstance(value, dict) else {}
                    values.append(value)
                self.send({'id': message['id'], 'result': values})
            elif message['method'] == 'workspace/workspaceFolders':
                self.send({'id': message['id'], 'result': [{'uri': self.workspace.as_uri(), 'name': 'generated'}]})
            elif message['method'] == 'window/workDoneProgress/create':
                self.send({'id': message['id'], 'result': None})
            else:
                self.send({'id': message['id'], 'error': {'code': -32601, 'message': 'Unsupported client request'}})
        return message

    def request(self, method, params, timeout=15):
        self.serial += 1
        serial = self.serial
        self.send({'id': serial, 'method': method, 'params': params})
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = self.receive(deadline - time.monotonic())
            if result.get('id') == serial and 'method' not in result:
                assert 'error' not in result, result
                return result['result']
        raise TimeoutError(method)

    def drain(self, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                self.receive(deadline - time.monotonic())
            except queue.Empty:
                return

    def close(self):
        try:
            self.request('shutdown', None, 3)
            self.send({'method': 'exit'})
            self.process.stdin.close()
            self.process.wait(timeout=5)
        except Exception:
            self.process.kill()
            self.process.wait()
        self.stderr.close()


def position(source, needle, after):
    prefix = source[:source.index(needle) + after]
    return {'line': prefix.count('\n'), 'character': len(prefix.split('\n')[-1].encode('utf-16-le')) // 2}


def definitions(value):
    entries = value if isinstance(value, list) else [value] if value else []
    return [unquote(urlparse(item.get('uri', item.get('targetUri', ''))).path) for item in entries]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    workspace, path, output = args.workspace.resolve(), args.source.resolve(), args.output.absolute()
    assert path.is_relative_to(workspace) and not output.is_relative_to(workspace)
    output.mkdir(parents=True, exist_ok=False)
    body = path.read_bytes()
    source, uri = body.decode('utf-8'), path.as_uri()
    flat = json.loads((workspace / '.luarc.json').read_text())
    config = {}
    for key, value in flat.items():
        if key.startswith('$'):
            continue
        node = config
        parts = key.split('.')
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        node[parts[-1]] = value
    client = Client(args.server.resolve(), workspace, output, config)
    result = {'source_sha256': hashlib.sha256(body).hexdigest(), 'server_sha256': hashlib.sha256(args.server.read_bytes()).hexdigest()}
    try:
        result['initialize'] = client.request('initialize', {'processId': os.getpid(), 'rootUri': workspace.as_uri(),
            'workspaceFolders': [{'uri': workspace.as_uri(), 'name': 'generated'}],
            'capabilities': {'workspace': {'configuration': True, 'workspaceFolders': True},
                             'textDocument': {'completion': {'completionItem': {'snippetSupport': True}},
                                              'publishDiagnostics': {'relatedInformation': True}}}})
        client.send({'method': 'initialized', 'params': {}})
        client.send({'method': 'textDocument/didOpen', 'params': {'textDocument': {'uri': uri, 'languageId': 'lua', 'version': 1, 'text': source}}})
        client.drain(2)
        for name, needle, offset in [('properties', 'props.speed', 6), ('host', 'host.resolve', 5)]:
            value = client.request('textDocument/completion', {'textDocument': {'uri': uri}, 'position': position(source, needle, offset), 'context': {'triggerKind': 1}})
            result[name + '_completion'] = value
            items = value if isinstance(value, list) else value.get('items', []) if value else []
            labels = {item['label'].strip(chr(34)) for item in items}
            expected = {'speed', 'enabled', 'target'} if name == 'properties' else {'resolve'}
            assert expected <= labels, (name, labels)
        for name, needle, offset in [('speed', 'props.speed', 8), ('resolve', 'host.resolve', 8)]:
            value = client.request('textDocument/definition', {'textDocument': {'uri': uri}, 'position': position(source, needle, offset)})
            result[name + '_definition'] = value
            assert any(Path(p).is_relative_to(workspace / 'types') for p in definitions(value)), value
        result['valid_diagnostics'] = [m['message']['params'] for m in client.trace if m['direction'] == 'recv' and m['message'].get('method') == 'textDocument/publishDiagnostics']
        latest = {message['uri']: message for message in result['valid_diagnostics']}
        assert not any(item.get('severity', 1) <= 2 for message in latest.values() for item in message.get('diagnostics', [])), latest
        bad = source.replace('if not props.enabled', 'props.enabled = "wrong type"\n        if not props.enabled', 1)
        assert bad != source
        client.send({'method': 'textDocument/didChange', 'params': {'textDocument': {'uri': uri, 'version': 2}, 'contentChanges': [{'text': bad}]}})
        client.drain(4)
        diagnostics = [m['message']['params'] for m in client.trace if m['direction'] == 'recv' and m['message'].get('method') == 'textDocument/publishDiagnostics']
        result['diagnostics_after_in_memory_fault'] = diagnostics
        assert any(item.get('code') == 'assign-type-mismatch' for message in diagnostics for item in message.get('diagnostics', [])), diagnostics
        assert path.read_bytes() == body, 'Protocol-only fault changed the working file'
        result['ok'] = True
    except Exception as error:
        result['error'] = repr(error)
        raise
    finally:
        client.close()
        result['exit_code'] = client.process.returncode
        if result.get('ok') and client.process.returncode != 0:
            result['ok'] = False
            result['error'] = 'LuaLS did not exit normally'
        (output / 'transcript.json').write_text(json.dumps(client.trace, indent=2) + '\n')
        (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    assert result['ok'], result
    print(json.dumps({'ok': True, 'source_sha256': result['source_sha256'], 'output': str(output)}, indent=2))


if __name__ == '__main__':
    main()
