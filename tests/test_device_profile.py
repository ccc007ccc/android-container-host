import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.device_profile import device_default_defconfig, device_default_profiles, load_device_profile


class DeviceProfileTest(unittest.TestCase):
    def test_loads_nested_values_and_lists(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "device.yml"
            path.write_text(
                "id: demo\n"
                "kernel:\n"
                "  defconfig: arch/arm64/configs/demo_defconfig\n"
                "container:\n"
                "  default_cgroup_mode: v1\n"
                "  profiles:\n"
                "    - android-container-host-v1\n"
                "    - docker-bridge-overlay2\n"
                "network:\n"
                "  uplink_auto_detect: true\n"
                "known_limitations:\n"
                "  - qtaguid placeholder\n"
            )

            profile = load_device_profile(path)

        self.assertEqual(profile["id"], "demo")
        self.assertEqual(profile["kernel"]["defconfig"], "arch/arm64/configs/demo_defconfig")
        self.assertEqual(profile["container"]["profiles"], ["android-container-host-v1", "docker-bridge-overlay2"])
        self.assertTrue(profile["network"]["uplink_auto_detect"])
        self.assertEqual(profile["known_limitations"], ["qtaguid placeholder"])
        self.assertEqual(device_default_defconfig(profile), "arch/arm64/configs/demo_defconfig")
        self.assertEqual(device_default_profiles(profile), "android-container-host-v1,docker-bridge-overlay2")


if __name__ == "__main__":
    unittest.main()
