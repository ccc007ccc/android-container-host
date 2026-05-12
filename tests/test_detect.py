import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.kernel_detect import detect_kernel


class DetectTest(unittest.TestCase):
    def test_detects_android_kernel_features(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            out = tree / "out"
            (tree / "arch" / "arm64" / "configs").mkdir(parents=True)
            (tree / "drivers" / "android").mkdir(parents=True)
            (tree / "fs" / "overlayfs").mkdir(parents=True)
            (tree / "drivers" / "net").mkdir(parents=True)
            (tree / "net" / "bridge").mkdir(parents=True)
            (tree / "net" / "netfilter").mkdir(parents=True)
            (tree / "kernel" / "cgroup").mkdir(parents=True)
            out.mkdir(parents=True)

            (tree / "Makefile").write_text("VERSION = 4\nPATCHLEVEL = 19\nSUBLEVEL = 311\n")
            (tree / "arch" / "arm64" / "configs" / "lmi_defconfig").write_text("CONFIG_TEST=y\n")
            (tree / "fs" / "overlayfs" / "Kconfig").write_text("config OVERLAY_FS\n")
            (tree / "drivers" / "net" / "veth.c").write_text("")
            (tree / "net" / "bridge" / "Kconfig").write_text("config BRIDGE\n")
            (tree / "net" / "bridge" / "br_netfilter_hooks.c").write_text("")
            (tree / "net" / "netfilter" / "Kconfig").write_text("config NETFILTER_XT_MATCH_QTAGUID\n")
            (tree / "net" / "netfilter" / "xt_qtaguid.c").write_text("")
            (tree / "net" / "Kconfig").write_text("config ANDROID_PARANOID_NETWORK\n")
            (tree / "kernel" / "cgroup" / "cgroup-v1.c").write_text("noprefix\n")
            (tree / "kernel" / "cgroup" / "cgroup.c").write_text("cgroup2\n")
            (tree / "kernel" / "cgroup" / "cpuset.c").write_text("cpuset,noprefix\n")
            (out / ".config").write_text(
                "CONFIG_ARM64=y\n"
                "CONFIG_ANDROID_BINDER_IPC=y\n"
                "CONFIG_OVERLAY_FS=y\n"
                "CONFIG_VETH=y\n"
                "CONFIG_BRIDGE=y\n"
                "# CONFIG_BRIDGE_NETFILTER is not set\n"
                "CONFIG_NETFILTER=y\n"
                "CONFIG_NETFILTER_XT_MATCH_OWNER=y\n"
                "CONFIG_ANDROID_PARANOID_NETWORK=y\n"
                "# CONFIG_PID_NS is not set\n"
            )

            result = detect_kernel(tree, out)

        self.assertEqual(result["kernel_version"], "4.19.311")
        self.assertEqual(result["arch"], "arm64")
        self.assertTrue(result["android_kernel"])
        self.assertIn("arch/arm64/configs/lmi_defconfig", result["defconfig_candidates"])
        self.assertTrue(result["features"]["qtaguid"]["source"])
        self.assertEqual(result["features"]["qtaguid"]["owner_config"], "y")
        self.assertTrue(any("qtaguid" in risk for risk in result["risk"]))


if __name__ == "__main__":
    unittest.main()
