import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.docker.moby_check_config import parse_moby_output, run_moby_check


class MobyCheckConfigTest(unittest.TestCase):
    def test_parse_moby_output_classifies_sections(self):
        output = """
Generally Necessary:
- CONFIG_NAMESPACES: enabled
- CONFIG_PID_NS: missing
Optional Features:
- CONFIG_USER_NS: missing
- CONFIG_AUFS_FS: missing
- CONFIG_CGROUP_BPF: enabled
"""
        report = parse_moby_output(output)

        categories = {entry["name"]: entry["category"] for entry in report["entries"]}
        self.assertEqual(categories["CONFIG_NAMESPACES"], "Docker required")
        self.assertEqual(categories["CONFIG_PID_NS"], "Docker required")
        self.assertEqual(categories["CONFIG_USER_NS"], "Docker recommended")
        self.assertEqual(categories["CONFIG_AUFS_FS"], "ignored because Android")
        self.assertEqual(categories["CONFIG_CGROUP_BPF"], "Android-specific present")
        self.assertEqual(len(report["missing"]), 3)

    def test_run_moby_check_uses_script_and_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = root / "check-config.sh"
            config = root / ".config"
            script.write_text(
                "#!/usr/bin/env bash\n"
                "echo 'Generally Necessary:'\n"
                "echo '- CONFIG_PID_NS: missing'\n"
                "exit 1\n"
            )
            script.chmod(0o755)
            config.write_text("# CONFIG_PID_NS is not set\n")

            report = run_moby_check(script, config)

        self.assertEqual(report["exit_code"], 1)
        self.assertEqual(report["missing"][0]["name"], "CONFIG_PID_NS")


if __name__ == "__main__":
    unittest.main()
