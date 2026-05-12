import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.kernel_detect import parse_config
from achost.verify_config import evaluate_config, has_required_failures


class ConfigTest(unittest.TestCase):
    def test_parse_config_values(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / ".config"
            config.write_text(
                "CONFIG_NAMESPACES=y\n"
                "# CONFIG_PID_NS is not set\n"
                "CONFIG_LOCALVERSION=\"-test\"\n"
            )

            values = parse_config(config)

        self.assertEqual(values["CONFIG_NAMESPACES"], "y")
        self.assertEqual(values["CONFIG_PID_NS"], "not set")
        self.assertEqual(values["CONFIG_LOCALVERSION"], '"-test"')

    def test_verify_config_reports_required_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / ".config"
            config.write_text(
                "CONFIG_NAMESPACES=y\n"
                "CONFIG_UTS_NS=y\n"
                "# CONFIG_PID_NS is not set\n"
                "CONFIG_NET_NS=y\n"
            )
            results = evaluate_config(config, "android-container-host-v1")

        pid_ns = next(item for item in results if item["symbol"] == "CONFIG_PID_NS")
        self.assertFalse(pid_ns["ok"])
        self.assertEqual(pid_ns["level"], "required")
        self.assertTrue(has_required_failures(results))


if __name__ == "__main__":
    unittest.main()
