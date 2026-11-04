#!/usr/bin/env python3
"""Bounded hidden-workspace release MSAA comparison; no desktop input is sent.

Requires root-coordinated exclusive UI/measurement access and the temporary
exact-title floating/render_unfocused window rule documented in M8-AA.md.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path('/tmp/ruster-aa-native')
OUT.mkdir(exist_ok=True)
INSTANCE = '0aa6745f705f2c4b0a5004eb019ef76e5bbdb426_1793690289_1133262353'


def hypr(name):
    return json.loads(subprocess.check_output(['hyprctl', '-i', INSTANCE, '-j', name]))


def run():
    assert hypr('activeworkspace')['id'] != 2, 'Test workspace must be hidden before launch'
    assert not any(c['class'] == 'org.ruster.scene-runtime' for c in hypr('clients'))
    env = dict(os.environ, XDG_RUNTIME_DIR='/run/user/1000', WAYLAND_DISPLAY='wayland-1',
               HYPRLAND_INSTANCE_SIGNATURE=INSTANCE,
               LD_LIBRARY_PATH='/home/akilf/.local/share/ruster-tools/vulkan-validation/usr/lib',
               VK_LAYER_PATH='/home/akilf/.local/share/ruster-tools/vulkan-validation/usr/share/vulkan/explicit_layer.d',
               VK_INSTANCE_LAYERS='VK_LAYER_KHRONOS_validation',
               VK_LAYER_VALIDATE_SYNC='1', VK_LOADER_DEBUG='layer')
    results = OUT / 'results.json'
    records = json.loads(results.read_text()) if results.exists() else []
    for repeat in range(1, 4):
        for objects in [3, 256]:
            for samples in [1, 2, 4]:
                if any(r['repeat'] == repeat and r['objects'] == objects and r['requested'] == samples for r in records):
                    continue
                before_workspace = hypr('activeworkspace')['id']
                assert before_workspace != 2, 'User entered test workspace; stop isolated matrix'
                name = f'objects{objects}-msaa{samples}-repeat{repeat}'
                log = OUT / f'{name}.log'
                env['RUSTER_MSAA_SAMPLES'] = str(samples)
                command = [str(ROOT / 'target/release/ruster-render'), '--objects', str(objects), '--frames', '180']
                started = time.monotonic()
                with log.open('wb') as output:
                    process = subprocess.Popen(command, env=env, stdout=output, stderr=subprocess.STDOUT)
                    try:
                        window = None
                        while time.monotonic() - started < 15:
                            matching = [c for c in hypr('clients') if c['pid'] == process.pid]
                            if matching:
                                window = matching[0]
                                break
                            if process.poll() is not None:
                                break
                            time.sleep(.01)
                        assert window is not None, f'{name}: no mapped window'
                        assert window['workspace']['id'] == 2 and window['title'] == 'Ruster | Shared 3D', window
                        assert window['size'] == [1100, 760] and window['floating'], window
                        assert hypr('activeworkspace')['id'] == before_workspace, 'Workspace changed during launch; stop to inspect'
                        deadline = time.monotonic() + 30
                        while process.poll() is None:
                            assert hypr('activeworkspace')['id'] != 2, 'User entered test workspace during measurement'
                            assert time.monotonic() < deadline, 'Native frame run timed out'
                            time.sleep(.2)
                        assert process.returncode == 0, (name, process.returncode)
                        assert hypr('activeworkspace')['id'] != 2, 'Test became visible during measurement'
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=10)
                subprocess.run(['python3', str(ROOT / 'scripts/check_vulkan_log.py'), str(log)], check=True, stdout=subprocess.DEVNULL)
                content = log.read_text()
                assert 'Inserted device layer "VK_LAYER_KHRONOS_validation"' in content
                summary = next(line for line in content.splitlines() if 'Ruster shared 3D summary:' in line)
                fields = dict(re.findall(r'([a-z0-9_]+)=([^ ]+)', summary))
                assert fields['msaa_effective'] == str(samples)
                assert int(fields['frames']) >= 180 and int(fields['samples_gpu']) >= 32, fields
                assert int(fields['objects']) == objects, fields
                records.append(dict(repeat=repeat, objects=objects, requested=samples,
                                    elapsed_s=time.monotonic()-started, start_workspace=before_workspace,
                                    end_workspace=hypr('activeworkspace')['id'], window=window, metrics=fields))
                (OUT / 'results.json').write_text(json.dumps(records, indent=2) + '\n')
                print(name, fields['target_width'], fields['target_height'],
                      fields['cpu_prepare_us_p50'], fields['gpu_scene_us_p50'], flush=True)
    assert hypr('activeworkspace')['id'] != 2
    assert not any(c['class'] == 'org.ruster.scene-runtime' for c in hypr('clients'))


if __name__ == '__main__':
    run()
