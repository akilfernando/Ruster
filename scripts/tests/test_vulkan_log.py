"""Unique regression: mixed-case driver errors must never be called clean."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("vulkan_log", Path(__file__).resolve().parents[1] / "check_vulkan_log.py")
log = importlib.util.module_from_spec(spec)
spec.loader.exec_module(log)


class VulkanLogTests(unittest.TestCase):
    def test_validation_error_hazard_and_expected_notice_classification(self):
        for text in [
            "Validation Error: [ VUID-StandaloneSpirv-None-10684 ]",
            "validation error: shader layout",
            "ERROR wgpu_hal::vulkan::instance: driver error",
            "Synchronization issue: SYNC-HAZARD-WRITE-AFTER-READ",
        ]:
            self.assertTrue(log.defects(text), text)
        for text in [
            "WARN wgpu_hal::vulkan::instance: env var VK_INSTANCE_LAYERS adds validation layer",
            "Ruster shader: candidate rejected; last good pipeline retained",
        ]:
            self.assertFalse(log.defects(text), text)
