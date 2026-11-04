#!/usr/bin/env python3
"""Root-controlled behavior inspection adapter; no launches, signals or compositor writes.

Uses the existing content/recovery driver's audited loopback, process ownership,
per-request workspace isolation and transcript protocol. Artifacts and fixtures
are confined to /tmp/ruster-behavior-native/NAME. No user desktop input or
clipboard. Root launches and controls the exact process and grants each phase.

    PYTHONPATH=/tmp/ruster-walkthrough-python python3 docs/evidence/M8-behavior-native.py prepare NAME
    PYTHONPATH=/tmp/ruster-walkthrough-python python3 docs/evidence/M8-behavior-native.py run NAME --pid PID --instance SIGNATURE --port PORT --actions /tmp/ruster-behavior-native/NAME/actions.json

Additional JSON actions: {"op":"wait_label", "contains":"...", "seconds":15},
and {"op":"scroll", "selector":{...}, "dy":-400}. Scroll uses app-scoped
inspection input with pointer over the uniquely matched widget. Other action
forms match M8-content-recovery-native.py. Each wait polls guarded app trees;
its timeout is a failure, never a hidden retry. Evaluate unique bounds from
actual trees, particularly the repeated property-row buttons.
"""
from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location('ruster_native_base', Path(__file__).with_name('M8-content-recovery-native.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
base.BASE = Path('/tmp/ruster-behavior-native')


class BehaviorDriver(base.Driver):
    def action(self, a, index):
        if a['op'] == 'wait_label':
            seconds = float(a.get('seconds', 15))
            assert 0 < seconds <= 30
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                matches = [n[-1].get('value', '') for n in self.nodes()
                           if n[0] == 'label' and a['contains'] in str(n[-1].get('value', ''))]
                if matches:
                    return {'matched_labels': matches}
                time.sleep(.1)
            raise AssertionError(f"Timed out waiting for label containing {a['contains']!r}")
        if a['op'] == 'scroll':
            dy = float(a['dy'])
            assert 0 < abs(dy) <= 1000
            pos = self.center(a['selector'])
            self.event([{'PointerMoved': pos}, {'MouseWheel': {'unit': 'Point', 'phase': 'Move',
                       'source': 'Unknown', 'delta': {'x': 0.0, 'y': dy}, 'modifiers': base.MODS}}])
            return {}
        return super().action(a, index)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['prepare', 'run'])
    parser.add_argument('name')
    parser.add_argument('--pid', type=int)
    parser.add_argument('--instance')
    parser.add_argument('--port', type=int)
    parser.add_argument('--actions')
    args = parser.parse_args()
    path = base.runpath(args.name)
    if args.stage == 'prepare':
        base.prepare(path)
    else:
        assert path.is_dir() and args.pid and args.instance and args.port and args.actions
        base.Driver = BehaviorDriver
        base.execute(args, path)


if __name__ == '__main__':
    main()
