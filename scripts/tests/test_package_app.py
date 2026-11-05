"""Distribution boundary tests: portable payloads, isolation and failure atomicity."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import new_app
import package_app


class DistributionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "checkout"
        self.root.mkdir()
        put = lambda name, text: package_app.put(self.root, name, text.encode())
        put("Cargo.toml", '[workspace]\nmembers = []\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\npublish = false\nlicense = "MIT OR Apache-2.0"\nrepository = "https://example.invalid/source"\n')
        for name in new_app.ROOT_FILES:
            put(name, "fixture\n")
        for name in new_app.PACKAGING_FILES:
            put(name, "fixture package tooling\n")
        for name in new_app.CRATES:
            put(f"crates/{name}/Cargo.toml", f'[package]\nname = "{name}"\nversion.workspace = true\n')
            put(f"crates/{name}/src/lib.rs", "pub fn run() {}\n")
        put("crates/ruster-scene/tests/fixtures/legacy-v1.scene.json", "{}\n")
        put("examples/yard-app/Cargo.toml", '[package]\nname = "ruster-yard"\nversion.workspace = true\n[dependencies]\nruster-app = {path = "../../crates/ruster-app"}\n')
        put("examples/yard-app/src/main.rs", "fn main() {}\n")
        put("examples/logistics.scenario.json", "{}\n")
        put("examples/yard-app/resources/truck.obj", "v 0 0 0\n")
        for name in new_app.VENDOR_FILES:
            put("third_party/wgpu-hal/" + name, "fixture\n")
        put("third_party/wgpu-hal/src/lib.rs", "pub fn patched() {}\n")
        put("third_party/wgpu-hal/src/gles/shaders/clear.frag", "void main() {}\n")
        put("third_party/wgpu-hal/examples/a.rs", "fn main() {}\n")
        put("packaging/SOURCE-README.md", "portable source\n")
        put("packaging/BINARY-README.txt", "portable binary\n")
        put("packaging/licenses/example.txt", "readable license\n")
        put("packaging/license-sources.json", json.dumps({"texts": [{"path": "licenses/example.txt", "sha256": hashlib.sha256(b"readable license\n").hexdigest(), "source": "https://example.invalid/license", "packages": []}]}))
        self.app = {"id": "yard", "name": package_app.APP, "version": "0.1.0", "source": None}
        self.meta = {"packages": [self.app], "resolve": {"nodes": [{"id": "yard", "deps": []}]}}

    def test_source_allowlist_relocation_and_no_overwrite(self):
        """A new creator tree carries relative engine paths, never local private/cache files."""
        for relative in (".env", "crates/ruster-app/src/credentials.env", "crates/ruster-app/src/target/leaked.rs",
                         "examples/yard-app/resources/private.obj", "crates/ruster-editor/src/lib.rs"):
            package_app.put(self.root, relative, b"SECRET")
        destination = self.base / "my-app"
        manifest = new_app.export(self.root, destination, normalize_lock=False)
        moved = self.base / "relocated-app"
        destination.rename(moved)
        cargo = tomllib.loads((moved / "Cargo.toml").read_text())
        self.assertEqual(len(cargo["workspace"]["members"]), 6)
        self.assertTrue((moved / cargo["patch"]["crates-io"]["wgpu-hal"]["path"] / "src/lib.rs").is_file())
        self.assertTrue((moved / "third_party/wgpu-hal/src/gles/shaders/clear.frag").is_file())
        sample = moved / "examples/yard-app"
        path = tomllib.loads((sample / "Cargo.toml").read_text())["dependencies"]["ruster-app"]["path"]
        self.assertTrue((sample / path / "src/lib.rs").is_file())
        self.assertTrue((moved / "crates/ruster-scene/tests/fixtures/legacy-v1.scene.json").is_file())
        for name in new_app.PACKAGING_FILES:
            self.assertTrue((moved / name).is_file())
        for file in manifest["files"]:
            data = (moved / file["path"]).read_bytes()
            self.assertNotIn(b"SECRET", data)
            self.assertNotIn(str(self.root).encode(), data)
        with self.assertRaisesRegex(ValueError, "overwrite"):
            new_app.export(self.root, moved, normalize_lock=False)
        linked = self.base / "points-into-checkout"
        linked.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "outside the checkout"):
            new_app.export(self.root, linked / "wrong-location", normalize_lock=False)

    def test_export_missing_file_and_symlink_fail_without_partial_destination(self):
        """Incomplete or symlinked sources cannot escape or expose a partial source project."""
        destination = self.base / "my-app"
        resource = self.root / "examples/yard-app/resources/truck.obj"
        resource.unlink()
        with self.assertRaises(OSError):
            new_app.export(self.root, destination, normalize_lock=False)
        self.assertFalse(destination.exists())
        resource.symlink_to(self.root / "LICENSE-MIT")
        with self.assertRaisesRegex(ValueError, "symlink"):
            new_app.export(self.root, destination, normalize_lock=False)
        self.assertFalse(destination.exists())
        self.assertFalse(list(self.base.glob(".ruster-source-*")))

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("readelf"), "Linux ELF tooling required")
    def test_real_elf_archive_relocation_safe_members_and_no_overwrite(self):
        """Archive extraction preserves a real executable, safe relative names and payload checksums."""
        binary = Path(shutil.which("true")).resolve()
        output = self.base / "yard.tar.gz"
        with patch.object(package_app, "metadata", return_value=self.meta):
            manifest = package_app.package(self.root, binary, output)
        original = output.read_bytes()
        with tarfile.open(output) as archive:
            for member in archive.getmembers():
                self.assertTrue(member.isfile())
                self.assertFalse(Path(member.name).is_absolute())
                self.assertNotIn("..", Path(member.name).parts)
            archive.extractall(self.base / "relocated", filter="data")
        payload = self.base / "relocated/ruster-yard"
        for file in manifest["files"]:
            self.assertEqual(hashlib.sha256((payload / file["path"]).read_bytes()).hexdigest(), file["sha256"])
        subprocess.run([str(payload / "ruster-yard")], cwd=self.base, check=True)
        with patch.object(package_app, "metadata", return_value=self.meta):
            with self.assertRaisesRegex(ValueError, "overwrite"):
                package_app.package(self.root, binary, output)
        self.assertEqual(output.read_bytes(), original)

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("readelf"), "Linux ELF tooling required")
    def test_motion_selection_carries_optional_backend_and_owned_resources(self):
        """The second app exports its own closure/templates/vendor and packages its own payload."""
        put = lambda name, text: package_app.put(self.root, name, text.encode())
        for name in ("ruster-script", "ruster-behavior"):
            put(f"crates/{name}/Cargo.toml", f'[package]\nname = "{name}"\nversion.workspace = true\n')
            put(f"crates/{name}/src/lib.rs", "pub fn run() {}\n")
        put("crates/ruster-behavior/templates/follower.lua", "return {}\n")
        put("examples/motion-lab/Cargo.toml", '[package]\nname = "ruster-motion-lab"\nversion.workspace = true\n')
        put("examples/motion-lab/src/main.rs", "fn main() {}\n")
        put("examples/motion-lab/resources/motion.rhai", "fn update() {}\n")
        put("examples/motion-lab/resources/generic.scene.json", "{}\n")
        put("examples/motion-lab/resources/private.json", "SECRET")
        put("packaging/MOTION-SOURCE-README.md", "Motion source\n")
        put("packaging/MOTION-BINARY-README.txt", "Motion binary\n")
        # A source archive omits mlua's ignored development lockfile.
        for name in new_app.MLUA_FILES:
            if name != "Cargo.lock":
                put("third_party/mlua/" + name, "pinned upstream fixture\n")
        for directory in ("src", "docs", "tests", "examples", "benches"):
            put(f"third_party/mlua/{directory}/fixture.rs", "// pinned fixture\n")
        source = self.base / "motion-source"
        manifest = new_app.export(self.root, source, normalize_lock=False, app="motion-lab")
        self.assertEqual(manifest["application"], "ruster-motion-lab")
        cargo = tomllib.loads((source / "Cargo.toml").read_text())
        self.assertIn("crates/ruster-behavior", cargo["workspace"]["members"])
        self.assertIn("third_party/mlua", cargo["workspace"]["exclude"])
        self.assertTrue((source / "third_party/mlua/RUSTER-PATCH.md").is_file())
        self.assertTrue((source / "Cargo.lock").is_file())
        self.assertFalse((source / "third_party/mlua/Cargo.lock").exists())
        self.assertTrue((source / "crates/ruster-behavior/templates/follower.lua").is_file())
        self.assertFalse((source / "examples/yard-app").exists())
        self.assertFalse((source / "examples/motion-lab/resources/private.json").exists())
        meta = {"packages": [{"id":"motion", "name":"ruster-motion-lab", "version":"0.1.0", "source":None}],
                "resolve":{"nodes":[{"id":"motion", "deps":[]}]}}
        output = self.base / "motion.tar.gz"
        with patch.object(package_app, "metadata", return_value=meta):
            binary = package_app.package(self.root, Path(shutil.which("true")).resolve(), output, app="motion-lab")
        self.assertEqual(binary["application"], "ruster-motion-lab")
        with tarfile.open(output) as archive:
            names = set(archive.getnames())
            self.assertIn("ruster-motion-lab/resources/generic.scene.json", names)
            self.assertIn("ruster-motion-lab/ruster-motion-lab", names)
            self.assertNotIn("ruster-motion-lab/resources/truck.obj", names)
            self.assertEqual(archive.extractfile("ruster-motion-lab/README.txt").read(), b"Motion binary\n")

    def test_missing_notices_or_corrupted_supplements_are_blocking(self):
        """Declared SPDX metadata cannot silently replace readable redistribution notices."""
        depdir = self.base / "dependency"
        depdir.mkdir()
        missing = {"name": "external", "version": "1", "license": "MIT", "source": "registry", "manifest_path": str(depdir / "Cargo.toml")}
        with self.assertRaisesRegex(ValueError, "lack readable notices: external 1"):
            package_app.notices(self.root, [missing])
        (self.root / "packaging/licenses/example.txt").write_text("modified")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            package_app.notices(self.root, [])
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            new_app.export(self.root, self.base / "corrupt-source", normalize_lock=False)
        self.assertFalse((self.base / "corrupt-source").exists())

    def test_normal_build_closure_excludes_dev_only_dependencies(self):
        """Notice selection follows production/build edges, with dev-only crates excluded."""
        def dependency(package, kind):
            return {"pkg": package, "dep_kinds": [{"kind": kind}]}
        meta = {"packages": [self.app] + [{"id": n, "name": n, "version": "1"} for n in ("normal", "build", "dev")],
                "resolve": {"nodes": [{"id": "yard", "deps": [dependency("normal", None), dependency("build", "build"), dependency("dev", "dev")]}] +
                            [{"id": n, "deps": []} for n in ("normal", "build", "dev")]}}
        self.assertEqual([p["name"] for p in package_app.active_packages(meta)], ["build", "normal", "ruster-yard"])


if __name__ == "__main__":
    unittest.main()
