"""Disposable provisioning only; never changes a real client or installation."""
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class EnrollmentProvisioning(unittest.TestCase):
    def test_private_allowlist_and_no_legacy_permission_upgrade(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            adapter = root / "adapter"
            adapter.write_text("#!/bin/sh\nexit 0\n")
            adapter.chmod(0o700)
            registry = root / "legacy.json"
            records = [{"id": p, "name": p, "scene_id": "same"} for p in ("one", "two")]
            registry.write_text(json.dumps({"projects": records}))
            registry.chmod(0o600)
            original = registry.read_bytes()
            output = root / "enrolled"
            command = [sys.executable, str(ROOT / "scripts/enroll_local_connector.py"), "--registry", str(registry), "--adapter", str(adapter), "--output", str(output), "--connector-id", "fixture", "--project", "one"]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            grant = json.loads((output / "enrollment.json").read_text())
            self.assertEqual(grant["version"], 1)
            self.assertEqual([(p["id"], p["allowed"]) for p in grant["projects"]], [("one", True), ("two", False)])
            self.assertNotIn(grant["secret"], result.stdout + result.stderr)
            self.assertEqual(len(grant["secret"]), 64)
            self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE((output / "enrollment.json").stat().st_mode), 0o600)
            self.assertEqual(registry.read_bytes(), original)
            enrolled = json.loads((output / "registry.json").read_text())
            self.assertTrue(all(p["enrollment"] == str(output / "enrollment.json") for p in enrolled["projects"]))
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
            self.assertEqual(json.loads((output / "enrollment.json").read_text()), grant)


if __name__ == "__main__":
    unittest.main()
