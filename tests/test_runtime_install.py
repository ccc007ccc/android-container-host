import hashlib
import io
import json
import os
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
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
    LXC_REQUIRED_BINARIES,
    STALE_RUNTIME_ENTRYPOINTS,
    create_runtime_zip,
    generate_runtime_package,
)


class RuntimeInstallTest(unittest.TestCase):
    def webroot_text(self, output: Path) -> str:
        webroot = output / "webroot"
        chunks = []
        for path in sorted(webroot.rglob("*")):
            if path.is_file():
                chunks.append(path.read_text(encoding="utf-8", errors="ignore"))
        return "\n".join(chunks)

    def test_manual_package_contains_scripts_and_configs(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual"
            report = generate_runtime_package(output, mode="manual", cgroup_mode="v1")

            common_wrappers = [
                output / "achost" / "bin" / "detect-uplink.sh",
                output / "achost" / "bin" / "container-nat-manager.sh",
                output / "achost" / "bin" / "container-network-watchdog.sh",
                output / "achost" / "bin" / "protect-container-daemons.sh",
            ]
            validate = output / "achost" / "bin" / "achost-container-validate.sh"
            docker_start = output / "achost" / "bin" / "achost-docker-start.sh"
            docker_stop = output / "achost" / "bin" / "achost-docker-stop.sh"
            docker_runtime = output / "achost" / "bin" / "achost-docker-runtime"
            webui_api = output / "achost" / "bin" / "achost-webui-api"
            webui_api_wrapper = output / "achost" / "bin" / "achost-webui-api.sh"
            runtime_core = output / "achost" / "bin" / "achost-runtime-core"
            lxc_runtime = output / "achost" / "bin" / "achost-lxc-runtime"
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
            self.assertEqual(report["docker_runtime_mode"], "native")
            for wrapper in common_wrappers:
                self.assertFalse(wrapper.exists())
            self.assertTrue(validate.exists())
            self.assertTrue(validate.stat().st_mode & stat.S_IXUSR)
            self.assertFalse(docker_start.exists())
            self.assertFalse(docker_stop.exists())
            self.assertTrue(docker_runtime.exists())
            self.assertTrue(docker_runtime.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(webui_api.exists())
            self.assertTrue(webui_api.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(runtime_core.exists())
            self.assertTrue(runtime_core.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(lxc_runtime.exists())
            self.assertTrue(lxc_runtime.stat().st_mode & stat.S_IXUSR)
            self.assertEqual(report["assets"]["runtime_core"]["path"], "achost/bin/achost-runtime-core")
            self.assertEqual(report["assets"]["docker_runtime"]["path"], "achost/bin/achost-docker-runtime")
            self.assertEqual(report["assets"]["lxc_runtime"]["path"], "achost/bin/achost-lxc-runtime")
            webui_api_wrapper_text = webui_api_wrapper.read_text()
            self.assertIn('exec "$SCRIPT_DIR/achost-webui-api" "$@"', webui_api_wrapper_text)
            self.assertNotIn("status_json()", webui_api_wrapper_text)
            self.assertEqual(categories["achost/bin/achost-runtime-core"], "common")
            self.assertNotIn("achost/bin/achost-docker-start.sh", categories)
            self.assertNotIn("achost/bin/achost-docker-stop.sh", categories)
            self.assertEqual(categories["achost/bin/achost-docker-runtime"], "docker")
            self.assertEqual(categories["achost/bin/achost-webui-api"], "webui")
            self.assertEqual(categories["achost/bin/runtime-docker-feature-test.sh"], "docker")
            self.assertEqual(categories["achost/bin/achost-lxc-runtime"], "lxc")
            self.assertEqual(categories["achost/bin/achost-lxc-validate.sh"], "lxc")
            for wrapper_path in [
                "achost/bin/detect-uplink.sh",
                "achost/bin/container-nat-manager.sh",
                "achost/bin/container-network-watchdog.sh",
                "achost/bin/protect-container-daemons.sh",
            ]:
                self.assertNotIn(wrapper_path, categories)
            self.assertEqual(categories["achost/etc/docker/daemon.json"], "docker")
            self.assertEqual(categories["achost/etc/lxc/default.conf"], "lxc")
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
            self.assertIn("ACHOST_RUNTIME_MODE=native", runtime_config.read_text())
            self.assertIn("ACHOST_USE_CHROOT=0", runtime_config.read_text())
            self.assertIn("ACHOST_CGROUP_MODE=v1", runtime_config.read_text())
            docker_daemon = json.loads(docker_config.read_text())
            self.assertIn("native.cgroupdriver=cgroupfs", docker_daemon["exec-opts"])
            self.assertFalse(docker_daemon["iptables"])
            self.assertFalse(docker_daemon["ip6tables"])
            self.assertEqual(docker_daemon["dns-opts"], ["use-vc"])
            self.assertEqual(docker_daemon["runtimes"]["runc-nopivot"]["options"]["BinaryName"], "@ACHOST_PREFIX@/bin/runc")
            self.assertIn("/data/adb/achost/etc/lxc/android-common.conf", lxc_config.read_text())
            self.assertIn("lxc.net.0.link = lxcbr0", lxc_config.read_text())
            self.assertTrue(install_script.stat().st_mode & stat.S_IXUSR)
            install_text = install_script.read_text()
            self.assertIn("stop_old_watchdog", install_text)
            self.assertIn('prune_stale_runtime_entrypoints "$DEST/bin"', install_text)
            for stale_entrypoint in STALE_RUNTIME_ENTRYPOINTS:
                self.assertIn(stale_entrypoint, install_text)
            self.assertTrue(runtime_test.stat().st_mode & stat.S_IXUSR)
            self.assertEqual(manifest["cgroup_mode"], "v1")
            self.assertEqual(manifest["docker_runtime_mode"], "native")
            self.assertIsNone(manifest["assets"]["docker"])

    def test_manual_install_prunes_stale_runtime_entrypoints(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            package = tmp_path / "manual"
            dest = tmp_path / "dest"
            generate_runtime_package(package, mode="manual")
            stale_bin = dest / "bin"
            stale_bin.mkdir(parents=True)
            for stale_entrypoint in STALE_RUNTIME_ENTRYPOINTS:
                stale_path = stale_bin / stale_entrypoint
                stale_path.write_text("stale")
                stale_path.chmod(0o755)

            env = os.environ.copy()
            env["DEST"] = str(dest)
            subprocess.run(
                ["bash", str(package / "install.sh")],
                check=True,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            for stale_entrypoint in STALE_RUNTIME_ENTRYPOINTS:
                self.assertFalse((stale_bin / stale_entrypoint).exists())
            self.assertTrue((dest / "bin" / "achost-runtime-core").exists())
            self.assertTrue((dest / "bin" / "achost-docker-runtime").exists())
            self.assertTrue((dest / "bin" / "achost-lxc-runtime").exists())

    def test_kernelsu_module_contains_module_entrypoints(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            report = generate_runtime_package(output, mode="kernelsu-module", cgroup_mode="v2")

            module_prop = (output / "module.prop").read_text()
            service = output / "service.sh"
            post_fs_data = output / "post-fs-data.sh"
            customize = output / "customize.sh"
            uninstall = output / "uninstall.sh"
            runtime_config = output / "achost" / "etc" / "achost-runtime.conf"
            runtime_core = output / "achost" / "bin" / "achost-runtime-core"
            lxc_runtime = output / "achost" / "bin" / "achost-lxc-runtime"
            lxc_config = output / "achost" / "etc" / "lxc" / "default.conf"
            docker_config = output / "achost" / "etc" / "docker" / "daemon.json"
            webroot_index = output / "webroot" / "index.html"

            self.assertEqual(report["install_prefix"], "/data/adb/modules/achost-runtime/achost")
            self.assertIn("id=achost-runtime", module_prop)
            self.assertTrue(service.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(post_fs_data.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(customize.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(uninstall.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(runtime_core.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(lxc_runtime.exists())
            self.assertTrue(lxc_runtime.stat().st_mode & stat.S_IXUSR)
            self.assertIn("ACHOST_VAR=/data/adb/achost-runtime", runtime_config.read_text())
            self.assertIn("ACHOST_CHROOT=/data/adb/achost-runtime/chroot", runtime_config.read_text())
            service_text = service.read_text()
            customize_text = customize.read_text()
            uninstall_text = uninstall.read_text()
            self.assertIn("achost-runtime-core", service_text)
            self.assertIn("protect-daemons", service_text)
            self.assertIn("net-watchdog", service_text)
            self.assertNotIn('"$COMMON_BIN/container-network-watchdog.sh"', service_text)
            self.assertIn('prune_stale_runtime_entrypoints "$ACHOST/bin"', service_text)
            self.assertIn('prune_stale_runtime_entrypoints "$ACHOST/bin"', customize_text)
            for stale_entrypoint in STALE_RUNTIME_ENTRYPOINTS:
                self.assertIn(stale_entrypoint, service_text)
                self.assertIn(stale_entrypoint, customize_text)
            self.assertIn('"$ACHOST_DATA/containerd/root"', customize_text)
            self.assertIn("/data/adb/ksu/bin", service_text)
            self.assertIn("/data/adb/ksu/bin", customize_text)
            self.assertIn("ACHOST_DOCKER_WRAPPER", service_text)
            self.assertIn("ACHOST_DOCKER_WRAPPER", customize_text)
            self.assertIn("ACHOST_LXC_WRAPPER", service_text)
            self.assertIn("ACHOST_LXC_WRAPPER", customize_text)
            self.assertIn('exec "$ACHOST/lxc/bin/$name" "$@"', service_text)
            self.assertIn("achost-docker-runtime", uninstall_text)
            self.assertIn('"$ACHOST/bin/achost-docker-runtime" stop', uninstall_text)
            self.assertIn("grep -q 'ACHOST_DOCKER_WRAPPER'", uninstall_text)
            self.assertIn("rm -f /data/adb/ksu/bin/docker", uninstall_text)
            self.assertIn("grep -q 'ACHOST_LXC_WRAPPER'", uninstall_text)
            self.assertIn("/data/adb/ksu/bin/lxc*", uninstall_text)
            self.assertIn("/data/adb/ksu/bin/lxd*", uninstall_text)
            self.assertIn("/data/local/tmp/achost-network-watchdog.pid", uninstall_text)
            self.assertIn("/data/adb/modules/achost-runtime/achost/etc/lxc/android-common.conf", lxc_config.read_text())
            self.assertIn("lxc.net.0.link = lxcbr0", lxc_config.read_text())
            self.assertIn('"bip": "172.31.0.1/16"', docker_config.read_text())
            self.assertNotIn('"bridge": "docker0"', docker_config.read_text())
            self.assertFalse((output / "system" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "xbin" / "docker").exists())
            self.assertTrue(webroot_index.exists())
            webroot_index_text = webroot_index.read_text()
            self.assertIn("ACHost Docker", webroot_index_text)
            self.assertIn('name="achost-webui-config"', webroot_index_text)
            self.assertIn('&quot;moduleTarget&quot;:&quot;legacy&quot;', webroot_index_text)
            manifest = json.loads((output / "manifest.json").read_text())
            categories = {item["path"]: item["category"] for item in manifest["files"]}
            self.assertEqual(categories["webroot/index.html"], "webui")

    def test_kernelsu_module_zip_contains_root_entries(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            generate_runtime_package(output, mode="kernelsu-module")
            zip_path = create_runtime_zip(output)

            with zipfile.ZipFile(zip_path) as archive:
                names = set(archive.namelist())

            self.assertIn("module.prop", names)
            self.assertIn("service.sh", names)
            self.assertIn("customize.sh", names)
            self.assertIn("uninstall.sh", names)
            self.assertIn("webroot/index.html", names)
            self.assertNotIn("achost/bin/achost-docker-start.sh", names)
            self.assertNotIn("achost/bin/achost-docker-stop.sh", names)
            self.assertIn("achost/bin/achost-docker-runtime", names)
            self.assertIn("achost/bin/achost-lxc-runtime", names)
            self.assertIn("achost/bin/achost-runtime-core", names)
            self.assertIn("achost/bin/achost-webui-api", names)
            self.assertFalse(any(name.startswith("system/") and name.endswith("/docker") for name in names))

    def test_kernelsu_base_module_excludes_feature_payloads(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "base"
            report = generate_runtime_package(output, mode="kernelsu-module", module_target="base")
            manifest = json.loads((output / "manifest.json").read_text())
            paths = {item["path"] for item in manifest["files"]}
            env_text = (output / "achost" / "bin" / "achost-container-env.sh").read_text()
            service_text = (output / "service.sh").read_text()
            customize_text = (output / "customize.sh").read_text()

            self.assertEqual(report["module_id"], "achost-base")
            self.assertEqual(manifest["module_target"], "base")
            self.assertEqual(manifest["requires"], [])
            self.assertEqual(manifest["provides"], ["common"])
            self.assertIn("achost/bin/achost-container-env.sh", paths)
            self.assertIn('LXC_TEMPLATE_PATH="${LXC_TEMPLATE_PATH:-$ACHOST_LXC/share/lxc/templates}"', env_text)
            self.assertIn("achost/bin/achost-runtime-core", paths)
            self.assertIn("achost/bin/achost-supervise", paths)
            for wrapper_path in [
                "achost/bin/detect-uplink.sh",
                "achost/bin/container-nat-manager.sh",
                "achost/bin/container-network-watchdog.sh",
                "achost/bin/protect-container-daemons.sh",
            ]:
                self.assertNotIn(wrapper_path, paths)
            self.assertEqual(report["assets"]["runtime_core"]["path"], "achost/bin/achost-runtime-core")
            self.assertIsNone(report["assets"]["docker_runtime"])
            self.assertIsNone(report["assets"]["lxc_runtime"])
            self.assertNotIn("achost/bin/achost-docker-start.sh", paths)
            self.assertNotIn("achost/bin/achost-docker-stop.sh", paths)
            self.assertNotIn("achost/bin/achost-docker-runtime", paths)
            self.assertNotIn("achost/bin/achost-webui-api", paths)
            self.assertNotIn("achost/bin/achost-lxc-validate.sh", paths)
            self.assertNotIn("achost/bin/achost-lxc-runtime", paths)
            self.assertFalse((output / "system" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "xbin" / "docker").exists())
            self.assertFalse((output / "system" / "product" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "system_ext" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "vendor" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "vendor" / "xbin" / "docker").exists())
            self.assertFalse((output / "webroot" / "index.html").exists())
            self.assertIn('"$ACHOST_NATIVE_ROOT"', service_text)
            self.assertNotIn('"$ACHOST_VAR/docker"', service_text)
            self.assertNotIn('"$ACHOST_VAR/lxc"', service_text)
            self.assertNotIn('"$ACHOST_DATA/docker"', customize_text)
            self.assertNotIn('"$ACHOST_DATA/lxc"', customize_text)
            self.assertNotIn("docker", manifest["included_categories"])
            self.assertNotIn("lxc", manifest["included_categories"])
            self.assertNotIn("webui", manifest["included_categories"])

    def test_kernelsu_docker_module_depends_on_base_and_excludes_lxc(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "docker"
            report = generate_runtime_package(output, mode="kernelsu-module", module_target="docker")
            manifest = json.loads((output / "manifest.json").read_text())
            webui_config = json.loads((output / "webroot" / "achost-webui-config.json").read_text())
            webroot_index_text = (output / "webroot" / "index.html").read_text()
            webroot_text = self.webroot_text(output)
            paths = {item["path"] for item in manifest["files"]}
            module_prop = (output / "module.prop").read_text()
            service_text = (output / "service.sh").read_text()
            customize_text = (output / "customize.sh").read_text()
            uninstall_text = (output / "uninstall.sh").read_text()
            webui_api_wrapper_text = (output / "achost" / "bin" / "achost-webui-api.sh").read_text()

            self.assertEqual(report["module_id"], "achost-docker")
            self.assertEqual(manifest["module_target"], "docker")
            self.assertEqual(manifest["requires"], ["achost-base"])
            self.assertEqual(manifest["provides"], ["docker"])
            self.assertIn("requires=achost-base", module_prop)
            self.assertNotIn("achost/bin/achost-docker-start.sh", paths)
            self.assertNotIn("achost/bin/achost-docker-stop.sh", paths)
            self.assertIn("achost/bin/achost-docker-runtime", paths)
            self.assertIn("achost/bin/achost-webui-api.sh", paths)
            self.assertIn("achost/bin/achost-webui-api", paths)
            self.assertNotIn("achost/bin/achost-runtime-core", paths)
            self.assertNotIn("achost/bin/restore-docker-iptables.sh", paths)
            self.assertIsNone(report["assets"]["runtime_core"])
            self.assertEqual(report["assets"]["docker_runtime"]["path"], "achost/bin/achost-docker-runtime")
            self.assertEqual(report["assets"]["webui_api"]["path"], "achost/bin/achost-webui-api")
            self.assertIn('exec "$SCRIPT_DIR/achost-webui-api" "$@"', webui_api_wrapper_text)
            self.assertNotIn("case \"${1:-}\"", webui_api_wrapper_text)
            self.assertFalse(any(path.startswith("system/") and path.endswith("/docker") for path in paths))
            self.assertIn("webroot/index.html", paths)
            self.assertNotIn("achost/bin/achost-container-env.sh", paths)
            self.assertNotIn("achost/bin/achost-supervise", paths)
            self.assertNotIn("achost/bin/achost-lxc-validate.sh", paths)
            self.assertNotIn("achost/bin/achost-lxc-runtime", paths)
            self.assertIsNone(report["assets"]["lxc_runtime"])
            self.assertIn("/data/adb/ksu/bin", service_text)
            self.assertIn("/data/adb/ksu/bin", customize_text)
            self.assertIn("requires achost-base module", service_text)
            self.assertIn("requires achost-base module", customize_text)
            self.assertIn("keeping existing non-ACHost command", service_text)
            self.assertIn("keeping existing non-ACHost command", customize_text)
            self.assertIn("ACHOST_DOCKER_WRAPPER", service_text)
            self.assertIn("ACHOST_DOCKER_WRAPPER", customize_text)
            self.assertIn("rewrite_docker_mount", service_text)
            self.assertIn("rewrite_docker_mount", customize_text)
            self.assertIn("achost-docker-runtime", service_text)
            self.assertIn('"$ACHOST/bin/achost-docker-runtime" start', service_text)
            self.assertIn('"$ACHOST_VAR/docker"', service_text)
            self.assertIn('"$ACHOST_VAR/containerd/root"', service_text)
            self.assertNotIn('"$ACHOST_VAR/lxc"', service_text)
            self.assertIn('"$ACHOST_DATA/docker"', customize_text)
            self.assertNotIn('"$ACHOST_DATA/lxc"', customize_text)
            self.assertNotIn("lxc-autostart.log", service_text)
            self.assertIn('"$ACHOST/bin/achost-docker-runtime" stop', uninstall_text)
            self.assertIn("grep -q 'ACHOST_DOCKER_WRAPPER'", uninstall_text)
            self.assertIn("rm -f /data/adb/ksu/bin/docker", uninstall_text)
            self.assertEqual(webui_config["api"], "/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh")
            self.assertIn("ACHost Docker", webroot_index_text)
            self.assertIn('&quot;moduleTarget&quot;:&quot;docker&quot;', webroot_index_text)
            self.assertIn('&quot;moduleId&quot;:&quot;achost-docker&quot;', webroot_index_text)
            self.assertIn("Docker 管理面板", webroot_text)
            self.assertIn("拉取镜像", webroot_text)
            self.assertIn("start-docker", webroot_text)
            self.assertNotIn("导入 LXC rootfs", webroot_text)
            self.assertNotIn("lxc-import-rootfs", webroot_text)
            self.assertNotIn("SSH 快速安装", webroot_text)
            self.assertNotIn("lxc", manifest["included_categories"])
            self.assertNotIn("supervisor", manifest["included_categories"])

    def test_docker_wrapper_rewrites_default_socket_mounts(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual"
            generate_runtime_package(output, mode="manual")
            fake_docker = output / "achost" / "bin" / "docker"
            fake_docker.write_text("#!/usr/bin/env sh\nprintf '%s\\n' \"$@\"\n")
            fake_docker.chmod(0o755)

            env = os.environ.copy()
            env["DOCKER_HOST"] = "unix:///data/adb/achost/run/docker.sock"
            result = subprocess.run(
                [
                    "bash",
                    str(output / "achost" / "wrappers" / "docker"),
                    "run",
                    "--name",
                    "portainer",
                    "-v",
                    "/var/run/docker.sock:/var/run/docker.sock",
                    "--volume=/run/docker.sock:/run/docker.sock",
                    "--mount",
                    "type=bind,source=/var/run/docker.sock,target=/docker.sock",
                    "--mount=type=bind,src=/run/docker.sock,target=/run/docker.sock",
                    "6053537/portainer-ce",
                ],
                check=True,
                env=env,
                stdout=subprocess.PIPE,
                text=True,
            )

            args = result.stdout.splitlines()
            self.assertIn("/data/adb/achost/run/docker.sock:/var/run/docker.sock", args)
            self.assertIn("--volume=/data/adb/achost/run/docker.sock:/run/docker.sock", args)
            self.assertIn("type=bind,source=/data/adb/achost/run/docker.sock,target=/docker.sock", args)
            self.assertIn("--mount=type=bind,src=/data/adb/achost/run/docker.sock,target=/run/docker.sock", args)
            self.assertNotIn("/var/run/docker.sock:/var/run/docker.sock", args)
            self.assertNotIn("--volume=/run/docker.sock:/run/docker.sock", args)
            self.assertNotIn("type=bind,source=/var/run/docker.sock,target=/docker.sock", args)
            self.assertNotIn("--mount=type=bind,src=/run/docker.sock,target=/run/docker.sock", args)

    def test_webui_api_wrapper_execs_rust_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "docker"
            generate_runtime_package(output, mode="kernelsu-module", module_target="docker")
            wrapper = output / "achost" / "bin" / "achost-webui-api.sh"
            binary = output / "achost" / "bin" / "achost-webui-api"

            wrapper_text = wrapper.read_text()
            self.assertTrue(binary.exists())
            self.assertTrue(binary.stat().st_mode & stat.S_IXUSR)
            self.assertIn("achost-container-env.sh", wrapper_text)
            self.assertIn("ACHOST_BASE_ENV_PRESENT=1", wrapper_text)
            self.assertIn('exec "$SCRIPT_DIR/achost-webui-api" "$@"', wrapper_text)
            self.assertNotIn("json_escape", wrapper_text)
            self.assertNotIn("normalize_mount_item", wrapper_text)

    def test_kernelsu_lxc_module_depends_on_base_and_excludes_docker(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "lxc"
            report = generate_runtime_package(output, mode="kernelsu-module", module_target="lxc")
            manifest = json.loads((output / "manifest.json").read_text())
            paths = {item["path"] for item in manifest["files"]}
            module_prop = (output / "module.prop").read_text()
            service_text = (output / "service.sh").read_text()
            customize_text = (output / "customize.sh").read_text()
            webui_config = json.loads((output / "webroot" / "achost-webui-config.json").read_text())
            webroot_index_text = (output / "webroot" / "index.html").read_text()
            webroot_text = self.webroot_text(output)
            uninstall_text = (output / "uninstall.sh").read_text()

            self.assertEqual(report["module_id"], "achost-lxc")
            self.assertEqual(manifest["module_target"], "lxc")
            self.assertEqual(manifest["requires"], ["achost-base"])
            self.assertEqual(manifest["provides"], ["lxc"])
            self.assertIn("requires=achost-base", module_prop)
            self.assertIn("achost/bin/achost-lxc-runtime", paths)
            self.assertIn("achost/bin/achost-lxc-validate.sh", paths)
            self.assertIn("achost/etc/lxc/default.conf", paths)
            self.assertEqual(report["assets"]["lxc_runtime"]["path"], "achost/bin/achost-lxc-runtime")
            self.assertNotIn("achost/bin/achost-docker-start.sh", paths)
            self.assertNotIn("achost/bin/achost-docker-stop.sh", paths)
            self.assertNotIn("achost/bin/achost-docker-runtime", paths)
            self.assertIn("achost/bin/achost-webui-api.sh", paths)
            self.assertIn("achost/bin/achost-webui-api", paths)
            self.assertIn("webroot/index.html", paths)
            self.assertIn('chmod 0755 "$ACHOST/lxc/bin"/*', service_text)
            self.assertIn('chmod 0755 "$ACHOST/lxc/bin"/*', customize_text)
            self.assertIn('chmod 0755 "$ACHOST/lxc/share/lxc/templates"/lxc-*', service_text)
            self.assertIn('chmod 0755 "$ACHOST/lxc/share/lxc/templates"/lxc-*', customize_text)
            self.assertIn("/data/adb/ksu/bin", service_text)
            self.assertIn("/data/adb/ksu/bin", customize_text)
            self.assertIn("requires achost-base module", service_text)
            self.assertIn("requires achost-base module", customize_text)
            self.assertIn("keeping existing non-ACHost command", service_text)
            self.assertIn("keeping existing non-ACHost command", customize_text)
            self.assertIn("ACHOST_LXC_WRAPPER", service_text)
            self.assertIn("ACHOST_LXC_WRAPPER", customize_text)
            self.assertIn('exec "$ACHOST/lxc/bin/$name" "$@"', service_text)
            self.assertIn('"$ACHOST/bin/achost-lxc-runtime" autostart', service_text)
            self.assertIn("lxc-autostart.log", service_text)
            self.assertIn('"$ACHOST_VAR/lxc"', service_text)
            self.assertIn('"$ACHOST_VAR/run/lxc"', service_text)
            self.assertNotIn('"$ACHOST_VAR/docker"', service_text)
            self.assertNotIn('"$ACHOST_VAR/containerd/root"', service_text)
            self.assertIn('"$ACHOST_DATA/lxc"', customize_text)
            self.assertNotIn('"$ACHOST_DATA/docker"', customize_text)
            self.assertNotIn('"$ACHOST_DATA/containerd/root"', customize_text)
            self.assertNotIn('"$ACHOST/bin/achost-docker-runtime" start', service_text)
            self.assertIn("grep -q 'ACHOST_LXC_WRAPPER'", uninstall_text)
            self.assertIn("/data/adb/ksu/bin/lxc*", uninstall_text)
            self.assertIn("/data/adb/ksu/bin/lxd*", uninstall_text)
            self.assertEqual(report["assets"]["webui_api"]["path"], "achost/bin/achost-webui-api")
            self.assertNotIn("achost/bin/achost-runtime-core", paths)
            self.assertIsNone(report["assets"]["docker_runtime"])
            self.assertNotIn("lxc_rootfs", report["assets"])
            self.assertNotIn("achost/rootfs/ubuntu-26.04-arm64.tar.gz", paths)
            self.assertNotIn("achost/bin/achost-supervise", paths)
            self.assertIn("ACHOST_LXC_VAR=/data/adb/achost/lxc", (output / "achost" / "etc" / "achost-runtime.conf").read_text())
            self.assertIn("LXC_BRIDGE=lxcbr0", (output / "achost" / "etc" / "achost-runtime.conf").read_text())
            self.assertIn("lxc.net.0.link = lxcbr0", (output / "achost" / "etc" / "lxc" / "default.conf").read_text())
            self.assertNotIn("lxc.idmap", (output / "achost" / "etc" / "lxc" / "unprivileged.conf").read_text())
            self.assertEqual(webui_config["api"], "/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh")
            self.assertIn("ACHost LXC", webroot_index_text)
            self.assertIn('&quot;moduleTarget&quot;:&quot;lxc&quot;', webroot_index_text)
            self.assertIn('&quot;moduleId&quot;:&quot;achost-lxc&quot;', webroot_index_text)
            self.assertIn("LXC 容器面板", webroot_text)
            self.assertIn("导入 LXC rootfs", webroot_text)
            self.assertIn("lxc-import-rootfs", webroot_text)
            self.assertIn("用户密码", webroot_text)
            self.assertIn("lxc-force-stop", webroot_text)
            self.assertIn("lxc-set-autostart", webroot_text)
            self.assertIn("lxc-destroy", webroot_text)
            self.assertIn("删除", webroot_text)
            self.assertNotIn("SSH 快速安装", webroot_text)
            self.assertNotIn("lxc-ssh-quick-install", webroot_text)
            self.assertNotIn("lxc-service", webroot_text)
            self.assertNotIn("Docker 管理面板", webroot_text)
            self.assertNotIn("拉取镜像", webroot_text)
            self.assertNotIn("start-docker", webroot_text)
            self.assertFalse((output / "system" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "xbin" / "docker").exists())
            self.assertFalse((output / "system" / "product" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "system_ext" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "vendor" / "bin" / "docker").exists())
            self.assertFalse((output / "system" / "vendor" / "xbin" / "docker").exists())
            self.assertTrue((output / "webroot" / "index.html").exists())
            self.assertNotIn("docker", manifest["included_categories"])
            self.assertIn("webui", manifest["included_categories"])
            self.assertNotIn("supervisor", manifest["included_categories"])

    def test_ubuntu_lxc_module_target_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "unsupported module target"):
                generate_runtime_package(Path(tmp) / "ubuntu", mode="kernelsu-module", module_target="ubuntu-lxc")

    def test_split_module_asset_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            with self.assertRaisesRegex(ValueError, "base module target does not accept"):
                generate_runtime_package(output, mode="kernelsu-module", module_target="base", docker_asset="docker.tgz")
            with self.assertRaisesRegex(ValueError, "docker module target does not accept"):
                generate_runtime_package(output, mode="kernelsu-module", module_target="docker", lxc_asset="lxc.tgz")
            with self.assertRaisesRegex(ValueError, "lxc module target does not accept"):
                generate_runtime_package(output, mode="kernelsu-module", module_target="lxc", buildx_asset="buildx")
            with self.assertRaisesRegex(ValueError, "start_docker_on_boot requires a Docker module target"):
                generate_runtime_package(Path(tmp) / "base-autostart", mode="kernelsu-module", module_target="base", start_docker_on_boot=True)
            with self.assertRaisesRegex(ValueError, "start_docker_on_boot requires a Docker module target"):
                generate_runtime_package(Path(tmp) / "lxc-autostart", mode="kernelsu-module", module_target="lxc", start_docker_on_boot=True)

    def test_kernelsu_module_can_start_docker_on_boot(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "module"
            generate_runtime_package(output, mode="kernelsu-module", start_docker_on_boot=True)

            service = (output / "service.sh").read_text()
            customize = (output / "customize.sh").read_text()
            self.assertIn("docker.autostart", service)
            self.assertIn("achost-docker-runtime", service)
            self.assertIn('"$ACHOST/bin/achost-docker-runtime" start', service)
            self.assertNotIn('"$ACHOST/bin/achost-docker-start.sh"', service)
            self.assertIn("dockerd-start.log", service)
            self.assertIn("printf '1", service)
            self.assertIn("docker.autostart", customize)
            self.assertIn("achost-runtime-core", service)
            self.assertIn("protect-daemons", service)
            self.assertIn("net-watchdog", service)
            self.assertNotIn('"$COMMON_BIN/protect-container-daemons.sh"', service)
            self.assertNotIn('"$COMMON_BIN/container-network-watchdog.sh"', service)

    def test_native_runtime_mode_writes_config_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual-native"
            report = generate_runtime_package(output, docker_runtime_mode="native")
            manifest = json.loads((output / "manifest.json").read_text())
            runtime_config = (output / "achost" / "etc" / "achost-runtime.conf").read_text()
            env = (output / "achost" / "bin" / "achost-container-env.sh").read_text()

            self.assertEqual(report["docker_runtime_mode"], "native")
            self.assertEqual(manifest["docker_runtime_mode"], "native")
            self.assertIn("ACHOST_RUNTIME_MODE=native", runtime_config)
            self.assertIn("ACHOST_USE_CHROOT=0", runtime_config)
            self.assertIn("ACHOST_CGROUP_MODE=v1", runtime_config)
            self.assertFalse((output / "achost" / "bin" / "achost-docker-start.sh").exists())
            self.assertFalse((output / "achost" / "bin" / "achost-docker-stop.sh").exists())
            self.assertTrue((output / "achost" / "bin" / "achost-docker-runtime").exists())
            self.assertIn('ACHOST_RUNTIME_MODE="${ACHOST_RUNTIME_MODE:-native}"', env)
            self.assertIn("*) ACHOST_RUNTIME_MODE=native; ACHOST_USE_CHROOT_DEFAULT=0 ;;", env)
            self.assertIn("ACHOST_RUNTIME_CONF", env)
            self.assertIn("ACHOST_BIND_PATHS", env)
            self.assertIn("ACHOST_NATIVE_ROOT", env)

    def test_cgroup_v2_mode_writes_runtime_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "manual"
            report = generate_runtime_package(output, cgroup_mode="v2")
            manifest = json.loads((output / "manifest.json").read_text())
            runtime_config = (output / "achost" / "etc" / "achost-runtime.conf").read_text()

            self.assertEqual(report["cgroup_mode"], "v2")
            self.assertEqual(manifest["cgroup_mode"], "v2")
            self.assertIn("ACHOST_CGROUP_MODE=v2", runtime_config)
            self.assertFalse((output / "achost" / "bin" / "achost-docker-start.sh").exists())
            self.assertFalse((output / "achost" / "bin" / "achost-docker-stop.sh").exists())

    def test_refuses_non_native_docker_runtime_mode(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError, "unsupported docker runtime mode"):
                generate_runtime_package(Path(tmp) / "manual", docker_runtime_mode="bad")
            with self.assertRaisesRegex(ValueError, "unsupported docker runtime mode"):
                generate_runtime_package(Path(tmp) / "manual-chroot", docker_runtime_mode="chroot")

    def test_docker_runtime_stop_uses_achost_scoped_process_cleanup(self):
        source = (Path(__file__).resolve().parents[1] / "crates" / "achost-docker-runtime" / "src" / "main.rs").read_text()

        self.assertIn('let dockerd = config.achost_bin.join("dockerd");', source)
        self.assertIn('let containerd = config.achost_bin.join("containerd");', source)
        self.assertIn('stop_pid_file("dockerd", &config.dockerd_pid, &dockerd)', source)
        self.assertIn('stop_owned_processes("dockerd", &dockerd)', source)
        self.assertIn('stop_owned_processes("containerd", &containerd)', source)
        self.assertIn("process_uses_executable", source)
        self.assertIn("stop_network_watchdog", source)
        self.assertNotIn("stop_named_processes", source)
        self.assertNotIn("pids_for_name", source)

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

    def test_lxc_asset_extracts_required_binaries_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "lxc-android-aarch64.tar.gz"
            self.write_lxc_asset(asset)
            digest = hashlib.sha256(asset.read_bytes()).hexdigest()
            output = tmp_path / "manual"

            report = generate_runtime_package(output, lxc_asset=asset, lxc_sha256=digest)
            manifest = json.loads((output / "manifest.json").read_text())

            self.assertEqual(report["assets"]["lxc"]["sha256"], digest)
            self.assertEqual(manifest["assets"]["lxc"]["sha256"], digest)
            self.assertEqual(set(report["assets"]["lxc"]["required_binaries"]), set(LXC_REQUIRED_BINARIES))
            self.assertEqual(set(report["assets"]["lxc"]["files"]), set(LXC_REQUIRED_BINARIES))
            for name in LXC_REQUIRED_BINARIES:
                binary = output / "achost" / "lxc" / "bin" / name
                self.assertTrue(binary.exists(), name)
                self.assertTrue(binary.stat().st_mode & stat.S_IXUSR, name)
            template = output / "achost" / "lxc" / "share" / "lxc" / "templates" / "lxc-download"
            self.assertTrue(template.exists())
            self.assertTrue(template.stat().st_mode & stat.S_IXUSR)
            asset_entries = [item for item in manifest["files"] if item.get("asset") == "lxc"]
            binary_entries = [item for item in asset_entries if Path(item["path"]).parent.as_posix() == "achost/lxc/bin"]
            template_entries = [item for item in asset_entries if item["path"] == "achost/lxc/share/lxc/templates/lxc-download"]
            self.assertEqual({Path(item["path"]).name for item in binary_entries}, set(LXC_REQUIRED_BINARIES))
            self.assertEqual(len(template_entries), 1)
            self.assertTrue(template_entries[0]["executable"])

    def test_refuses_lxc_asset_missing_required_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            asset = tmp_path / "lxc-android-aarch64.tar.gz"
            self.write_lxc_asset(asset, names=LXC_REQUIRED_BINARIES[:-1])

            with self.assertRaisesRegex(ValueError, "lxc asset missing required binaries"):
                generate_runtime_package(tmp_path / "manual", lxc_asset=asset)

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

    def write_lxc_asset(self, path: Path, names=LXC_REQUIRED_BINARIES):
        with tarfile.open(path, "w:gz") as archive:
            for name in names:
                data = f"#!/system/bin/sh\nprintf '{name} test binary\\n'\n".encode()
                info = tarfile.TarInfo(f"lxc/bin/{name}")
                info.size = len(data)
                info.mode = 0o755
                archive.addfile(info, io.BytesIO(data))
            data = b"#!/system/bin/sh\nprintf 'download template\\n'\n"
            info = tarfile.TarInfo("lxc/share/lxc/templates/lxc-download")
            info.size = len(data)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))

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
