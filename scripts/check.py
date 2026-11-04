#!/usr/bin/env python3
"""Risk-based local checks. Use --full when the automatic scope is insufficient."""

import argparse
from dataclasses import dataclass
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from urllib.parse import unquote, urlsplit


@dataclass
class Plan:
    scope: str
    reason: str
    packages: tuple[str, ...] = ()


def git(root, *args):
    return subprocess.run(
        ["git", "-C", str(root), *args], check=True, capture_output=True
    ).stdout


def changed_files(root, base=None):
    """Return committed, index, worktree and untracked paths, plus a full reason."""
    try:
        git(root, "rev-parse", "--verify", "HEAD^{commit}")
    except subprocess.CalledProcessError:
        if base is not None:
            git(root, "rev-parse", "--verify", base + "^{commit}")
        return set(), "unborn HEAD: verify the entire workspace"
    ref = base if base is not None else "HEAD"
    # Invalid refs and disconnected histories are errors, never an empty change set.
    ancestor = git(root, "merge-base", ref, "HEAD").decode().strip()
    paths = set()
    for args in (
        ("diff", "--name-only", "--no-renames", "-z", ancestor, "HEAD"),
        ("diff", "--cached", "--name-only", "--no-renames", "-z", "HEAD"),
        ("diff", "--name-only", "--no-renames", "-z"),
        ("ls-files", "--others", "--exclude-standard", "-z"),
    ):
        paths.update(p.decode("utf-8", "surrogateescape") for p in git(root, *args).split(b"\0") if p)
    return paths, None


def markdown_files(root):
    """Use Git inventory so ignored build artifacts are never scanned."""
    return {p.decode("utf-8", "surrogateescape") for p in git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z").split(b"\0") if p and docs_path(p.decode("utf-8", "surrogateescape"))}


def cargo_binary():
    cargo = shutil.which("cargo")
    if cargo:
        return cargo
    fallback = Path.home() / ".cargo/bin/cargo"
    if fallback.is_file():
        return str(fallback)
    raise RuntimeError("cargo not found; install Rust or source ~/.cargo/env")


def metadata(root):
    result = subprocess.run(
        [cargo_binary(), "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=root, check=True, capture_output=True, text=True,
    )
    return json.loads(result.stdout)


def docs_path(path):
    return Path(path).suffix.lower() == ".md"


def choose_plan(root, paths, full_reason=None, workspace=None):
    if full_reason:
        return Plan("full", full_reason)
    if not paths:
        return Plan("none", "no changes relative to base or in the working tree")
    if all(docs_path(p) for p in paths):
        return Plan("docs", "only Markdown changed")
    # Policy, build inputs and unrecognized files can affect the whole workspace.
    if any(not (p.startswith("crates/") and Path(p).suffix == ".rs" and Path(p).name != "build.rs") for p in paths if not docs_path(p)):
        return Plan("full", "shared build inputs, check policy, or unrecognized files changed")
    data = workspace if workspace is not None else metadata(root)
    members = set(data["workspace_members"])
    packages = [p for p in data["packages"] if p["id"] in members]
    roots = {p["name"]: Path(p["manifest_path"]).parent.relative_to(root).as_posix() for p in packages}
    affected = set()
    for path in paths:
        if docs_path(path):
            continue
        owners = [name for name, directory in roots.items() if path.startswith(directory + "/")]
        if not owners:
            return Plan("full", "changed source has no known workspace package")
        affected.update(owners)
    # Use manifest paths for local edges; dependency aliases need not match names.
    manifest_names = {str(Path(p["manifest_path"]).parent.resolve()): p["name"] for p in packages}
    reverse = {p["name"]: set() for p in packages}
    for package in packages:
        for dependency in package["dependencies"]:
            local = dependency.get("path")
            if local and str(Path(local).resolve()) in manifest_names:
                reverse[manifest_names[str(Path(local).resolve())]].add(package["name"])
    pending = list(affected)
    while pending:
        for dependent in reverse[pending.pop()]:
            if dependent not in affected:
                affected.add(dependent)
                pending.append(dependent)
    return Plan("packages", "changed packages and transitive workspace dependents", tuple(sorted(affected)))


# Inline and reference-style Markdown links. This intentionally does not fetch URLs
# or validate headings; link targets with spaces should use Markdown's <...> form.
LINK = re.compile(r"!?\[[^\]\n]*\]\(\s*(<[^>]+>|[^\s)]+)(?:\s+[^)]*)?\)|^\s*\[[^\]\n]+\]:\s*(<[^>]+>|\S+)", re.MULTILINE)


def validate_markdown(root, paths):
    errors = []
    for relative in sorted(paths):
        path = root / relative
        if not docs_path(relative) or not path.is_file():
            continue
        content = path.read_text(encoding="utf-8")
        for number, line in enumerate(content.splitlines(), 1):
            if line.rstrip() != line:
                errors.append(f"{relative}:{number}: trailing whitespace")
        if content and not content.endswith("\n"):
            errors.append(f"{relative}: missing final newline")
        # Fenced examples are not actual document links.
        prose = re.sub(r"(?ms)^\s*(`{3,}|~{3,})[^\n]*\n.*?^\s*\1\s*$", "", content)
        for match in LINK.finditer(prose):
            target = (match.group(1) or match.group(2)).strip("<>")
            parsed = urlsplit(target)
            if parsed.scheme or parsed.netloc or not parsed.path or parsed.path.startswith("/"):
                continue
            destination = path.parent / unquote(parsed.path)
            if not destination.exists():
                errors.append(f"{relative}: missing relative link target: {target}")
    if errors:
        raise RuntimeError("\n".join(errors))


def commands(plan):
    if plan.scope in ("none", "docs"):
        return []
    selection = ["--workspace"] if plan.scope == "full" else [arg for name in plan.packages for arg in ("-p", name)]
    checks = []
    if plan.scope == "full":
        checks.append([sys.executable, "-m", "unittest", "discover", "-s", "scripts/tests", "-v"])
    return checks + [
        ["cargo", "fmt", "--all", "--", "--check"],
        ["cargo", "clippy", *selection, "--all-targets", "--locked", "--", "-D", "warnings"],
        ["cargo", "test", *selection, "--locked"],
    ]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="compare from merge-base with REF (default: HEAD)")
    parser.add_argument("--full", action="store_true", help="run all workspace checks")
    parser.add_argument("--dry-run", action="store_true", help="print checks without executing them")
    args = parser.parse_args(argv)
    root = Path(__file__).resolve().parent.parent
    started = time.monotonic()
    try:
        paths, reason = changed_files(root, args.base)
        plan = choose_plan(root, paths, "explicit --full" if args.full else reason)
        print(f"Plan: {plan.scope}: {plan.reason}", flush=True)
        if plan.packages:
            print("Packages: " + ", ".join(plan.packages), flush=True)
        if plan.scope != "none":
            print("Check: Markdown whitespace and relative links", flush=True)
            if not args.dry_run:
                validate_markdown(root, markdown_files(root))
        for command in commands(plan):
            print("Check: " + " ".join(command), flush=True)
            if not args.dry_run:
                begin = time.monotonic()
                executable = cargo_binary() if command[0] == "cargo" else command[0]
                subprocess.run([executable, *command[1:]], cwd=root, check=True)
                print(f"Passed in {time.monotonic() - begin:.2f}s", flush=True)
        outcome = "No checks selected" if plan.scope == "none" else ("Planned" if args.dry_run else "Passed")
        print(f"{outcome} in {time.monotonic() - started:.2f}s", flush=True)
        return 0
    except (subprocess.CalledProcessError, OSError, ValueError, RuntimeError) as error:
        print(f"Check failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            detail = error.stderr.decode(errors="replace") if isinstance(error.stderr, bytes) else error.stderr
            print(detail.rstrip(), file=sys.stderr)
        print(f"Failed after {time.monotonic() - started:.2f}s", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
