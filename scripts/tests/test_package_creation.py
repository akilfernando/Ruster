"""Creation packaging boundary: exact bytes, fixed closure, identity and atomic failure."""

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import package_app
import package_creation


class CreationDistributionTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "linux" and shutil.which("readelf"), "Linux ELF tooling required")
    def test_reusable_kit_and_creation_boundary(self):
        """One real-ELF fixture crosses kit preparation, relocation and bounded publication."""
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "checkout"
            root.mkdir()
            for name in ("LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt"):
                package_app.put(root, name, (name + " fixture\n").encode())
            large_notice = b"UPSTREAM LICENSE TEXT\n" + b"*" * (package_creation.MIB + 1)
            package_app.put(root, "THIRD_PARTY_NOTICES.txt", large_notice)
            package_app.put(root, "packaging/license-sources.json", b'{"texts":[]}\n')
            for name in package_creation.HELPER_FILES:
                package_app.put(root, name, (package_creation.ROOT / name).read_bytes())
            package_app.put(root, ".env", b"PRIVATE CHECKOUT SECRET")
            binary = Path(shutil.which("true")).resolve()
            app = {"id": "motion", "name": "ruster-motion-lab", "version": "0.1.0", "source": None}
            metadata = {"packages": [app], "resolve": {"nodes": [{"id": "motion", "deps": []}]}}
            kit_archive = base / "runtime.tar.gz"
            with patch.object(package_app, "metadata", return_value=metadata) as cargo_metadata, \
                    patch.object(package_app, "notices", return_value=(large_notice, [])), \
                    patch.object(package_creation, "run_runtime", return_value=package_creation.PROFILE) as profile:
                prepared = package_creation.prepare_runtime(root, binary, kit_archive)
            cargo_metadata.assert_called_once_with(root, "ruster-motion-lab")
            profile.assert_called_once_with(binary, ["--creation-info"])
            with tarfile.open(kit_archive) as archive:
                archive.extractall(base / "relocated", filter="data")
            kit = base / "relocated/ruster-runtime"
            runtime_json = (kit / "runtime.json").read_bytes()
            self.assertEqual(prepared["runtime_kit_sha256"], hashlib.sha256(runtime_json).hexdigest())
            self.assertEqual(package_creation.inventory(kit), package_creation.KIT_FILES | {"runtime.json"})
            # The three copied helpers must import without a source checkout/new_app module.
            help_result = subprocess.run([sys.executable, str(kit / "scripts/package_creation.py"), "--help"],
                                         cwd=base, capture_output=True, text=True, check=True)
            self.assertIn("prepare-runtime", help_result.stdout)
            self.assertFalse((kit / "scripts/__pycache__").exists())
            content_root = base / "content"
            content_root.mkdir()
            scene = {"format": "ruster-scene", "format_version": 5,
                     "scene": {"id": "owned-scene", "revision": 4, "objects": [
                         {"id": "marker", "transform": {"translation": [0.1, 0, 0],
                          "rotation": [0, 0, 0], "scale": [1, 1, 1]},
                          "color": [0.12, 0.58, 0.85, 1], "mesh": "triangle"}]},
                     "authoring": {"sources": {}, "bindings": {}}, "undo": [], "redo": []}
            # Deliberate whitespace: saved bytes must survive without JSON re-encoding.
            scene_bytes = json.dumps(scene, indent=3).encode() + b"\n\n"
            mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"
            content = {"schema": 1, "title": "Creator's 世界", "tick_millis": 100,
                       "scene_sha256": hashlib.sha256(scene_bytes).hexdigest(),
                       "meshes": {"triangle": {"path": "meshes/0000.obj", "sha256": hashlib.sha256(mesh).hexdigest()}}}
            package_app.put(content_root, "scene.json", scene_bytes)
            package_app.put(content_root, "content.json", package_creation.encoded(content))
            package_app.put(content_root, "meshes/0000.obj", mesh)
            package_app.put(content_root, "private.obj", b"PRIVATE PROJECT SECRET")
            response = {"mode": "creation", "title": content["title"], "tick": "0", "fixed_dt": 0.1,
                        "authored_scene": package_creation.typed_scene(scene)}
            output = base / "creation.tar.gz"
            # Packaging itself must use only the prepared kit, never Cargo/readelf/notices.
            with patch.object(package_app, "metadata", side_effect=AssertionError("Cargo during export")), \
                    patch.object(package_app, "elf_requirements", side_effect=AssertionError("readelf during export")), \
                    patch.object(package_app, "notices", side_effect=AssertionError("notices during export")), \
                    patch.object(package_creation, "run_runtime", return_value=response):
                receipt = package_creation.package(kit, content_root, output)
                original = output.read_bytes()
                with tarfile.open(output) as archive:
                    members = archive.getmembers()
                    self.assertTrue(all(m.isfile() and m.name.startswith("application/")
                                        and ".." not in Path(m.name).parts for m in members))
                    archive.extractall(base / "application-relocated", filter="data")
                application = base / "application-relocated/application"
                self.assertEqual((application / "resources/scene.json").read_bytes(), scene_bytes)
                self.assertEqual((application / "resources/meshes/0000.obj").read_bytes(), mesh)
                for name in ("THIRD_PARTY_NOTICES.txt", "SUPPLEMENTAL_NOTICES.txt"):
                    self.assertEqual((application / name).read_bytes(), large_notice)
                self.assertFalse((application / "scripts").exists())
                self.assertFalse((application / "content.json").exists())
                manifest_bytes = (application / "manifest.json").read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertEqual(receipt["manifest_sha256"], hashlib.sha256(manifest_bytes).hexdigest())
                self.assertEqual(receipt["archive_sha256"], hashlib.sha256(original).hexdigest())
                for record in manifest["files"]:
                    data = (application / record["path"]).read_bytes()
                    self.assertEqual(hashlib.sha256(data).hexdigest(), record["sha256"])
                    self.assertNotIn(b"PRIVATE", data)
                with self.assertRaisesRegex(ValueError, "overwrite"):
                    package_creation.package(kit, content_root, output)
                self.assertEqual(output.read_bytes(), original)
                failed = base / "failed.tar.gz"
                def rejected():
                    with self.assertRaises((ValueError, OSError)):
                        package_creation.package(kit, content_root, failed)
                    self.assertFalse(failed.exists())
                # Exact bounded kit inventory and hashed bytes, including nested duplicate JSON keys.
                for mutation in ("tamper", "extra", "duplicate-record", "traversal", "oversize", "duplicate-key"):
                    with self.subTest(mutation=mutation):
                        value = json.loads(runtime_json)
                        if mutation == "tamper":
                            (kit / "README.txt").write_bytes(b"modified")
                        elif mutation == "extra":
                            (kit / "private.txt").write_bytes(b"private")
                        elif mutation == "duplicate-record":
                            value["files"][-1] = value["files"][0]
                        elif mutation == "traversal":
                            value["files"][0]["path"] = "../escape"
                        elif mutation == "oversize":
                            value["files"][0]["bytes"] = package_creation.MAX_BINARY_BYTES + 1
                        else:
                            (kit / "runtime.json").write_bytes(runtime_json.replace(b'"version": 1', b'"version":1,"version":1', 1))
                        if mutation not in ("tamper", "extra", "duplicate-key"):
                            (kit / "runtime.json").write_bytes(package_creation.encoded(value))
                        rejected()
                        (kit / "runtime.json").write_bytes(runtime_json)
                        (kit / "private.txt").unlink(missing_ok=True)
                        with tarfile.open(kit_archive) as archive:
                            readme = archive.extractfile("ruster-runtime/README.txt").read()
                        (kit / "README.txt").write_bytes(readme)
                # Changed/missing/symlinked references never cause recursive fallback or publication.
                (content_root / "scene.json").write_bytes(scene_bytes + b" ")
                rejected()
                (content_root / "scene.json").write_bytes(scene_bytes)
                (content_root / "meshes/0000.obj").unlink()
                rejected()
                (content_root / "meshes/0000.obj").symlink_to(content_root / "private.obj")
                rejected()
                (content_root / "meshes/0000.obj").unlink()
                (content_root / "meshes/0000.obj").write_bytes(mesh)
                with patch.object(package_creation, "run_runtime", side_effect=ValueError("invalid Lua")):
                    rejected()
                wrong = copy.deepcopy(response)
                wrong["authored_scene"]["objects"][0]["color"][0] = 0.12
                self.assertNotEqual(wrong["authored_scene"], response["authored_scene"])
                with patch.object(package_creation, "run_runtime", return_value=wrong):
                    rejected()
                cancellation = base / "cancel"
                cancellation.touch()
                with self.assertRaisesRegex(ValueError, "cancelled"):
                    package_creation.package(kit, content_root, failed, cancellation)
                self.assertFalse(failed.exists())

    @unittest.skipUnless(sys.platform == "linux", "Linux process lifecycle required")
    def test_cancellation_reaps_the_owned_preflight_child(self):
        """Cancellation during a running process kills and reaps only that process."""
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            marker, cancel = base / "pid", base / "cancel"
            code = f"import os,time;open({str(marker)!r},'w').write(str(os.getpid()));time.sleep(30)"
            timer = threading.Timer(0.3, cancel.touch)
            timer.start()
            try:
                with self.assertRaisesRegex(ValueError, "cancelled"):
                    package_creation.run_runtime(Path(sys.executable), ["-c", code], cancel)
            finally:
                timer.cancel()
                timer.join()
            self.assertTrue(marker.exists(), "child started before cancellation")
            with self.assertRaises(ProcessLookupError):
                os.kill(int(marker.read_text()), 0)


if __name__ == "__main__":
    unittest.main()
