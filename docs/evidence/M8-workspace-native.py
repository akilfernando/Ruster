#!/usr/bin/env python3
"""Root-coordinated, app-scoped workspace/keyboard journey. Never launches a UI.

Run only after checking the exact editor PID/title is on hidden Hyprland workspace2.
The root launches/relaunches the inspection-enabled editor with disposable state;
this script sends only that app's loopback inspection events. No global input,
clipboard replacement, compositor mutation, fake WindowFocused event or subprocess
termination is used. Physical focused keyboard behavior remains a separate gate.

Stages:
  blank   -- no-argument launch; create/rename/save, camera/layout/settings, close.
  resume  -- no-argument relaunch; verify saved prefs/recent and reopen project.
  dirty-close -- same instance after resume; invalid draft, close guard/recovery.

Artifacts stay below /tmp/ruster-workspace-native. Root reviews/copies evidence.
Requires msgpack (the existing walkthrough Python environment supplies it).
"""
from __future__ import annotations

import argparse
import json
import socket
import struct
import subprocess
import time
from pathlib import Path

import msgpack

BASE = Path('/tmp/ruster-workspace-native')
MODS = dict(alt=False, ctrl=False, shift=False, mac_cmd=False, command=False)


class Inspection:
    def __init__(self, port):
        self.socket = socket.create_connection(('127.0.0.1', port), timeout=10)
        self.socket.settimeout(15)
        assert self.exact(8) == b'eins\0\0\0\1', 'Unexpected inspection handshake'

    def exact(self, count):
        value = b''
        while len(value) < count:
            part = self.socket.recv(count - len(value))
            if not part:
                raise EOFError('Editor inspection connection closed')
            value += part
        return value

    def call(self, value):
        payload = msgpack.packb(value)
        self.socket.sendall(struct.pack('>I', len(payload)) + payload)
        length = struct.unpack('>I', self.exact(4))[0]
        return msgpack.unpackb(self.exact(length), raw=False, strict_map_key=False)

    def nodes(self):
        return [node for _, node in self.call('GetTree')['Tree'][2][0]]

    def events(self, events):
        assert self.call({'ApplyEvents': {'events': events}}) == 'Done'
        time.sleep(.12)

    def click(self, label=None, box=None, modifiers=None):
        if box is None:
            matching = [node[-1] for node in self.nodes() if node[-1].get('label') == label]
            assert len(matching) == 1, (label, matching)
            box = matching[0]['bounds']
        pos = dict(x=(box[0] + box[2]) / 2, y=(box[1] + box[3]) / 2)
        for pressed in (True, False):
            self.events([{'PointerMoved': pos}, {'PointerButton': {
                'pos': pos, 'button': 'Primary', 'pressed': pressed,
                'modifiers': modifiers or MODS}}])

    def key(self, key, modifiers=None):
        for pressed in (True, False):
            self.events([{'Key': dict(key=key, physical_key=None, pressed=pressed,
                                     repeat=False, modifiers=modifiers or MODS)}])

    def text(self, value):
        self.key('A', {**MODS, 'ctrl': True, 'command': True})
        self.events([{'Text': value}])

    def inputs(self):
        return sorted((node[-1] for node in self.nodes() if node[0] == 'textInput'),
                      key=lambda p: (p['bounds'][1], p['bounds'][0]))

    def modal_input(self, anchor):
        label = next(node[-1]['bounds'] for node in self.nodes()
                     if node[0] == 'label' and node[-1].get('value') == anchor)
        fields = [field for field in self.inputs()
                  if field['bounds'][2] - field['bounds'][0] > 120
                  and field['bounds'][1] >= label[3] - 3]
        assert fields, (anchor, self.inputs())
        return min(fields, key=lambda field: abs(field['bounds'][1] - label[3])
                   + abs(field['bounds'][0] - label[0]))

    def labels(self):
        return [str(node[-1].get('value', '')) for node in self.nodes() if node[0] == 'label']

    def expect(self, text):
        wait_until(lambda: any(text in label for label in self.labels()), f'Missing UI label: {text}')

    def screenshot(self, path):
        image = self.call({'GetScreenshot': {'pixels_per_point': 1.0}})['Screenshot']
        path.write_bytes(bytes(image[1]))
        return image[0]

    def close_menu(self):
        self.click('Workspace')
        options = [node[-1]['label'] for node in self.nodes()
                   if str(node[-1].get('label', '')).startswith('Close window ·')]
        assert len(options) == 1, options
        self.click(options[0])


def hypr(instance, command):
    return json.loads(subprocess.check_output(['hyprctl', '-i', instance, '-j', command]))


def wait_until(predicate, description, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.1)
    raise AssertionError(description)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['blank', 'resume', 'dirty-close'])
    parser.add_argument('--port', type=int, default=5738)
    parser.add_argument('--pid', type=int, required=True, help='Exact root-launched editor PID')
    parser.add_argument('--instance', required=True, help='Live Hyprland instance signature')
    parser.add_argument('--output', type=Path, default=BASE)
    parser.add_argument('--state', type=Path, default=BASE / 'state/editor.json')
    args = parser.parse_args()
    output = args.output.resolve()
    state = args.state.resolve()
    assert output.is_relative_to(BASE) and state.is_relative_to(BASE), 'Use disposable test-only paths'
    output.mkdir(parents=True, exist_ok=True)
    project = output / 'workspace-scene.json'
    summary_path = output / 'summary.json'
    summary = json.loads(summary_path.read_text()) if summary_path.exists() else {}

    before_workspace = hypr(args.instance, 'activeworkspace')['id']
    assert before_workspace != 2, 'Test workspace must remain hidden; never switch the user workspace'
    matching = [c for c in hypr(args.instance, 'clients') if c['pid'] == args.pid]
    assert len(matching) == 1, matching
    window = matching[0]
    assert window['workspace']['id'] == 2 and window['class'] == 'org.ruster.editor', window
    assert 'Ruster' in window['title'] and 'Editor' in window['title'], window
    api = Inspection(args.port)
    records = []

    def snapshot(label):
        assert hypr(args.instance, 'activeworkspace')['id'] == before_workspace
        tree = api.call('GetTree')
        (output / f'{args.stage}-{label}-tree.json').write_text(json.dumps(tree, indent=2, default=lambda value: {'binary_bytes': len(value)}) + '\n')
        records.append(label)
        print(label, flush=True)

    def preferences():
        assert state.is_file(), 'Workspace preferences were not persisted'
        return json.loads(state.read_text())

    def settings():
        api.click('Workspace')
        api.click('Workspace and shortcuts…')
        api.expect('Workspace and shortcuts')

    def set_scale(value):
        # Resolve the numeric control anew for each zoom level. Its position
        # changes when the UI zoom pass recenters/resizes the settings modal.
        scale = next(node[-1] for node in api.nodes()
                     if node[0] == 'spinButton'
                     and node[-1].get('minNumericValue') == .75
                     and node[-1].get('maxNumericValue') == 2.0)
        api.click(box=scale['bounds'])
        api.click(box=scale['bounds'])
        api.text(str(value))
        api.key('Enter')
        wait_until(lambda: any(node[0] == 'spinButton'
                              and abs(node[-1].get('numericValue', -1) - value) < 1e-6
                              and node[-1].get('maxNumericValue') == 2.0
                              for node in api.nodes()),
                   f'UI-scale numeric editor did not accept{value}')

    def close_settings():
        api.click('Close workspace settings')
        wait_until(lambda: not any(label == 'Workspace and shortcuts'
                                   for label in api.labels()),
                   'Workspace settings modal remained open after Close')

    def choose_row(label, choice):
        row = next(node[-1]['bounds'] for node in api.nodes()
                   if node[0] == 'label' and node[-1].get('value') == label)
        center_y = (row[1] + row[3]) / 2
        options = [node[-1] for node in api.nodes() if node[-1].get('label') == choice
                   and abs((node[-1]['bounds'][1] + node[-1]['bounds'][3]) / 2 - center_y) < 16]
        assert len(options) == 1, (label, choice, options)
        api.click(box=options[0]['bounds'])

    if args.stage == 'blank':
        assert not project.exists(), 'Use a fresh output directory for the blank-stage save'
        api.expect('Start creating')
        snapshot('blank-welcome')
        api.click('Create cube')
        api.click('Rename object')
        api.click(box=api.modal_input('Display name; the stable object ID stays unchanged.')['bounds'])
        api.text('Keyboard beacon')
        api.key('Enter')
        api.expect('Keyboard beacon')
        assert not any(node[-1].get('label') == 'Apply name' for node in api.nodes())
        api.click('Rename object')
        api.click(box=api.modal_input('Display name; the stable object ID stays unchanged.')['bounds'])
        api.text('Discard this name')
        api.key('Escape')
        api.expect('Keyboard beacon')
        assert not any(node[-1].get('label') == 'Apply name' for node in api.nodes())
        snapshot('rename-enter-escape')
        api.click('Assets…')
        api.expect('Project mesh assets')
        api.key('Escape')
        assert not any('Project mesh assets' in label for label in api.labels())
        snapshot('asset-modal-escape')

        api.click('Orthographic') if any(node[-1].get('label') == 'Orthographic' for node in api.nodes()) else api.click('Perspective')
        # Explicitly set a known axis/preset and then restore free perspective.
        api.click('View presets')
        api.click('Top (+Y)')
        assert any(node[-1].get('label') == 'Orthographic' for node in api.nodes())
        api.click('View presets')
        api.click('Isometric')
        api.click('Workspace')
        api.click('Frame selected')
        snapshot('orthographic-isometric-frame')
        api.click('View presets')
        api.click('Free perspective')
        assert any(node[-1].get('label') == 'Perspective' for node in api.nodes())

        api.click('Save scene')
        api.click(box=api.modal_input('Absolute scene path')['bounds'])
        api.text(str(project))
        api.key('Enter')
        wait_until(project.exists, 'Enter did not submit the path/save dialog')
        saved = json.loads(project.read_text())
        assert len(saved['scene']['objects']) == 1
        assert saved['scene']['objects'][0]['name'] == 'Keyboard beacon'
        snapshot('path-enter-save')
        api.click('View presets')
        api.click('Isometric')

        settings()
        choose_row('Inspector side', 'Left')
        api.click('Show hierarchy')
        set_scale(2.0)
        api.click('Save workspace preferences')
        changed = preferences()
        assert changed['inspector_side'] == 'left' and not changed['hierarchy_visible']
        assert abs(changed['ui_scale'] - 2.0) < 1e-6
        close_settings()
        snapshot('scaled-2x-footer-close')
        settings()
        set_scale(1.25)
        api.click('Save workspace preferences')
        assert abs(preferences()['ui_scale'] - 1.25) < 1e-6
        close_settings()
        snapshot('persisted-layout-scale')
        # Restore hierarchy but retain left inspector and readable zoom for relaunch.
        settings()
        api.click('Show hierarchy')
        api.click('Save workspace preferences')
        close_settings()
        expected = preferences()
        assert expected['hierarchy_visible'] and expected['inspector_side'] == 'left'
        assert str(project.resolve()) in expected['recent']
        dimensions = api.screenshot(output / 'workspace.png')
        snapshot('ready-normal-close')
        api.close_menu()
        wait_until(lambda: not any(c['pid'] == args.pid for c in hypr(args.instance, 'clients')),
                   'Clean app-scoped Close window did not exit normally')
        summary['blank'] = dict(records=records, preferences=preferences(),
                                screenshot_dimensions=dimensions, start_workspace=before_workspace,
                                end_workspace=hypr(args.instance, 'activeworkspace')['id'])

    elif args.stage == 'resume':
        assert project.is_file() and summary.get('blank')
        api.expect('Start creating')
        expected = summary['blank']['preferences']
        resumed = preferences()
        for key in ('ui_scale', 'inspector_side', 'hierarchy_visible', 'bindings'):
            assert resumed[key] == expected[key], (key, resumed[key], expected[key])
        snapshot('relaunch-blank-with-saved-workspace')
        api.click('Recent scenes')
        api.click(str(project.resolve()))
        api.click(box=api.modal_input('Absolute scene path')['bounds'])
        # Path already populated by explicit recent choice; Enter opens, not autoload.
        api.key('Enter')
        api.expect('Keyboard beacon')
        assert any(node[-1].get('label') == 'Orthographic' for node in api.nodes())
        assert any('Saved' in value for value in api.labels())
        snapshot('explicit-recent-enter-reopen')
        summary['resume'] = dict(records=records, preferences=resumed,
                                 end_workspace=hypr(args.instance, 'activeworkspace')['id'])

    else:
        assert project.is_file() and summary.get('resume')
        before = project.read_bytes()
        inputs = api.inputs()
        # First three text inputs are relative translation offsets; next9 precise components.
        assert len(inputs) == 12, inputs
        api.click(box=inputs[3]['bounds'])
        api.text('-')
        api.click('Workspace')
        # Workspace is already open here; choose the same guarded Close action.
        close = [node[-1]['label'] for node in api.nodes()
                 if str(node[-1].get('label', '')).startswith('Close window ·')]
        assert len(close) == 1
        api.click(close[0])
        api.expect('Save, discard or cancel before closing.')
        api.click('Cancel closing scene')
        assert project.read_bytes() == before
        assert api.inputs()[3]['value'] == '-', 'Invalid precision draft was lost'
        snapshot('invalid-draft-close-cancel-preserved')
        api.click('Workspace')
        close = next(node[-1]['label'] for node in api.nodes()
                     if str(node[-1].get('label', '')).startswith('Close window ·'))
        api.click(close)
        api.click('Discard and close scene')
        wait_until(lambda: not any(c['pid'] == args.pid for c in hypr(args.instance, 'clients')),
                   'Discard close did not exit normally')
        assert project.read_bytes() == before
        summary['dirty_close'] = dict(records=records, persisted_scene_unchanged=True,
                                      end_workspace=hypr(args.instance, 'activeworkspace')['id'])

    assert hypr(args.instance, 'activeworkspace')['id'] == before_workspace
    summary['limitations'] = [
        'Inspection-injected Enter/Escape and menu actions do not establish physical focused keyboard behavior.',
        'Global shortcut capture/fly/gizmo physical dragging need a suitable focused-window opportunity.',
        'No global input, clipboard modification, fake focus event, compositor mutation or SIGTERM occurred.',
        'Root must record normal process exit codes and scan full Vulkan captures separately.',
    ]
    summary_path.write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    main()
