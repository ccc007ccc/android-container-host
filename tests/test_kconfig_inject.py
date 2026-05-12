import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.kconfig_inject import DEFAULT_DESTINATION, inject_kconfig_report, rollback_kconfig_report, validate_destination


class KconfigInjectTest(unittest.TestCase):
    def test_dry_run_reports_actions_without_modifying_tree(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            tree.mkdir()
            (tree / "Kconfig").write_text("mainmenu \"test\"\n")

            report = inject_kconfig_report(tree)

            self.assertEqual(report["mode"], "dry-run")
            self.assertFalse((tree / DEFAULT_DESTINATION).exists())
            self.assertTrue(any(item["action"] == "append-source" and item["needed"] for item in report["actions"]))

    def test_apply_copies_kconfig_and_appends_source_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            tree.mkdir()
            root_kconfig = tree / "Kconfig"
            root_kconfig.write_text("mainmenu \"test\"\n")

            first = inject_kconfig_report(tree, apply=True)
            second = inject_kconfig_report(tree, apply=True)
            root_text = root_kconfig.read_text()

            self.assertEqual(first["mode"], "apply")
            self.assertTrue((tree / DEFAULT_DESTINATION).exists())
            self.assertEqual(root_text.count(first["source_line"]), 1)
            self.assertTrue(second["already_sourced"])

    def test_rollback_removes_injected_kconfig(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            tree.mkdir()
            root_kconfig = tree / "Kconfig"
            root_kconfig.write_text("mainmenu \"test\"\n")
            injected = inject_kconfig_report(tree, apply=True)

            dry_run = rollback_kconfig_report(tree)
            applied = rollback_kconfig_report(tree, apply=True)

            self.assertTrue(dry_run["source_present"])
            self.assertTrue(dry_run["destination_exists"])
            self.assertEqual(applied["mode"], "apply")
            self.assertNotIn(injected["source_line"], root_kconfig.read_text())
            self.assertFalse((tree / DEFAULT_DESTINATION).exists())

    def test_rejects_destination_outside_kernel_tree(self):
        with self.assertRaises(ValueError):
            validate_destination("/tmp/AndroidContainerHost.Kconfig")
        with self.assertRaises(ValueError):
            validate_destination("../AndroidContainerHost.Kconfig")


if __name__ == "__main__":
    unittest.main()
