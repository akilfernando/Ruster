#!/usr/bin/env python3
"""Root-controlled native inspection driver; does not launch, kill, or alter Hyprland.

prepare RUN: create a fresh fixture under /tmp/ruster-content-recovery-native/RUN
and print launch parameters. Copy only the listed source files; no project cache.
run RUN --pid PID --instance SIGNATURE --port PORT --actions FILE: execute a
JSON array of inspection actions against the root-launched process. Each event
and tree/screenshot request rechecks PID/class/title/workspace and active workspace.
checkpoint RUN: inspect and retain the actual completed checkpoint, verifying
its document checksum, original scene/catalog origin and required draft text.

No focus fabrication, workspace switching, global input, clipboard or process
termination. Input is app-scoped injection; it is not physical-focus evidence.
The root owns process exits, raw stdout/stderr, Vulkan scan, and crash boundaries.
Actions are adapted from actual trees so changing UI labels remain visible. Each
run writes a unique transcript, unaltered framebuffer and JSON tree when requested;
failures are recorded and never retried automatically. No performance claims.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import time
import traceback
import uuid

import msgpack

BASE = Path('/tmp/ruster-content-recovery-native')
SOURCE = Path(__file__).resolve().parents[2] / 'examples/content'
MODS = dict(alt=False, ctrl=False, shift=False, mac_cmd=False, command=False)


def runpath(name):
    assert name and name not in ('.', '..') and '/' not in name and '\\' not in name
    path = (BASE / name).resolve()
    assert path.parent == BASE and not path.is_symlink()
    return path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def prepare(path):
    path.mkdir(parents=True, exist_ok=False)
    fixture = path / 'project'
    fixture.mkdir()
    for name in ('scene.json', 'scene.wgsl', '.ruster-assets.json', 'meshes'):
        src = SOURCE / name
        assert not src.is_symlink() and src.exists(), src
        if src.is_dir():
            assert not any(p.is_symlink() for p in src.rglob('*')), 'Symlink in fixture'
            shutil.copytree(src, fixture / name, ignore=shutil.ignore_patterns('*.lock', '*cache*', '.*cache*'))
        else:
            shutil.copy2(src, fixture / name)
    (path / 'state').mkdir()
    (path / 'recovery').mkdir()
    manifest = {'scene': str(fixture / 'scene.json'), 'state': str(path / 'state/editor.json'),
                'recovery': str(path / 'recovery'), 'scene_sha256': sha((fixture / 'scene.json').read_bytes()),
                'environment': {'RUSTER_EDITOR_STATE': str(path / 'state/editor.json'),
                                'RUSTER_RECOVERY_DIR': str(path / 'recovery')},
                'launch_arguments': ['--scene', str(fixture / 'scene.json')],
                'resume_arguments': [], 'inspection': 'Root must enable the inspection feature and choose an unused loopback port.',
                'notes': ['Root owns launching, signals, logs and Vulkan scanner.',
                          'Use a fresh sibling path for recovery Save; keep original external modifications.',
                          'Inspection events do not establish physically focused keyboard or drag behavior.']}
    (path / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest, indent=2))


def hypr(instance, command):
    return json.loads(subprocess.check_output(['hyprctl', '-i', instance, '-j', command]))


class Driver:
    def __init__(self, args, path, artifact):
        self.args, self.path, self.artifact = args, path, artifact
        self.active = hypr(args.instance, 'activeworkspace')['id']
        assert self.active != 2, 'Workspace 2 must be hidden'
        self.guard()
        env = dict(item.split(b'=', 1) for item in Path(f'/proc/{args.pid}/environ').read_bytes().split(b'\0') if b'=' in item)
        assert os.fsdecode(env.get(b'RUSTER_EDITOR_STATE', b'')) == str(path / 'state/editor.json')
        assert os.fsdecode(env.get(b'RUSTER_RECOVERY_DIR', b'')) == str(path / 'recovery')
        self.sock = socket.create_connection(('127.0.0.1', args.port), timeout=10)
        self.sock.settimeout(15)
        assert self.exact(8) == b'eins\0\0\0\1', 'Inspection handshake mismatch'

    def guard(self):
        assert hypr(self.args.instance, 'activeworkspace')['id'] == self.active != 2
        clients = [c for c in hypr(self.args.instance, 'clients') if c['pid'] == self.args.pid]
        assert len(clients) == 1, clients
        w = clients[0]
        assert w['workspace']['id'] == 2 and w['class'] == 'org.ruster.editor', w
        assert 'Ruster' in w['title'] and 'Editor' in w['title'], w
        # Tie the loopback listener to this PID, rather than trusting the port.
        inodes = set()
        for line in Path('/proc/net/tcp').read_text().splitlines()[1:]:
            fields = line.split()
            if fields[1] == f'0100007F:{self.args.port:04X}' and fields[3] == '0A':
                inodes.add(fields[9])
        owned = set()
        for fd in Path(f'/proc/{self.args.pid}/fd').iterdir():
            try:
                target = os.readlink(fd)
            except FileNotFoundError:
                continue
            if target.startswith('socket:['):
                owned.add(target[8:-1])
        assert inodes & owned, 'Inspection listener is not owned by target PID on IPv4 loopback'

    def exact(self, count):
        data = b''
        while len(data) < count:
            part = self.sock.recv(count - len(data))
            if not part:
                raise EOFError('Inspection socket closed')
            data += part
        return data

    def call(self, request):
        self.guard()
        data = msgpack.packb(request)
        self.sock.sendall(struct.pack('>I', len(data)) + data)
        size = struct.unpack('>I', self.exact(4))[0]
        assert size <= 64 * 1024 * 1024
        return msgpack.unpackb(self.exact(size), raw=False, strict_map_key=False)

    def nodes(self):
        return [n for _, n in self.call('GetTree')['Tree'][2][0]]

    def match(self, selector):
        nodes = self.nodes()
        matches = [n for n in nodes if all((n[0] == v if k == 'role' else n[-1].get(k) == v)
                                          for k, v in selector.items())]
        assert len(matches) == 1, (selector, matches)
        return matches[0][-1]

    def event(self, values):
        assert self.call({'ApplyEvents': {'events': values}}) == 'Done'
        time.sleep(.12)

    def pointer(self, pos, button, pressed):
        self.event([{'PointerMoved': pos}, {'PointerButton': {'pos': pos, 'button': button,
                    'pressed': pressed, 'modifiers': MODS}}])

    def center(self, selector):
        box = self.match(selector)['bounds']
        return {'x': (box[0]+box[2])/2, 'y': (box[1]+box[3])/2}

    def key(self, key, modifiers=None):
        for pressed in (True, False):
            self.event([{'Key': dict(key=key, physical_key=None, pressed=pressed,
                                      repeat=False, modifiers=modifiers or MODS)}])

    def action(self, a, index):
        kind = a['op']
        if kind == 'click':
            pos = self.center(a['selector'])
            for pressed in (True, False): self.pointer(pos, a.get('button', 'Primary'), pressed)
        elif kind == 'key': self.key(a['key'], {**MODS, **a.get('modifiers', {})})
        elif kind == 'text':
            if a.get('replace', True): self.key('A', {**MODS, 'ctrl': True, 'command': True})
            self.event([{'Text': a['value']}])
        elif kind == 'drag':
            start, end = self.center(a['from']), self.center(a['to'])
            self.pointer(start, 'Primary', True)
            for step in range(1, 9):
                t = step / 8
                self.event([{'PointerMoved': {k: start[k] + (end[k]-start[k])*t for k in ('x','y')}}])
            self.pointer(end, 'Primary', False)
        elif kind == 'assert':
            value = self.match(a['selector'])
            for key, expected in a.get('properties', {}).items(): assert value.get(key) == expected, (key, value)
        elif kind == 'assert_absent':
            assert not any(all((n[0] == v if k == 'role' else n[-1].get(k) == v)
                               for k,v in a['selector'].items()) for n in self.nodes())
        elif kind == 'tree':
            tree = self.call('GetTree')
            (self.artifact / f'{index:03d}-tree.json').write_text(json.dumps(tree, indent=2, default=lambda b: {'binary_bytes': len(b)}) + '\n')
        elif kind == 'screenshot':
            image = self.call({'GetScreenshot': {'pixels_per_point': 1.0}})['Screenshot']
            (self.artifact / f'{index:03d}-frame.png').write_bytes(bytes(image[1]))
            return {'dimensions': image[0]}
        else: raise ValueError(f'Unknown action: {kind}')
        return {}


def execute(args, path):
    actions = json.loads(Path(args.actions).read_text())
    assert isinstance(actions, list) and 0 < len(actions) <= 128
    artifact = path / ('actions-' + uuid.uuid4().hex)
    artifact.mkdir()
    (artifact / 'actions.json').write_text(json.dumps(actions, indent=2) + '\n')
    transcript = []
    try:
        driver = Driver(args, path, artifact)
        for i, action in enumerate(actions):
            result = driver.action(action, i)
            transcript.append({'index': i, 'action': action, 'result': result, 'status': 'pass'})
            print(f'{i}: {action["op"]} passed', flush=True)
    except BaseException:
        transcript.append({'index': len(transcript), 'status': 'failed', 'traceback': traceback.format_exc()})
        raise
    finally:
        (artifact / 'transcript.json').write_text(json.dumps(transcript, indent=2) + '\n')
        print('Artifacts:', artifact, flush=True)


def checkpoint(path, draft):
    files = list((path / 'recovery').glob('window-*/checkpoint.json'))
    assert len(files) == 1, files
    raw = files[0].read_bytes()
    record = json.loads(raw)
    assert record['format'] == 'ruster-editor-recovery' and record['version'] == 1 and record['sequence'] > 0
    snap = record['snapshot']
    text = raw.decode('utf-8')
    # Hash the exact raw JSON document slice, as Rust's RawValue does.
    marker = '"document":'
    assert text.count(marker) == 1
    start = text.index(marker) + len(marker)
    while text[start].isspace(): start += 1
    _, end = json.JSONDecoder().raw_decode(text, start)
    assert sha(text[start:end].encode()) == snap['document_sha256'], 'Document checksum mismatch'
    assert snap['origin'] == str(path / 'project/scene.json'), snap['origin']
    assert snap['asset_origin'] == str(path / 'project'), snap['asset_origin']
    def contains_draft(value):
        if isinstance(value, dict):
            return any(contains_draft(v) for v in value.values())
        if isinstance(value, list):
            return any(contains_draft(v) for v in value)
        return isinstance(value, str) and value == draft
    assert contains_draft(snap['drafts']), 'Exact required draft not in completed checkpoint'
    artifact = path / ('checkpoint-' + uuid.uuid4().hex)
    artifact.mkdir()
    (artifact / 'checkpoint.json').write_bytes(raw)
    result = {'source': str(files[0]), 'checkpoint_sha256': sha(raw), 'document_sha256': snap['document_sha256'],
              'sequence': record['sequence'], 'draft_query': draft, 'origin': snap['origin'], 'asset_origin': snap['asset_origin'],
              'original_sha256': sha((path / 'project/scene.json').read_bytes())}
    (artifact / 'verified.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('stage', choices=['prepare', 'run', 'checkpoint'])
    p.add_argument('name', help='Fresh single-component test run name')
    p.add_argument('--pid', type=int)
    p.add_argument('--instance')
    p.add_argument('--port', type=int)
    p.add_argument('--actions')
    p.add_argument('--draft', default='-', help='Required exact draft string in checkpoint')
    args = p.parse_args()
    path = runpath(args.name)
    if args.stage == 'prepare': prepare(path)
    elif args.stage == 'run':
        assert path.is_dir() and args.pid and args.instance and args.port and args.actions
        execute(args, path)
    else:
        assert path.is_dir()
        checkpoint(path, args.draft)


if __name__ == '__main__': main()
