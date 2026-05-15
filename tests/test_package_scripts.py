import subprocess
import unittest
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = PROJECT_ROOT / "scripts"


class PackageScriptsTest(unittest.TestCase):
    def run_script(self, name: str) -> str:
        result = subprocess.run(
            [str(SCRIPTS / name), "--version", "0.1.4", "--version-code", "5", "--dry-run"],
            cwd=PROJECT_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return result.stdout

    def test_base_script_does_not_pass_feature_assets(self):
        output = self.run_script("package-base.sh")

        self.assertIn("--module-target base", output)
        self.assertIn("--version 0.1.4", output)
        self.assertIn("--version-code 5", output)
        self.assertIn("achost-base-v0.1.4.zip", output)
        self.assertNotIn("--docker-asset", output)
        self.assertNotIn("--compose-asset", output)
        self.assertNotIn("--buildx-asset", output)
        self.assertNotIn("--buildkit-asset", output)
        self.assertNotIn("--lxc-asset", output)

    def test_docker_script_passes_all_release_assets(self):
        output = self.run_script("package-docker.sh")

        self.assertIn("--module-target docker", output)
        self.assertIn("achost-docker-v0.1.4.zip", output)
        self.assertIn("--docker-asset", output)
        self.assertIn("--docker-sha256", output)
        self.assertIn("--compose-asset", output)
        self.assertIn("--compose-sha256", output)
        self.assertIn("--buildx-asset", output)
        self.assertIn("--buildx-sha256", output)
        self.assertIn("--buildkit-asset", output)
        self.assertIn("--buildkit-sha256", output)
        self.assertIn("docker-compose-linux-aarch64", output)
        self.assertIn("buildx-v0.33.0.linux-arm64", output)
        self.assertIn("buildkit-v0.29.0.linux-arm64.tar.gz", output)

    def test_lxc_script_passes_only_lxc_asset(self):
        output = self.run_script("package-lxc.sh")

        self.assertIn("--module-target lxc", output)
        self.assertIn("achost-lxc-v0.1.4.zip", output)
        self.assertIn("--lxc-asset", output)
        self.assertIn("--lxc-sha256", output)
        self.assertNotIn("--docker-asset", output)
        self.assertNotIn("--compose-asset", output)
        self.assertNotIn("--buildx-asset", output)
        self.assertNotIn("--buildkit-asset", output)


if __name__ == "__main__":
    unittest.main()
