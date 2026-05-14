import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.runtime_test import build_runtime_test_report, format_runtime_test_report


class RuntimeTestPlanTest(unittest.TestCase):
    def test_builds_default_runtime_test_command(self):
        report = build_runtime_test_report()

        self.assertEqual(report["target"], "all")
        self.assertEqual(report["script"], "/data/adb/achost/bin/runtime-test.sh")
        self.assertIn("MODE=all", report["command"])
        self.assertIn("achost-docker-runtime start", report["steps"])
        self.assertIn("runtime-smoke-docker", report["steps"])
        self.assertIn("runtime-docker-feature-test", report["steps"])
        self.assertIn("achost-docker-runtime stop", report["steps"])
        self.assertIn("achost-lxc-runtime write-configs", report["steps"])
        self.assertIn("achost-lxc-runtime validate-host", report["steps"])
        self.assertIn("achost-lxc-runtime validate-assets", report["steps"])
        self.assertIn("achost-lxc-runtime prepare-bridge", report["steps"])
        self.assertIn("runtime-smoke-lxc", report["steps"])
        self.assertTrue(any("ROOTFS_ASSET" in note for note in report["notes"]))

    def test_supports_kernelsu_module_root(self):
        report = build_runtime_test_report(
            package_root="/data/adb/modules/achost-runtime/achost",
            target="docker",
            out_dir="/data/local/tmp/achost-docker",
        )

        self.assertEqual(report["target"], "docker")
        self.assertIn("/data/adb/modules/achost-runtime/achost/bin/runtime-test.sh", report["command"])
        self.assertIn("achost-runtime-core net-reconcile", report["steps"])
        self.assertIn("runtime-docker-feature-test", report["steps"])
        self.assertNotIn("runtime-smoke-lxc", report["steps"])
        self.assertEqual(report["notes"], [])

    def test_rejects_relative_android_paths(self):
        with self.assertRaises(ValueError):
            build_runtime_test_report(package_root="relative/path")
        with self.assertRaises(ValueError):
            build_runtime_test_report(out_dir="relative/path")

    def test_formats_human_report(self):
        report = build_runtime_test_report(target="network")
        text = format_runtime_test_report(report)

        self.assertIn("Runtime test command:", text)
        self.assertIn("runtime-net-debug", text)


if __name__ == "__main__":
    unittest.main()
