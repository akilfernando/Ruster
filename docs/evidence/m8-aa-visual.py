#!/usr/bin/env python3
"""Capture actual hidden editor framebuffers through its existing inspection API.

Run with PYTHONPATH=/tmp/ruster-walkthrough-python after the coordinated build
and exact editor window rule described in M8-AA.md. No input/clipboard is used.
"""
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import msgpack
from importlib.machinery import SourceFileLoader

native = SourceFileLoader('aa_native', str(Path(__file__).with_name('m8-aa-native.py'))).load_module()
ROOT = native.ROOT
OUT = Path('/tmp/ruster-aa-visual')
records = []


def capture(samples):
    before = native.hypr('activeworkspace')['id']
    assert before != 2
    assert not any(c['class'] == 'org.ruster.editor' for c in native.hypr('clients'))
    env = dict(os.environ, XDG_RUNTIME_DIR='/run/user/1000', WAYLAND_DISPLAY='wayland-1',
               HYPRLAND_INSTANCE_SIGNATURE=native.INSTANCE,
               LD_LIBRARY_PATH='/home/akilf/.local/share/ruster-tools/vulkan-validation/usr/lib',
               VK_LAYER_PATH='/home/akilf/.local/share/ruster-tools/vulkan-validation/usr/share/vulkan/explicit_layer.d',
               VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation',
               VK_LAYER_VALIDATE_SYNC='1', VK_LOADER_DEBUG='layer',
               RUSTER_MSAA_SAMPLES=str(samples), EGUI_INSPECTION='127.0.0.1:5781',
               RUSTER_EDITOR_STATE=str(OUT / f'prefs{samples}.json'))
    with (OUT / f'msaa{samples}.log').open('wb') as log:
        process = subprocess.Popen([str(OUT / 'ruster-editor'), '--scene', str(OUT / 'project/scene.json')],
                                   env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline:
                matching = [c for c in native.hypr('clients') if c['pid'] == process.pid]
                if matching:
                    window = matching[0]
                    break
                assert process.poll() is None
                time.sleep(.05)
            assert window and window['workspace']['id'] == 2 and window['size'] == [1100, 760], window
            assert native.hypr('activeworkspace')['id'] == before
            while True:
                try:
                    connection = socket.create_connection(('127.0.0.1', 5781), timeout=3)
                    break
                except OSError:
                    assert time.monotonic() < deadline
                    time.sleep(.1)
            connection.settimeout(15)
            def exact(n):
                result = b''
                while len(result) < n:
                    chunk = connection.recv(n-len(result))
                    assert chunk
                    result += chunk
                return result
            assert exact(8) == b'eins\0\0\0\1'
            def ui(message):
                data = msgpack.packb(message)
                connection.sendall(struct.pack('>I', len(data)) + data)
                return msgpack.unpackb(exact(struct.unpack('>I', exact(4))[0]), raw=False, strict_map_key=False)
            tree = ui('GetTree')
            (OUT / f'msaa{samples}-tree.json').write_text(json.dumps(tree, indent=2, default=lambda value: {'binary_bytes': len(value)}) + '\n')
            screenshot = ui({'GetScreenshot': {'pixels_per_point': 1.0}})['Screenshot']
            (ROOT / f'docs/evidence/m8-aa-msaa{samples}.png').write_bytes(screenshot[1])
            connection.close()
            # Visual-only capture: terminate only this owned process. Normal
            # graceful frame-limited shutdown is covered by the release matrix.
            process.terminate()
            assert process.wait(timeout=15) == -15
            records.append(dict(samples=samples, window=window, screenshot_dimensions=screenshot[0],
                                start_workspace=before, end_workspace=native.hypr('activeworkspace')['id'],
                                shutdown='SIGTERM after visual-only capture'))
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
    subprocess.run(['python3', str(ROOT / 'scripts/check_vulkan_log.py'), str(OUT / f'msaa{samples}.log')], check=True)
    print('captured', samples, records[-1], flush=True)


for sample_count in [1, 4]:
    capture(sample_count)
(OUT / 'capture-records.json').write_text(json.dumps(records, indent=2) + '\n')
