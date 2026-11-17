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
    def test_malformed_registry_refused_before_output(self):
        valid = {"id": "one", "name": "One", "scene_id": "scene", "locator": "/fixture/connection.json", "binding": {"wire": "rot-owner-1", "incarnation": "i", "generation": "g", "connection": "c"}}
        cases = [({"projects": [{"id": "one", "name": "One", "scene_id": "s"}]}, "fixture"),
                 ({"projects": [valid]}, "é" * 128)]
        for field, value in [("id", "bad/id"), ("name", "é" * 128), ("scene_id", "é" * 128), ("unknown", True), ("binding", {**valid["binding"], "unknown": 1})]:
            cases.append(({"projects": [{**valid, field: value}]}, "fixture"))
        cases.extend([({"projects": [{**valid, "locator": "/\ud800"}]}, "fixture"),
                      ({"projects": [{**valid, "locator": "/" + "x" * 261900}]}, "fixture"),
                      ({"projects": [valid, valid]}, "fixture"),
                      ({"projects": [{**valid, "name": "bad\u0085name"}]}, "fixture"),
                      ({"projects": [{**valid, "startup": {"scene_path": "/s", "native_binary": "/n", "state_directory": "/d", "binary_identity": {}, "routing_identity": {}}}]}, "fixture"),
                      ('{"projects":[' + json.dumps(valid).replace('"id": "one"', '"id":"bad", "id":"one"') + ']}', "fixture"),
                      ({"projects": [valid, {**valid, "id": "two", "name": "one"}]}, "fixture"),
                      ({"projects": [valid], "unknown": True}, "fixture"),
                      ('{"projects":[],"projects":[' + json.dumps(valid) + ']}', "fixture")])
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            adapter = root / "adapter"
            adapter.write_text("#!/bin/sh\nexit 0\n")
            adapter.chmod(0o700)
            registry = root / "registry.json"
            for index, (value, connector) in enumerate(cases):
                with self.subTest(index=index):
                    registry.write_text(value if isinstance(value, str) else json.dumps(value))
                    registry.chmod(0o600)
                    output = root / str(index)
                    result = subprocess.run([sys.executable, str(ROOT / "scripts/enroll_local_connector.py"), "--registry", str(registry), "--adapter", str(adapter), "--output", str(output), "--connector-id", connector, "--project", "one"], capture_output=True, text=True)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(output.exists())
                    self.assertNotIn("stable_mcp_command", result.stdout)


    def test_private_allowlist_and_no_legacy_permission_upgrade(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            adapter = root / "adapter"
            adapter.write_text("#!/bin/sh\nexit 0\n")
            adapter.chmod(0o700)
            registry = root / "legacy.json"
            records = [{"id": p, "name": p, "scene_id": "same", "locator": "/fixture/connection.json", "binding": {"wire": "rot-owner-1", "incarnation": "i", "generation": "g", "connection": "c"}} for p in ("one", "two")]
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
