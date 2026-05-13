import hashlib
import io
import json
import stat
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from achost.runtime_install import (
    BUILDKIT_REQUIRED_BINARIES,
    BUILDX_PLUGIN_REL,
    BUILDX_STANDALONE_REL,
    COMPOSE_PLUGIN_REL,
    COMPOSE_STANDALONE_REL,
    DOCKER_REQUIRED_BINARIES,
    generate_runtime_package,
)


class RuntimeInstallTest(unittest.TestCase):
    def test_manual_package_contains_scripts_and_configs(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual"
            report = generate_runtime_package(output, mode="manual", cgroup_mode="v1")

            nat_script = output / "achost" / "bin" / "container-nat-manager.sh"
            watchdog = output / "achost" / "bin" / "container-network-watchdog.sh"
            validate = output / "achost" / "bin" / "achost-container-validate.sh"
            docker_start = output / "achost" / "bin" / "achost-docker-start.sh"
            docker_config = output / "achost" / "etc" / "docker" / "daemon.json"
            runtime_config = output / "achost" / "etc" / "achost-runtime.conf"
            docker_smoke = output / "achost" / "bin" / "runtime-smoke-docker.sh"
            docker_feature = output / "achost" / "bin" / "runtime-docker-feature-test.sh"
            docker_wrapper = output / "achost" / "wrappers" / "docker"
            lxc_config = output / "achost" / "etc" / "lxc" / "default.conf"
            install_script = output / "install.sh"
            runtime_test = output / "achost" / "bin" / "runtime-test.sh"
            manifest = json.loads((output / "manifest.json").read_text())
            categories = {item["path"]: item["category"] for item in manifest["files"]}

            self.assertEqual(report["mode"], "manual")
            self.assertEqual(report["docker_runtime_mode"], "chroot")
            self.assertTrue(nat_script.exists())
            self.assertTrue(nat_script.stat().st_mode & stat.S_IXUSR)
            nat_text = nat_script.read_text()
            self.assertIn("ensure_ip_rule \"$RETURN_RULE_PRIORITY\"", nat_text)
            self.assertIn("lookup main", nat_text)
            self.assertIn("lookup \"$ensure_policy_uplink\"", nat_text)
            self.assertTrue(watchdog.exists())
            self.assertTrue(watchdog.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(validate.exists())
            self.assertTrue(validate.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(docker_start.exists())
            self.assertTrue(docker_start.stat().st_mode & stat.S_IXUSR)
            self.assertEqual(categories["achost/bin/achost-docker-start.sh"], "docker")
            self.assertEqual(categories["achost/bin/runtime-docker-feature-test.sh"], "docker")
            self.assertEqual(categories["achost/bin/achost-lxc-validate.sh"], "lxc")
            self.assertEqual(categories["achost/bin/container-network-watchdog.sh"], "common")
            self.assertEqual(categories["achost/etc/docker/daemon.json"], "docker")
            self.assertEqual(categories["achost/etc/lxc/default.conf"], "lxc")
            docker_start_text = docker_start.read_text()
            self.assertIn("wait_for_bridge", docker_start_text)
            self.assertIn("network reconciled bridge=", docker_start_text)
            self.assertIn("/dev/memcg", docker_start_text)
            self.assertIn("ensure_host_memory_cgroup", docker_start_text)
            self.assertIn("mount_chroot_memory_cgroup_v1", docker_start_text)
            self.assertIn("ACHOST_NATIVE_ROOT", docker_start_text)
            self.assertIn("--native-root", docker_start_text)
            self.assertTrue(docker_smoke.stat().st_mode & stat.S_IXUSR)
            docker_smoke_text = docker_smoke.read_text()
            self.assertIn('DOCKER_SMOKE_MODE="${DOCKER_SMOKE_MODE:-local}"', docker_smoke_text)
            self.assertIn("docker import local smoke image", docker_smoke_text)
            self.assertTrue(docker_feature.stat().st_mode & stat.S_IXUSR)
            self.assertIn("docker exec", docker_feature.read_text())
            self.assertTrue(docker_wrapper.stat().st_mode & stat.S_IXUSR)
            docker_wrapper_text = docker_wrapper.read_text()
            self.assertIn("achost-container-env.sh", docker_wrapper_text)
            self.assertIn('exec "$ACHOST/bin/docker"', docker_wrapper_text)
            self.assertFalse((output / "system" / "bin" / "docker").exists())
            self.assertIn("ACHOST_RUNTIME_MODE=chroot", runtime_config.read_text())
            self.assertIn("ACHOST_USE_CHROOT=1", runtime_config.read_text())
            self.assertIn("ACHOST_CGROUP_MODE=v1", runtime_config.read_text())
            docker_daemon = json.loads(docker_config.read_text())
            self.assertIn("native.cgroupdriver=cgroupfs", docker_daemon["exec-opts"])
            self.assertFalse(docker_daemon["iptables"])
            self.assertFalse(docker_daemon["ip6tables"])
            self.assertEqual(docker_daemon["dns-opts"], ["use-vc"])
            self.assertEqual(docker_daemon["runtimes"]["runc-nopivot"]["options"]["BinaryName"], "@ACHOST_PREFIX@/bin/runc")
            self.assertIn("/data/adb/achost/etc/lxc/android-common.conf", lxc_config.read_text())
            self.assertTrue(install_script.stat().st_mode & stat.S_IXUSR)
            self.assertIn("stop_old_watchdog", install_script.read_text())
            self.assertTrue(runtime_test.stat().st_mode & stat.S_IXUSR)
            self.assertEqual(manifest["cgroup_mode"], "v1")
            self.assertEqual(manifest["docker_runtime_mode"], "chroot")
            self.assertIsNone(manifest["assets"]["docker"])

    def test_kernelsu_module_contains_module_entrypoints(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            report = generate_runtime_package(output, mode="kernelsu-module", cgroup_mode="v2")

            module_prop = (output / "module.prop").read_text()
            service = output / "service.sh"
            post_fs_data = output / "post-fs-data.sh"
            watchdog = output / "achost" / "bin" / "container-network-watchdog.sh"
            lxc_config = output / "achost" / "etc" / "lxc" / "default.conf"
            docker_config = output / "achost" / "etc" / "docker" / "daemon.json"
            system_docker = output / "system" / "bin" / "docker"

            self.assertEqual(report["install_prefix"], "/data/adb/modules/achost-runtime/achost")
            self.assertIn("id=achost-runtime", module_prop)
            self.assertTrue(service.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(post_fs_data.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(watchdog.stat().st_mode & stat.S_IXUSR)
            self.assertIn("container-network-watchdog.sh", service.read_text())
            self.assertIn("/data/adb/modules/achost-runtime/achost/etc/lxc/android-common.conf", lxc_config.read_text())
            self.assertIn('"bip": "172.31.0.1/16"', docker_config.read_text())
            self.assertNotIn('"bridge": "docker0"', docker_config.read_text())
            self.assertTrue(system_docker.stat().st_mode & stat.S_IXUSR)
            system_docker_text = system_docker.read_text()
            self.assertIn("/data/adb/modules/achost-runtime/achost", system_docker_text)
            self.assertIn("achost-container-env.sh", system_docker_text)
            self.assertIn('exec "$ACHOST/bin/docker"', system_docker_text)

    def test_kernelsu_module_can_start_docker_on_boot(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            generate_runtime_package(output, mode="kernelsu-module", start_docker_on_boot=True)

            service = (output / "service.sh").read_text()
            self.assertIn("achost-docker-start.sh", service)

    def test_native_runtime_mode_writes_config_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual-native"
            report = generate_runtime_package(output, docker_runtime_mode="native")
            manifest = json.loads((output / "manifest.json").read_text())
            runtime_config = (output / "achost" / "etc" / "achost-runtime.conf").read_text()
            docker_start = (output / "achost" / "bin" / "achost-docker-start.sh").read_text()
            env = (output / "achost" / "bin" / "achost-container-env.sh").read_text()

            self.assertEqual(report["docker_runtime_mode"], "native")
            self.assertEqual(manifest["docker_runtime_mode"], "native")
            self.assertIn("ACHOST_RUNTIME_MODE=native", runtime_config)
            self.assertIn("ACHOST_USE_CHROOT=0", runtime_config)
            self.assertIn("ACHOST_CGROUP_MODE=v1", runtime_config)
            self.assertIn("runtime_mode=", docker_start)
            self.assertIn("native_preflight", docker_start)
            self.assertIn("setup_native_root_files", docker_start)
            self.assertIn("daemon_namespace_diagnostics", docker_start)
            self.assertIn("--native-root", docker_start)
            self.assertIn("ACHOST_RUNTIME_CONF", env)
            self.assertIn("ACHOST_BIND_PATHS", env)
            self.assertIn("ACHOST_NATIVE_ROOT", env)
            self.assertIn("bind_chroot_path \"$bind_path\"", docker_start)

    def test_cgroup_v2_mode_writes_runtime_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual"
            report = generate_runtime_package(output, cgroup_mode="v2")
            manifest = json.loads((output / "manifest.json").read_text())
            runtime_config = (output / "achost" / "etc" / "achost-runtime.conf").read_text()
            docker_start = (output / "achost" / "bin" / "achost-docker-start.sh").read_text()

            self.assertEqual(report["cgroup_mode"], "v2")
            self.assertEqual(manifest["cgroup_mode"], "v2")
            self.assertIn("ACHOST_CGROUP_MODE=v2", runtime_config)
            self.assertIn("cgroup_mode=", docker_start)
            self.assertIn("setup_chroot_cgroups_v2", docker_start)
            self.assertIn("mount_chroot_memory_cgroup_v1", docker_start)

    def test_refuses_unknown_docker_runtime_mode(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "unsupported docker runtime mode"):
                generate_runtime_package(Path(tmp) / "manual", docker_runtime_mode="bad")

    def test_docker_asset_extracts_binaries_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "docker-static-aarch64.tgz"
            self.write_docker_asset(asset)
            digest = hashlib.sha256(asset.read_bytes()).hexdigest()
            output = tmp_path / "manual"

            report = generate_runtime_package(output, docker_asset=asset, docker_sha256=digest)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertEqual(report["assets"]["docker"]["sha256"], digest)
            self.assertEqual(manifest["assets"]["docker"]["sha256"], digest)
            for name in DOCKER_REQUIRED_BINARIES:
                binary = output / "achost" / "bin" / name
                self.assertTrue(binary.exists(), name)
                self.assertTrue(binary.stat().st_mode & stat.S_IXUSR, name)
            asset_entries = [item for item in manifest["files"] if item.get("asset") == "docker"]
            self.assertEqual({Path(item["path"]).name for item in asset_entries}, set(DOCKER_REQUIRED_BINARIES))

    def test_docker_asset_extracts_embedded_compose_plugin_when_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "docker-static-aarch64.tgz"
            self.write_docker_asset(asset, include_compose=True)
            output = tmp_path / "manual"

            report = generate_runtime_package(output, docker_asset=asset)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertEqual(report["assets"]["compose"]["embedded_in"], "docker")
            for rel_path in (COMPOSE_PLUGIN_REL, COMPOSE_STANDALONE_REL):
                compose = output / rel_path
                self.assertTrue(compose.exists(), rel_path)
                self.assertTrue(compose.stat().st_mode & stat.S_IXUSR, rel_path)
            compose_entries = [item for item in manifest["files"] if item.get("asset") == "compose"]
            self.assertEqual({item["path"] for item in compose_entries}, {COMPOSE_PLUGIN_REL, COMPOSE_STANDALONE_REL})

    def test_explicit_compose_asset_installs_plugin_and_overrides_embedded(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            docker_asset = tmp_path / "docker-static-aarch64.tgz"
            compose_asset = tmp_path / "docker-compose-linux-aarch64"
            self.write_docker_asset(docker_asset, include_compose=True)
            self.write_single_binary(compose_asset, "explicit compose")
            output = tmp_path / "manual"

            report = generate_runtime_package(output, docker_asset=docker_asset, compose_asset=compose_asset)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertIsNone(report["assets"]["compose"]["member"])
            self.assertEqual(manifest["assets"]["compose"]["source"], str(compose_asset.resolve()))
            for rel_path in (COMPOSE_PLUGIN_REL, COMPOSE_STANDALONE_REL):
                compose = output / rel_path
                self.assertTrue(compose.exists(), rel_path)
                self.assertTrue(compose.stat().st_mode & stat.S_IXUSR, rel_path)
                self.assertIn("explicit compose", compose.read_text())

    def test_buildx_asset_installs_plugin_and_standalone(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "buildx-v0.test.linux-arm64"
            self.write_single_binary(asset, "buildx test")
            digest = hashlib.sha256(asset.read_bytes()).hexdigest()
            output = tmp_path / "manual"

            report = generate_runtime_package(output, buildx_asset=asset, buildx_sha256=digest)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertEqual(report["assets"]["buildx"]["sha256"], digest)
            self.assertEqual(manifest["assets"]["buildx"]["sha256"], digest)
            for rel_path in (BUILDX_PLUGIN_REL, BUILDX_STANDALONE_REL):
                buildx = output / rel_path
                self.assertTrue(buildx.exists(), rel_path)
                self.assertTrue(buildx.stat().st_mode & stat.S_IXUSR, rel_path)

    def test_buildkit_asset_extracts_required_binaries(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "buildkit-linux-arm64.tar.gz"
            self.write_buildkit_asset(asset)
            output = tmp_path / "manual"

            report = generate_runtime_package(output, buildkit_asset=asset)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertEqual(set(report["assets"]["buildkit"]["files"]), set(BUILDKIT_REQUIRED_BINARIES))
            self.assertEqual(set(manifest["assets"]["buildkit"]["files"]), set(BUILDKIT_REQUIRED_BINARIES))
            for name in BUILDKIT_REQUIRED_BINARIES:
                binary = output / "achost" / "bin" / name
                self.assertTrue(binary.exists(), name)
                self.assertTrue(binary.stat().st_mode & stat.S_IXUSR, name)

    def test_refuses_new_asset_checksum_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            asset = Path(tmp) / "buildx"
            self.write_single_binary(asset, "buildx test")

            with self.assertRaisesRegex(ValueError, "sha256 mismatch"):
                generate_runtime_package(Path(tmp) / "manual", buildx_asset=asset, buildx_sha256="0" * 64)

    def test_refuses_buildkit_asset_missing_required_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "buildkit-linux-arm64.tar.gz"
            self.write_buildkit_asset(asset, names=("buildctl",))

            with self.assertRaisesRegex(ValueError, "buildkit asset missing required binaries"):
                generate_runtime_package(tmp_path / "manual", buildkit_asset=asset)

    def test_refuses_missing_docker_asset(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(FileNotFoundError):
                generate_runtime_package(Path(tmp) / "manual", docker_asset=Path(tmp) / "missing.tgz")

    def test_refuses_docker_asset_checksum_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "docker-static-aarch64.tgz"
            self.write_docker_asset(asset)

            with self.assertRaisesRegex(ValueError, "sha256 mismatch"):
                generate_runtime_package(tmp_path / "manual", docker_asset=asset, docker_sha256="0" * 64)

    def test_refuses_docker_asset_missing_required_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "docker-static-aarch64.tgz"
            self.write_docker_asset(asset, names=DOCKER_REQUIRED_BINARIES[:-1])

            with self.assertRaisesRegex(ValueError, "docker asset missing required binaries"):
                generate_runtime_package(tmp_path / "manual", docker_asset=asset)

    def test_refuses_non_empty_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "existing"
            output.mkdir()
            (output / "keep.txt").write_text("do not overwrite\n")

            with self.assertRaises(FileExistsError):
                generate_runtime_package(output)

    def write_docker_asset(self, path: Path, names=DOCKER_REQUIRED_BINARIES, include_compose=False):
        with tarfile.open(path, "w:gz") as archive:
            for name in names:
                data = f"#!/system/bin/sh\nprintf '{name} test binary\\n'\n".encode()
                info = tarfile.TarInfo(f"docker/{name}")
                info.size = len(data)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(data))
            if include_compose:
                data = b"#!/system/bin/sh\nprintf 'docker compose test plugin\\n'\n"
                info = tarfile.TarInfo("docker/cli-plugins/docker-compose")
                info.size = len(data)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(data))

    def write_single_binary(self, path: Path, label: str):
        path.write_text(f"#!/system/bin/sh\nprintf '{label}\\n'\n")
        path.chmod(0o755)

    def write_buildkit_asset(self, path: Path, names=BUILDKIT_REQUIRED_BINARIES):
        with tarfile.open(path, "w:gz") as archive:
            for name in names:
                data = f"#!/system/bin/sh\nprintf '{name} test binary\\n'\n".encode()
                info = tarfile.TarInfo(f"bin/{name}")
                info.size = len(data)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(data))


if __name__ == "__main__":
    unittest.main()
