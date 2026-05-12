import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.config_merge import merge_config_files, parse_config_line, render_config


class ConfigMergeTest(unittest.TestCase):
    def test_parse_config_line(self):
        self.assertEqual(parse_config_line("CONFIG_PID_NS=y"), ("CONFIG_PID_NS", "y"))
        self.assertEqual(parse_config_line("# CONFIG_ANDROID_PARANOID_NETWORK is not set"), ("CONFIG_ANDROID_PARANOID_NETWORK", "not set"))
        self.assertIsNone(parse_config_line("# plain comment"))

    def test_merge_config_files_applies_fragment_overrides(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            base = root / "base.config"
            fragment = root / "fragment.config"
            output = root / "merged.config"
            base.write_text(
                "CONFIG_NAMESPACES=y\n"
                "# CONFIG_PID_NS is not set\n"
                "CONFIG_ANDROID_PARANOID_NETWORK=y\n"
            )
            fragment.write_text(
                "CONFIG_PID_NS=y\n"
                "# CONFIG_ANDROID_PARANOID_NETWORK is not set\n"
                "CONFIG_NEW_FEATURE=y\n"
            )

            report = merge_config_files(base, output, [fragment])
            merged = output.read_text()

        self.assertIn("CONFIG_NAMESPACES=y", merged)
        self.assertIn("CONFIG_PID_NS=y", merged)
        self.assertIn("# CONFIG_ANDROID_PARANOID_NETWORK is not set", merged)
        self.assertIn("CONFIG_NEW_FEATURE=y", merged)
        self.assertEqual(report["summary"]["changes"], 3)
        self.assertEqual(report["summary"]["added"], 1)

    def test_render_config_writes_unset_format(self):
        text = render_config({"CONFIG_A": "y", "CONFIG_B": "not set"}, ["CONFIG_A", "CONFIG_B"])

        self.assertEqual(text, "CONFIG_A=y\n# CONFIG_B is not set\n")


if __name__ == "__main__":
    unittest.main()
