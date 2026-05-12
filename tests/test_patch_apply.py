import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.patch_apply import apply_patch_report, discover_patches, kernel_family_from_version, list_patch_report


class PatchApplyTest(unittest.TestCase):
    def test_kernel_family_from_version(self):
        self.assertEqual(kernel_family_from_version("4.19.311"), "linux-4.19")
        self.assertEqual(kernel_family_from_version("linux-4.19"), "linux-4.19")

    def test_discover_patches_reads_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            family = root / "linux-4.19"
            family.mkdir()
            (family / "demo.patch").write_text("diff --git a/a b/a\n")
            (family / "demo.md").write_text(
                "---\n"
                "name: demo\n"
                "kernel_family: linux-4.19\n"
                "status: ready\n"
                "default_enabled: true\n"
                "risk: low\n"
                "affected_files: a, b\n"
                "summary: demo patch\n"
                "---\n"
            )

            patches = discover_patches("linux-4.19", root)

        self.assertEqual(len(patches), 1)
        self.assertEqual(patches[0].name, "demo")
        self.assertTrue(patches[0].default_enabled)
        self.assertEqual(patches[0].affected_files, ("a", "b"))

    def test_apply_patch_report_dry_run_success_and_placeholder_skip(self):
        project_root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            tree.mkdir()
            subprocess.run(["git", "init"], cwd=tree, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
            (tree / "Makefile").write_text("VERSION = 4\nPATCHLEVEL = 19\nSUBLEVEL = 311\n")
            (tree / "foo.txt").write_text("old\n")
            patch_root = project_root / "patches" / "linux-4.19"
            patch_root.mkdir(parents=True, exist_ok=True)
            patch_path = patch_root / "unit-test-demo.patch"
            doc_path = patch_root / "unit-test-demo.md"
            patch_path.write_text(
                "diff --git a/foo.txt b/foo.txt\n"
                "--- a/foo.txt\n"
                "+++ b/foo.txt\n"
                "@@ -1 +1 @@\n"
                "-old\n"
                "+new\n"
            )
            doc_path.write_text(
                "---\n"
                "name: unit-test-demo\n"
                "kernel_family: linux-4.19\n"
                "status: ready\n"
                "default_enabled: true\n"
                "risk: low\n"
                "---\n"
            )
            try:
                report = apply_patch_report(tree, kernel_version="linux-4.19", patch_names=["unit-test-demo"])
            finally:
                patch_path.unlink(missing_ok=True)
                doc_path.unlink(missing_ok=True)

        self.assertTrue(report["ok"])
        self.assertTrue(report["results"][0]["check"]["ok"])

    def test_list_patch_report_marks_placeholder(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "kernel"
            tree.mkdir()
            (tree / "Makefile").write_text("VERSION = 4\nPATCHLEVEL = 19\nSUBLEVEL = 311\n")
            report = list_patch_report(tree)

        qtaguid = next(item for item in report["patches"] if item["name"] == "xt-qtaguid-container-safe")
        self.assertEqual(qtaguid["status"], "placeholder")
        self.assertIn("not runnable", qtaguid["skip_reason"])


if __name__ == "__main__":
    unittest.main()
