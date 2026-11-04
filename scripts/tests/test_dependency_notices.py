"""Unique publication risk: a patched path dependency still needs source notices."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("dependency_notices", Path(__file__).resolve().parents[1] / "dependency_notices.py")
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class NoticeTests(unittest.TestCase):
    def test_nonworkspace_vendored_dependency_is_in_inventory_without_local_paths(self):
        packages = [
            {"id": "own", "name": "ruster", "version": "0.1.0", "source": None},
            {"id": "registry", "name": "serde", "version": "1.0", "source": "registry+example"},
            {"id": "patch", "name": "wgpu-hal", "version": "30.0.1", "source": None,
             "manifest_path": str(notices.ROOT / "third_party/wgpu-hal/Cargo.toml")},
        ]
        result = notices.external_packages({"workspace_members": ["own"], "packages": packages})
        self.assertEqual([p["name"] for p in result], ["serde", "wgpu-hal"])
        self.assertEqual(result[1]["source"], "vendored:third_party/wgpu-hal")
