#!/usr/bin/env python3
"""List what agent sessions left behind on this machine. Removes nothing unless asked.

    python3 scripts/tidy.py                   # report only
    python3 scripts/tidy.py --apply           # remove clean, merged leftovers
    python3 scripts/tidy.py --apply --discard-edits --build-output

Removed with --apply: worktrees with no uncommitted changes whose commit is
already on the trunk, branches merged into the trunk, and /tmp/rot-* entries
older than a day. --discard-edits also removes merged worktrees that hold
uncommitted measurement edits. --build-output also deletes debug build output
and loose folders under target/ (the next build is a full rebuild).

Never removed: the main checkout, worktrees whose branch is not merged, and
worktrees under Ruster-Audits or Ruster-Creations, which are kept on purpose.
"""

import argparse
import glob
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
KEPT = ("Ruster-Audits", "Ruster-Creations")
TARGET_KEEP = {"release", "CACHEDIR.TAG", ".rustc_info.json", "tmp"}


def git(*args, cwd=ROOT, check=True):
    return subprocess.run(["git", "-C", str(cwd), *args], check=check, capture_output=True, text=True)


def size(path):
    result = subprocess.run(["du", "-sx", "--block-size=1", str(path)], capture_output=True, text=True)
    try:
        return int(result.stdout.split()[0])
    except (IndexError, ValueError):
        return 0


def human(count):
    for unit in ("B", "KB", "MB", "GB"):
        if count < 1024 or unit == "GB":
            return f"{count:.0f} {unit}" if unit == "B" else f"{count:.1f} {unit}"
        count /= 1024


def trunk():
    head = git("symbolic-ref", "--short", "refs/remotes/origin/HEAD", check=False).stdout.strip()
    return head.split("/", 1)[1] if "/" in head else "main"


def worktrees():
    entries, current = [], {}
    for line in git("worktree", "list", "--porcelain").stdout.splitlines() + [""]:
        if not line:
            if current:
                entries.append(current)
            current = {}
        elif line.startswith("worktree "):
            current["path"] = Path(line[9:])
        elif line.startswith("HEAD "):
            current["head"] = line[5:]
        elif line.startswith("branch "):
            current["branch"] = line[7:].removeprefix("refs/heads/")
    return entries


def classify(entry, main):
    path = entry["path"]
    if path.resolve() == ROOT:
        return "main checkout"
    if any(part in KEPT for part in path.parts):
        return "kept on purpose"
    if not path.is_dir():
        return "missing"
    merged = git("merge-base", "--is-ancestor", entry["head"], main, check=False).returncode == 0
    if not merged:
        return "not merged"
    dirty = git("status", "--porcelain", cwd=path, check=False).stdout.strip()
    return "merged, has edits" if dirty else "merged, clean"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--apply", action="store_true", help="remove clean, merged leftovers")
    parser.add_argument("--discard-edits", action="store_true", help="with --apply: also remove merged worktrees that have uncommitted edits")
    parser.add_argument("--build-output", action="store_true", help="with --apply: also delete target/ except release/")
    parser.add_argument("--older-than-hours", type=float, default=24, help="age a /tmp entry needs before it is removed")
    args = parser.parse_args()
    main_branch = trunk()
    freed = 0

    print(f"Worktrees (trunk: {main_branch})")
    trees = worktrees()
    counts = {}
    for entry in trees:
        state = classify(entry, main_branch)
        counts[state] = counts.get(state, 0) + 1
        removable = state == "merged, clean" or (state == "merged, has edits" and args.discard_edits)
        if state in ("not merged", "merged, has edits"):
            print(f"  {state:18} {entry['path']}  {entry.get('branch', 'detached')}")
        if args.apply and removable:
            bytes_used = size(entry["path"])
            if git("worktree", "remove", "--force", str(entry["path"]), check=False).returncode == 0:
                freed += bytes_used
                print(f"  removed            {entry['path']}")
    for state, count in sorted(counts.items()):
        print(f"  {count:3}  {state}")
    if args.apply:
        git("worktree", "prune")

    print("Branches merged into the trunk")
    checked_out = {entry.get("branch") for entry in worktrees()}
    for branch in git("branch", "--merged", main_branch, "--format=%(refname:short)").stdout.split():
        if branch == main_branch or branch in checked_out:
            continue
        print(f"  {branch}")
        if args.apply:
            git("branch", "-d", branch, check=False)

    print("Temporary folders")
    registered = {str(entry["path"]) for entry in trees}
    cutoff = time.time() - args.older_than_hours * 3600
    stale = [p for p in glob.glob("/tmp/rot-*") if p not in registered and os.lstat(p).st_mtime < cutoff and os.lstat(p).st_uid == os.getuid()]
    stale_bytes = sum(size(p) for p in stale)
    print(f"  {len(stale)} entries in /tmp/rot-* older than {args.older_than_hours:g} h, {human(stale_bytes)} (this is memory when /tmp is tmpfs)")
    if args.apply:
        for path in stale:
            if os.path.islink(path) or os.path.isfile(path):
                os.unlink(path)
            else:
                shutil.rmtree(path, ignore_errors=True)
        freed += stale_bytes

    print("Build output")
    target = ROOT / "target"
    if target.is_dir():
        entries = sorted(target.iterdir())
        removable = [p for p in entries if p.name not in TARGET_KEEP]
        removable_bytes = sum(size(p) for p in removable)
        print(f"  target/ is {human(size(target))}; {human(removable_bytes)} is outside release/ (debug builds, old fixtures)")
        if args.apply and args.build_output:
            for path in removable:
                if path.is_dir() and not path.is_symlink():
                    shutil.rmtree(path, ignore_errors=True)
                else:
                    path.unlink()
            freed += removable_bytes

    stashes = git("stash", "list").stdout.strip().splitlines()
    if stashes:
        print(f"Stashes: {len(stashes)} (not touched; see `git stash list`)")

    if args.apply:
        print(f"Freed about {human(freed)}.")
    else:
        print("Nothing was removed. Run with --apply to remove clean, merged leftovers; see --help for the rest.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
