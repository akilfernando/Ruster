#!/usr/bin/env python3
"""Fail on Vulkan validation/synchronization defects in captured native logs.

Expected layer-injection loader notices are not errors. Capture both stdout and
stderr, and retain raw logs: this scanner cannot reveal messages a driver or
upstream debug callback suppressed. Handled candidate shader parse failures are
not Vulkan driver defects; raw VUIDs and uncaptured validation lines are.
"""
import argparse
from pathlib import Path
import re
import sys

DEFECT = re.compile(r"\bVUID-[A-Za-z0-9-]+|\bSYNC[-_ ]HAZARD\b|^\s*Validation\s+Error\b|^\s*ERROR\b.*\b(?:wgpu|vulkan)\b", re.IGNORECASE)


def defects(text):
    return [(number, line) for number, line in enumerate(text.splitlines(), 1) if DEFECT.search(line)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", type=Path, nargs="+")
    args = parser.parse_args()
    failed = False
    for path in args.logs:
        try:
            found = defects(path.read_text(encoding="utf-8", errors="replace"))
        except OSError as exc:
            print(f"{path}: {exc}", file=sys.stderr)
            failed = True
            continue
        for number, line in found:
            print(f"{path}:{number}: {line}", file=sys.stderr)
        failed |= bool(found)
    if not failed:
        print("No Vulkan validation/synchronization defect found in supplied logs; inspect capture coverage separately.")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
