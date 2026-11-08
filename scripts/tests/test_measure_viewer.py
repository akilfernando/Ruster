"""Measurement provenance must not turn inherited validation into a false claim."""
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import measure_viewer


class MeasurementMetadataTests(unittest.TestCase):
    def test_validation_and_profile_remain_unknown_with_inherited_layer_requests(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "release-viewer"
            binary.write_bytes(b"arbitrary binary bytes")
            for environment in (
                {},
                {"VK_INSTANCE_LAYERS": "VK_LAYER_KHRONOS_validation", "VK_LAYER_VALIDATE_SYNC": "1"},
                {"VK_LOADER_LAYERS_ENABLE": "*validation*", "VK_LAYER_PATH": "/layers"},
                {"WGPU_VALIDATION": "1", "WGPU_DEBUG": "1"},
                {"VK_LOADER_LAYERS_DISABLE": "~implicit~", "UNRELATED_SECRET": "excluded"},
            ):
                with self.subTest(environment=environment):
                    original = environment.copy()
                    metadata = measure_viewer.measurement_metadata(binary, environment)
                    self.assertIsNone(metadata["validation"])
                    self.assertIsNone(metadata["binary_profile"])
                    self.assertFalse(metadata["validation_requested"]["command_line"])
                    self.assertTrue(metadata["validation_requested"]["environment_preserved"])
                    self.assertEqual(
                        metadata["validation_requested"]["environment"],
                        {key: value for key, value in environment.items() if key != "UNRELATED_SECRET"},
                    )
                    self.assertEqual(environment, original)
                    self.assertEqual(
                        metadata["binary_sha256"],
                        hashlib.sha256(binary.read_bytes()).hexdigest(),
                    )


if __name__ == "__main__":
    unittest.main()
