#!/usr/bin/env python3
"""Exact-process shared-kit native adapter; root owns launch/close/rules.

Same isolated fixture/protocol as behavior native checks, with explicit app class
and title. No workspace switch/global input/clipboard or process lifecycle here.
"""
import argparse
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('ruster_behavior_native', Path(__file__).with_name('M8-behavior-native.py'))
behavior = importlib.util.module_from_spec(spec)
spec.loader.exec_module(behavior)
behavior.base.BASE = Path('/tmp/ruster-motion-native')


class MotionDriver(behavior.BehaviorDriver):
    expected_class = 'ruster-motion-lab'
    title_parts = ('Motion Lab', 'Shared behavior comparison')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['prepare', 'run'])
    parser.add_argument('name')
    parser.add_argument('--pid', type=int)
    parser.add_argument('--instance')
    parser.add_argument('--port', type=int)
    parser.add_argument('--actions')
    args = parser.parse_args()
    path = behavior.base.runpath(args.name)
    if args.stage == 'prepare':
        behavior.base.prepare(path)
    else:
        assert path.is_dir() and args.pid and args.instance and args.port and args.actions
        behavior.base.Driver = MotionDriver
        behavior.base.execute(args, path)


if __name__ == '__main__':
    main()
