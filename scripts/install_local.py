#!/usr/bin/env python3
"""Build and install Rot Engine for this user, with one-step rollback.

    python3 scripts/install_local.py              # build release, install under ~/.local
    python3 scripts/install_local.py --no-build   # install the existing target/release binaries
    python3 scripts/install_local.py --rollback   # put the previous version back

Installs `rot-editor`, `rot-mcp` and the `rot-player` player into ~/.local/bin, the desktop entry
`org.rot.editor.desktop` and its icon. Whatever those files replace, plus the
pre-rename `ruster-*` install, is moved to ~/.local/share/rot-engine/previous
so `--rollback` can restore it exactly. AI app configuration is never touched.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HOME = Path.home()
BIN = HOME / ".local/bin"
APPS = HOME / ".local/share/applications"
ICONS = HOME / ".local/share/icons/hicolor"
PREVIOUS = HOME / ".local/share/rot-engine/previous"
EDITOR = BIN / "rot-editor"

DESKTOP = f"""[Desktop Entry]
Type=Application
Name=Rot Engine
Comment=Create and edit 3D scenes and games
Exec={EDITOR}
TryExec={EDITOR}
Icon=org.rot.editor
Terminal=false
Categories=Graphics;3DGraphics;
StartupWMClass=org.rot.editor
"""

# Files from the pre-rename build. Install moves them aside; rollback restores them.
LEGACY = [BIN / "ruster-editor", BIN / "ruster-mcp", BIN / "rot-motion-lab", APPS / "org.ruster.editor.desktop",
          ICONS / "128x128/apps/org.ruster.editor.png", ICONS / "128x128/apps/rot-engine-launcher.png"]


def targets(release):
    """{installed path: source bytes} for this build."""
    return {
        EDITOR: (release / "rot-editor").read_bytes(),
        BIN / "rot-mcp": (release / "rot-mcp").read_bytes(),
        BIN / "rot-player": (release / "rot-player").read_bytes(),
        APPS / "org.rot.editor.desktop": DESKTOP.encode(),
        ICONS / "128x128/apps/org.rot.editor.png": (ROOT / "crates/rot-editor/assets/rot-engine-launcher.png").read_bytes(),
    }


def write(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.installing")
    temporary.write_bytes(data)
    temporary.chmod(mode)
    os.replace(temporary, path)


def refresh_desktop():
    for command in (["update-desktop-database", str(APPS)], ["gtk-update-icon-cache", "-q", "-t", str(ICONS)]):
        if shutil.which(command[0]):
            subprocess.run(command, check=False, capture_output=True)


def install(release):
    files = targets(release)
    binaries = [path for path in files if path.parent == BIN]
    if (PREVIOUS / "manifest.json").exists() and all(
            path.exists() and path.read_bytes() == files[path] for path in binaries):
        # Same build again (for example a desktop entry fix): keep the real previous version.
        for path, data in files.items():
            write(path, data, 0o755 if path in binaries else 0o644)
        refresh_desktop()
        print(f"Reinstalled the same build; the previous version in {PREVIOUS} is kept.")
        return
    # Keep exactly one previous version: the set of files this install replaces.
    if PREVIOUS.exists():
        shutil.rmtree(PREVIOUS)
    PREVIOUS.mkdir(parents=True)
    saved = []
    for index, path in enumerate([*files, *LEGACY]):
        if path.exists():
            backup = PREVIOUS / f"{index}-{path.name}"
            shutil.copy2(path, backup)
            saved.append({"path": str(path), "backup": backup.name})
    manifest = {"installed": [str(path) for path in files], "saved": saved}
    (PREVIOUS / "manifest.json").write_text(json.dumps(manifest, indent=1))
    for path, data in files.items():
        write(path, data, 0o755 if path.parent == BIN else 0o644)
    for path in LEGACY:
        path.unlink(missing_ok=True)
    refresh_desktop()
    print(f"Installed {EDITOR} with rot-mcp and the player; launch Rot Engine from the app launcher.")
    print(f"Previous version kept in {PREVIOUS}; undo with --rollback.")


def rollback():
    manifest_path = PREVIOUS / "manifest.json"
    if not manifest_path.exists():
        raise SystemExit("Nothing to roll back: no previous version is kept.")
    manifest = json.loads(manifest_path.read_text())
    for path in manifest["installed"]:
        Path(path).unlink(missing_ok=True)
    for entry in manifest["saved"]:
        path = Path(entry["path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(PREVIOUS / entry["backup"], path)
    shutil.rmtree(PREVIOUS)
    refresh_desktop()
    print("Restored the previous version:")
    for entry in manifest["saved"]:
        print(f"  {entry['path']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--rollback", action="store_true", help="restore the version replaced by the last install")
    parser.add_argument("--no-build", action="store_true", help="install target/release as it is")
    args = parser.parse_args()
    if args.rollback:
        return rollback()
    if not args.no_build:
        subprocess.run(["cargo", "build", "--release", "--locked", "-p", "rot-editor", "-p", "rot-mcp", "-p", "rot-player"],
                       cwd=ROOT, check=True)
    release = ROOT / "target/release"
    for name in ("rot-editor", "rot-mcp", "rot-player"):
        if not (release / name).is_file():
            raise SystemExit(f"{release / name} is missing; run without --no-build.")
    install(release)


if __name__ == "__main__":
    sys.exit(main())
