from __future__ import annotations

import hashlib
import html
import json
import os
import shutil
import subprocess
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any

PROJECT_ROOT = Path(__file__).resolve().parents[1]
RUNTIME_ROOT = PROJECT_ROOT / "runtime" / "android"
SCRIPT_ROOT = PROJECT_ROOT / "scripts"
LEGACY_MODULE_ID = "achost-runtime"
LEGACY_DATA_ROOT = "/data/adb/achost-runtime"
SPLIT_DATA_ROOT = "/data/adb/achost"
SUPPORTED_MODES = ("manual", "kernelsu-module")
SUPPORTED_MODULE_TARGETS = ("legacy", "base", "docker", "lxc")
SUPPORTED_CGROUP_MODES = ("v1", "v2")
SUPPORTED_DOCKER_RUNTIME_MODES = ("native",)
STALE_RUNTIME_ENTRYPOINTS = (
    "achost-docker-start.sh",
    "achost-docker-stop.sh",
    "detect-uplink.sh",
    "container-nat-manager.sh",
    "container-network-watchdog.sh",
    "protect-container-daemons.sh",
)

COMMON_RUNTIME_FILES = (
    (RUNTIME_ROOT / "bin" / "achost-container-env.sh", "achost/bin/achost-container-env.sh"),
    (RUNTIME_ROOT / "bin" / "achost-container-validate.sh", "achost/bin/achost-container-validate.sh"),
    (SCRIPT_ROOT / "runtime-net-debug.sh", "achost/bin/runtime-net-debug.sh"),
    (SCRIPT_ROOT / "runtime-test.sh", "achost/bin/runtime-test.sh"),
    (SCRIPT_ROOT / "collect-logs.sh", "achost/bin/collect-logs.sh"),
)
DOCKER_RUNTIME_FILES = (
    (SCRIPT_ROOT / "docker" / "runtime-smoke-docker.sh", "achost/bin/runtime-smoke-docker.sh"),
    (SCRIPT_ROOT / "docker" / "runtime-docker-feature-test.sh", "achost/bin/runtime-docker-feature-test.sh"),
)
WEBUI_RUNTIME_FILES = (
    (RUNTIME_ROOT / "bin" / "achost-webui-api.sh", "achost/bin/achost-webui-api.sh"),
)
LXC_RUNTIME_FILES = (
    (RUNTIME_ROOT / "bin" / "achost-lxc-validate.sh", "achost/bin/achost-lxc-validate.sh"),
    (SCRIPT_ROOT / "runtime-smoke-lxc.sh", "achost/bin/runtime-smoke-lxc.sh"),
    (SCRIPT_ROOT / "verify-lxc-checkconfig.sh", "achost/bin/verify-lxc-checkconfig.sh"),
)

LXC_FILES = ("android-common.conf", "default.conf", "unprivileged.conf")
DOCKER_REQUIRED_BINARIES = ("docker", "dockerd", "containerd", "containerd-shim-runc-v2", "ctr", "runc")
DOCKER_OPTIONAL_BINARIES = ("containerd-shim", "docker-init", "docker-proxy", "containerd-stress")
LXC_REQUIRED_BINARIES = (
    "lxc-start",
    "lxc-stop",
    "lxc-attach",
    "lxc-info",
    "lxc-ls",
    "lxc-destroy",
    "lxc-execute",
    "lxc-checkconfig",
)
LXC_OPTIONAL_BINARIES = ("lxc-create", "lxc-copy", "lxc-console")
COMPOSE_ASSET_NAMES = ("docker-compose", "docker-compose-linux-aarch64", "docker-compose-linux-arm64")
COMPOSE_PLUGIN_REL = "achost/etc/docker/cli-plugins/docker-compose"
COMPOSE_STANDALONE_REL = "achost/bin/docker-compose"
BUILDX_PLUGIN_REL = "achost/etc/docker/cli-plugins/docker-buildx"
BUILDX_STANDALONE_REL = "achost/bin/docker-buildx"
BUILDKIT_REQUIRED_BINARIES = ("buildctl", "buildkitd")
RUST_ANDROID_TARGET = "aarch64-linux-android"
SUPERVISOR_CRATE = PROJECT_ROOT / "crates" / "achost-supervise"
SUPERVISOR_TARGET = RUST_ANDROID_TARGET
SUPERVISOR_DEST = "achost/bin/achost-supervise"
WEBUI_API_CRATE = PROJECT_ROOT / "crates" / "achost-webui-api"
WEBUI_API_TARGET = RUST_ANDROID_TARGET
WEBUI_API_DEST = "achost/bin/achost-webui-api"
RUNTIME_CORE_CRATE = PROJECT_ROOT / "crates" / "achost-runtime-core"
RUNTIME_CORE_TARGET = RUST_ANDROID_TARGET
RUNTIME_CORE_DEST = "achost/bin/achost-runtime-core"
DOCKER_RUNTIME_CRATE = PROJECT_ROOT / "crates" / "achost-docker-runtime"
DOCKER_RUNTIME_TARGET = RUST_ANDROID_TARGET
DOCKER_RUNTIME_DEST = "achost/bin/achost-docker-runtime"
LXC_RUNTIME_CRATE = PROJECT_ROOT / "crates" / "achost-lxc-runtime"
LXC_RUNTIME_TARGET = RUST_ANDROID_TARGET
LXC_RUNTIME_DEST = "achost/bin/achost-lxc-runtime"
WEBUI_DIST_ROOT = PROJECT_ROOT / "webui" / "dist"
LXC_ALLOWED_ROOTS = ("bin", "lib", "lib64", "libexec", "share")


@dataclass(frozen=True)
class RuntimeFile:
    path: str
    source: str | None
    executable: bool
    asset: str | None = None
    category: str = "common"


@dataclass(frozen=True)
class RustRuntimeBinary:
    package: str
    crate: Path
    target: str
    dest: str
    category: str


@dataclass(frozen=True)
class ModuleSpec:
    target: str
    module_id: str
    name: str
    description: str
    data_root: str
    requires: tuple[str, ...]
    provides: tuple[str, ...]
    include_common: bool
    include_docker: bool
    include_lxc: bool
    include_webui: bool
    include_supervisor: bool


MODULE_SPECS = {
    "legacy": ModuleSpec(
        "legacy",
        LEGACY_MODULE_ID,
        "ACHost 运行时（旧版）",
        "旧版一体化运行时模块，包含 ACHost 公共组件、Docker 和 LXC。",
        LEGACY_DATA_ROOT,
        (),
        ("common", "docker", "lxc"),
        True,
        True,
        True,
        True,
        True,
    ),
    "base": ModuleSpec(
        "base",
        "achost-base",
        "ACHost 基础模块",
        "提供 ACHost 公共运行时、supervisor、sysctl、网络守护和诊断能力，供 Docker/LXC 模块依赖。",
        SPLIT_DATA_ROOT,
        (),
        ("common",),
        True,
        False,
        False,
        False,
        True,
    ),
    "docker": ModuleSpec(
        "docker",
        "achost-docker",
        "ACHost Docker 模块",
        "提供 ACHost 的 Docker 引擎、命令行包装器、Compose/buildx/BuildKit 支持和 Docker 管理 WebUI。",
        SPLIT_DATA_ROOT,
        ("achost-base",),
        ("docker",),
        False,
        True,
        False,
        True,
        False,
    ),
    "lxc": ModuleSpec(
        "lxc",
        "achost-lxc",
        "ACHost LXC 模块",
        "提供 ACHost 的通用 LXC 运行时、配置、用户态文件和容器管理 WebUI，依赖基础模块运行。",
        SPLIT_DATA_ROOT,
        ("achost-base",),
        ("lxc",),
        False,
        False,
        True,
        True,
        False,
    ),
}


def generate_runtime_package(
    output: str | Path,
    mode: str = "manual",
    cgroup_mode: str = "v1",
    docker_asset: str | Path | None = None,
    docker_sha256: str | None = None,
    compose_asset: str | Path | None = None,
    compose_sha256: str | None = None,
    buildx_asset: str | Path | None = None,
    buildx_sha256: str | None = None,
    buildkit_asset: str | Path | None = None,
    buildkit_sha256: str | None = None,
    lxc_asset: str | Path | None = None,
    lxc_sha256: str | None = None,
    start_docker_on_boot: bool = False,
    docker_runtime_mode: str = "native",
    module_target: str = "legacy",
) -> dict[str, Any]:
    if mode not in SUPPORTED_MODES:
        raise ValueError(f"unsupported runtime package mode: {mode}")
    if module_target not in SUPPORTED_MODULE_TARGETS:
        raise ValueError(f"unsupported module target: {module_target}")
    if mode != "kernelsu-module" and module_target != "legacy":
        raise ValueError("module targets are only supported for kernelsu-module mode")
    if cgroup_mode not in SUPPORTED_CGROUP_MODES:
        raise ValueError(f"unsupported cgroup mode: {cgroup_mode}")
    if docker_runtime_mode not in SUPPORTED_DOCKER_RUNTIME_MODES:
        raise ValueError(f"unsupported docker runtime mode: {docker_runtime_mode}")
    if docker_sha256 and docker_asset is None:
        raise ValueError("docker checksum requires a docker asset")
    if compose_sha256 and compose_asset is None:
        raise ValueError("compose checksum requires a compose asset")
    if buildx_sha256 and buildx_asset is None:
        raise ValueError("buildx checksum requires a buildx asset")
    if buildkit_sha256 and buildkit_asset is None:
        raise ValueError("buildkit checksum requires a buildkit asset")
    if lxc_sha256 and lxc_asset is None:
        raise ValueError("lxc checksum requires an lxc asset")
    if start_docker_on_boot and mode != "kernelsu-module":
        raise ValueError("start_docker_on_boot is only supported for kernelsu-module mode")

    spec = MODULE_SPECS[module_target]
    if start_docker_on_boot and not spec.include_docker:
        raise ValueError("start_docker_on_boot requires a Docker module target")
    validate_assets_for_module(
        spec,
        docker_asset,
        compose_asset,
        buildx_asset,
        buildkit_asset,
        lxc_asset,
    )

    root = Path(output).expanduser().resolve()
    ensure_empty_output(root)
    root.mkdir(parents=True, exist_ok=True)

    install_prefix = install_prefix_for_mode(mode, spec)
    replacements = {"@ACHOST_PREFIX@": install_prefix}
    files: list[RuntimeFile] = []

    if mode == "manual" or spec.include_common:
        for src, dst in COMMON_RUNTIME_FILES:
            files.append(copy_text_file(src, root / dst, root, executable=True, category="common"))
        files.append(
            copy_text_file(
                RUNTIME_ROOT / "sysctl" / "99-container-host.conf",
                root / "achost" / "etc" / "sysctl.d" / "99-container-host.conf",
                root,
                category="common",
            )
        )

    if mode == "manual" or spec.include_docker:
        for src, dst in DOCKER_RUNTIME_FILES:
            files.append(copy_text_file(src, root / dst, root, executable=True, category="docker"))
        files.append(
            copy_text_file(
                RUNTIME_ROOT / "docker" / "etc" / f"daemon.cgroup-{cgroup_mode}.json",
                root / "achost" / "etc" / "docker" / "daemon.json",
                root,
                category="docker",
            )
        )
        files.extend(write_docker_wrappers(root, mode, install_prefix, spec))

    if mode == "manual" or spec.include_webui:
        for src, dst in WEBUI_RUNTIME_FILES:
            files.append(copy_text_file(src, root / dst, root, executable=True, category="webui"))

    if mode == "manual" or spec.include_lxc:
        for src, dst in LXC_RUNTIME_FILES:
            files.append(copy_text_file(src, root / dst, root, executable=True, category="lxc"))
        for name in LXC_FILES:
            files.append(
                copy_text_file(
                    RUNTIME_ROOT / "lxc" / name,
                    root / "achost" / "etc" / "lxc" / name,
                    root,
                    replacements=replacements,
                    category="lxc",
                )
            )

    files.append(write_runtime_config(root, mode, spec, docker_runtime_mode, cgroup_mode))
    ensure_runtime_dirs(root, spec if mode == "kernelsu-module" else MODULE_SPECS["legacy"])
    runtime_core_report: dict[str, Any] | None = None
    if mode == "manual" or spec.include_common:
        runtime_core_report, runtime_core_files = install_runtime_core_helper(root)
        files.extend(runtime_core_files)
    supervisor_report: dict[str, Any] | None = None
    if mode == "manual" or spec.include_supervisor:
        supervisor_report, supervisor_files = install_supervisor_helper(root)
        files.extend(supervisor_files)
    docker_runtime_report: dict[str, Any] | None = None
    if mode == "manual" or spec.include_docker:
        docker_runtime_report, docker_runtime_files = install_docker_runtime_helper(root)
        files.extend(docker_runtime_files)
    lxc_runtime_report: dict[str, Any] | None = None
    if mode == "manual" or spec.include_lxc:
        lxc_runtime_report, lxc_runtime_files = install_lxc_runtime_helper(root)
        files.extend(lxc_runtime_files)
    webui_api_report: dict[str, Any] | None = None
    if mode == "manual" or spec.include_webui:
        webui_api_report, webui_api_files = install_webui_api_helper(root)
        files.extend(webui_api_files)
    assets: dict[str, Any] = {
        "docker": None,
        "compose": None,
        "buildx": None,
        "buildkit": None,
        "lxc": None,
        "runtime_core": runtime_core_report,
        "docker_runtime": docker_runtime_report,
        "lxc_runtime": lxc_runtime_report,
        "supervisor": supervisor_report,
        "webui_api": webui_api_report,
        "start_docker_on_boot": start_docker_on_boot,
    }
    if docker_asset is not None:
        docker_report, docker_files, embedded_compose_report, embedded_compose_files = install_docker_asset(
            docker_asset,
            root,
            docker_sha256,
            include_embedded_compose=compose_asset is None,
        )
        assets["docker"] = docker_report
        files.extend(docker_files)
        if embedded_compose_report is not None:
            assets["compose"] = embedded_compose_report
            files.extend(embedded_compose_files)
    if compose_asset is not None:
        compose_report, compose_files = install_compose_asset(compose_asset, root, compose_sha256)
        assets["compose"] = compose_report
        files.extend(compose_files)
    if buildx_asset is not None:
        buildx_report, buildx_files = install_buildx_asset(buildx_asset, root, buildx_sha256)
        assets["buildx"] = buildx_report
        files.extend(buildx_files)
    if buildkit_asset is not None:
        buildkit_report, buildkit_files = install_buildkit_asset(buildkit_asset, root, buildkit_sha256)
        assets["buildkit"] = buildkit_report
        files.extend(buildkit_files)
    if lxc_asset is not None:
        lxc_report, lxc_files = install_lxc_asset(lxc_asset, root, lxc_sha256)
        assets["lxc"] = lxc_report
        files.extend(lxc_files)
    if mode == "kernelsu-module" and spec.include_webui:
        files.extend(install_webui(root, spec))
    entrypoints = write_mode_files(root, mode, spec, files, start_docker_on_boot=start_docker_on_boot)
    files.append(RuntimeFile("manifest.json", None, False, category="module-config"))
    included_categories = sorted({item.category for item in files})
    report = {
        "mode": mode,
        "module_target": module_target,
        "module_id": spec.module_id if mode == "kernelsu-module" else None,
        "module_name": spec.name if mode == "kernelsu-module" else None,
        "data_root": spec.data_root if mode == "kernelsu-module" else None,
        "requires": list(spec.requires) if mode == "kernelsu-module" else [],
        "provides": list(spec.provides) if mode == "kernelsu-module" else [],
        "included_categories": included_categories,
        "excluded_categories": [
            category
            for category in ("common", "docker", "lxc", "webui", "supervisor")
            if category not in included_categories
        ],
        "cgroup_mode": cgroup_mode,
        "docker_runtime_mode": docker_runtime_mode,
        "output": str(root),
        "install_prefix": install_prefix,
        "entrypoints": entrypoints,
        "assets": assets,
        "files": [file_entry(item) for item in files],
    }
    (root / "manifest.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def validate_assets_for_module(
    spec: ModuleSpec,
    docker_asset: str | Path | None,
    compose_asset: str | Path | None,
    buildx_asset: str | Path | None,
    buildkit_asset: str | Path | None,
    lxc_asset: str | Path | None,
) -> None:
    docker_assets = [asset for asset in (docker_asset, compose_asset, buildx_asset, buildkit_asset) if asset is not None]
    if spec.target == "base" and (docker_assets or lxc_asset is not None):
        raise ValueError("base module target does not accept Docker or LXC assets")
    if spec.target == "docker" and lxc_asset is not None:
        raise ValueError("docker module target does not accept LXC assets")
    if spec.target == "lxc" and docker_assets:
        raise ValueError("lxc module target does not accept Docker assets")


def install_prefix_for_mode(mode: str, spec: ModuleSpec) -> str:
    if mode == "kernelsu-module":
        return f"/data/adb/modules/{spec.module_id}/achost"
    return "/data/adb/achost"


def ensure_empty_output(root: Path) -> None:
    if root.exists() and not root.is_dir():
        raise FileExistsError(f"output exists and is not a directory: {root}")
    if root.exists() and any(root.iterdir()):
        raise FileExistsError(f"output directory is not empty: {root}")


def ensure_runtime_dirs(root: Path, spec: ModuleSpec) -> None:
    rel_paths = [
        "achost/etc",
        "achost/var/run",
        "achost/var/log",
    ]
    if spec.include_docker:
        rel_paths.extend(
            (
                "achost/etc/docker/cli-plugins",
                "achost/var/docker",
                "achost/var/containerd/root",
                "achost/var/containerd/state",
                "achost/wrappers",
            )
        )
    if spec.include_lxc:
        rel_paths.extend(("achost/etc/lxc", "achost/lxc"))
    for rel_path in rel_paths:
        (root / rel_path).mkdir(parents=True, exist_ok=True)


def write_runtime_config(root: Path, mode: str, spec: ModuleSpec, docker_runtime_mode: str, cgroup_mode: str) -> RuntimeFile:
    lines = [
        f"ACHOST_MODULE_TARGET={spec.target}",
        f"ACHOST_RUNTIME_MODE={docker_runtime_mode}",
        "ACHOST_USE_CHROOT=0",
        f"ACHOST_CGROUP_MODE={cgroup_mode}",
    ]
    if mode == "kernelsu-module":
        lines.extend(
            (
                f"ACHOST_VAR={spec.data_root}",
                f"ACHOST_CONFIG={spec.data_root}/config",
                f"ACHOST_CHROOT={spec.data_root}/chroot",
                "ACHOST_BASE=/data/adb/modules/achost-base/achost",
                "ACHOST_DOCKER_MODULE=/data/adb/modules/achost-docker/achost",
                "ACHOST_LXC_MODULE=/data/adb/modules/achost-lxc/achost",
                f"ACHOST_LXC_VAR={spec.data_root}/lxc",
                f"ACHOST_LXC_RUN={spec.data_root}/run/lxc",
                f"ACHOST_LXC_LOG={spec.data_root}/log/lxc",
                f"ACHOST_LXC_ROOTFS={spec.data_root}/lxc/rootfs",
                f"ACHOST_LXC_CONTAINERS={spec.data_root}/lxc/containers",
                "LXC_BRIDGE=lxcbr0",
                "LXC_SUBNET=172.32.0.0/16",
            )
        )
    dst = root / "achost" / "etc" / "achost-runtime.conf"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text("\n".join(lines) + "\n")
    return RuntimeFile(str(dst.relative_to(root)), None, False, category="module-config")


def write_executable_text(root: Path, rel_path: str, text: str, category: str = "common") -> RuntimeFile:
    dst = root / rel_path
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text)
    os.chmod(dst, 0o755)
    return RuntimeFile(str(dst.relative_to(root)), None, True, category=category)


def write_docker_wrappers(root: Path, mode: str, install_prefix: str, spec: ModuleSpec) -> list[RuntimeFile]:
    files = [
        write_executable_text(
            root,
            "achost/wrappers/docker",
            manual_docker_wrapper(),
            category="docker",
        )
    ]
    return files


def docker_wrapper_rewrite_functions() -> str:
    return r'''
docker_socket_path() {
    case "${DOCKER_HOST:-}" in
        unix://*) printf '%s' "${DOCKER_HOST#unix://}" ;;
        *) printf '%s' "$ACHOST/var/run/docker.sock" ;;
    esac
}

rewrite_docker_mount() {
    docker_socket="$(docker_socket_path)"
    case "$1" in
        /var/run/docker.sock|/run/docker.sock)
            printf '%s' "$docker_socket"
            ;;
        /var/run/docker.sock:*|/run/docker.sock:*)
            printf '%s:%s' "$docker_socket" "${1#*:}"
            ;;
        type=bind,source=/var/run/docker.sock,*)
            printf 'type=bind,source=%s,%s' "$docker_socket" "${1#type=bind,source=/var/run/docker.sock,}"
            ;;
        type=bind,source=/run/docker.sock,*)
            printf 'type=bind,source=%s,%s' "$docker_socket" "${1#type=bind,source=/run/docker.sock,}"
            ;;
        type=bind,src=/var/run/docker.sock,*)
            printf 'type=bind,src=%s,%s' "$docker_socket" "${1#type=bind,src=/var/run/docker.sock,}"
            ;;
        type=bind,src=/run/docker.sock,*)
            printf 'type=bind,src=%s,%s' "$docker_socket" "${1#type=bind,src=/run/docker.sock,}"
            ;;
        *)
            printf '%s' "$1"
            ;;
    esac
}

quote_docker_arg() {
    printf "'"
    printf '%s' "$1" | sed "s/'/'\\\\''/g"
    printf "'"
}

append_docker_arg() {
    docker_exec_args="$docker_exec_args $(quote_docker_arg "$1")"
}

exec_docker() {
    case "${1:-}" in
        run|create) ;;
        *) exec "$ACHOST/bin/docker" "$@" ;;
    esac
    docker_exec_args=""
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -v|--volume|--mount)
                docker_opt="$1"
                shift
                append_docker_arg "$docker_opt"
                if [ "$#" -gt 0 ]; then
                    append_docker_arg "$(rewrite_docker_mount "$1")"
                else
                    break
                fi
                ;;
            --volume=*)
                docker_mount_value="${1#--volume=}"
                append_docker_arg "--volume=$(rewrite_docker_mount "$docker_mount_value")"
                ;;
            --mount=*)
                docker_mount_value="${1#--mount=}"
                append_docker_arg "--mount=$(rewrite_docker_mount "$docker_mount_value")"
                ;;
            -v/*)
                docker_mount_value="${1#-v}"
                append_docker_arg "-v$(rewrite_docker_mount "$docker_mount_value")"
                ;;
            *)
                append_docker_arg "$1"
                ;;
        esac
        shift || break
    done
    eval "exec \"\$ACHOST/bin/docker\" $docker_exec_args"
}
'''


def stale_runtime_entrypoints_shell_words() -> str:
    return " ".join(STALE_RUNTIME_ENTRYPOINTS)


def prune_stale_runtime_entrypoints_function() -> str:
    return f'''prune_stale_runtime_entrypoints() {{
    bin_dir="$1"
    for name in {stale_runtime_entrypoints_shell_words()}; do
        rm -f "$bin_dir/$name" 2>/dev/null || true
    done
}}
'''


def manual_docker_wrapper() -> str:
    return f'''#!/system/bin/sh
set -u
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${{ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}}"
. "$ACHOST/bin/achost-container-env.sh"
{docker_wrapper_rewrite_functions()}exec_docker "$@"
'''


def module_docker_wrapper(install_prefix: str, spec: ModuleSpec) -> str:
    base_source = ""
    if spec.target == "docker":
        base_source = '''
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
'''
    return f'''#!/system/bin/sh
# ACHOST_DOCKER_WRAPPER
set -u
ACHOST="${{ACHOST:-{install_prefix}}}"
if [ -r "$ACHOST/bin/achost-container-env.sh" ]; then
    . "$ACHOST/bin/achost-container-env.sh"
{base_source}else
    printf 'ACHost env not found\n' >&2
    exit 1
fi
{docker_wrapper_rewrite_functions()}exec_docker "$@"
'''


def ksu_docker_wrapper_install_script(install_prefix: str, spec: ModuleSpec) -> str:
    wrapper = module_docker_wrapper(install_prefix, spec).rstrip()
    return f'''
install_docker_cli_wrapper() {{
    ksu_bin="/data/adb/ksu/bin"
    mkdir -p "$ksu_bin" 2>/dev/null || return 0
    target="$ksu_bin/docker"
    if [ -e "$target" ] && ! grep -q 'ACHOST_DOCKER_WRAPPER' "$target" 2>/dev/null; then
        printf 'ACHost: keeping existing non-ACHost command: %s\n' "$target" >&2
        return 0
    fi
    cat > "$target" <<'ACHOST_DOCKER_WRAPPER'
{wrapper}
ACHOST_DOCKER_WRAPPER
    chmod 0755 "$target" 2>/dev/null || true
}}
install_docker_cli_wrapper
'''


def ksu_lxc_wrapper_install_script(install_prefix: str) -> str:
    return f'''
install_lxc_cli_wrappers() {{
    ksu_bin="/data/adb/ksu/bin"
    mkdir -p "$ksu_bin" 2>/dev/null || return 0
    [ -d "$ACHOST/lxc/bin" ] || return 0
    for src in "$ACHOST"/lxc/bin/lxc* "$ACHOST"/lxc/bin/lxd*; do
        [ -f "$src" ] || continue
        [ -x "$src" ] || continue
        name="${{src##*/}}"
        case "$name" in
            lxc*|lxd*) ;;
            *) continue ;;
        esac
        target="$ksu_bin/$name"
        if [ -e "$target" ] && ! grep -q 'ACHOST_LXC_WRAPPER' "$target" 2>/dev/null; then
            printf 'ACHost: keeping existing non-ACHost command: %s\n' "$target" >&2
            continue
        fi
        cat > "$target" <<'ACHOST_LXC_WRAPPER'
#!/system/bin/sh
# ACHOST_LXC_WRAPPER
set -u
name="${{0##*/}}"
ACHOST="${{ACHOST:-{install_prefix}}}"
if [ -r "$ACHOST/bin/achost-container-env.sh" ]; then
    . "$ACHOST/bin/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${{ACHOST_BASE:-/data/adb/modules/achost-base/achost}}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
else
    printf 'ACHost env not found\n' >&2
    exit 1
fi
exec "$ACHOST/lxc/bin/$name" "$@"
ACHOST_LXC_WRAPPER
        chmod 0755 "$target" 2>/dev/null || true
    done
}}
install_lxc_cli_wrappers
'''


def copy_text_file(
    src: Path,
    dst: Path,
    root: Path,
    executable: bool = False,
    replacements: dict[str, str] | None = None,
    category: str = "common",
) -> RuntimeFile:
    if not src.exists():
        raise FileNotFoundError(f"runtime source not found: {src}")

    text = src.read_text()
    for old, new in (replacements or {}).items():
        text = text.replace(old, new)

    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text)
    if executable:
        os.chmod(dst, 0o755)
    return RuntimeFile(str(dst.relative_to(root)), str(src.relative_to(PROJECT_ROOT)), executable, category=category)


def install_runtime_core_helper(root: Path) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_rust_runtime_binary(
        root,
        RustRuntimeBinary("achost-runtime-core", RUNTIME_CORE_CRATE, RUNTIME_CORE_TARGET, RUNTIME_CORE_DEST, "common"),
    )


def install_docker_runtime_helper(root: Path) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_rust_runtime_binary(
        root,
        RustRuntimeBinary(
            "achost-docker-runtime",
            DOCKER_RUNTIME_CRATE,
            DOCKER_RUNTIME_TARGET,
            DOCKER_RUNTIME_DEST,
            "docker",
        ),
    )


def install_lxc_runtime_helper(root: Path) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_rust_runtime_binary(
        root,
        RustRuntimeBinary(
            "achost-lxc-runtime",
            LXC_RUNTIME_CRATE,
            LXC_RUNTIME_TARGET,
            LXC_RUNTIME_DEST,
            "lxc",
        ),
    )


def install_supervisor_helper(root: Path) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_rust_runtime_binary(
        root,
        RustRuntimeBinary("achost-supervise", SUPERVISOR_CRATE, SUPERVISOR_TARGET, SUPERVISOR_DEST, "supervisor"),
    )


def install_webui_api_helper(root: Path) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_rust_runtime_binary(
        root,
        RustRuntimeBinary("achost-webui-api", WEBUI_API_CRATE, WEBUI_API_TARGET, WEBUI_API_DEST, "webui"),
    )


def install_rust_runtime_binary(root: Path, binary: RustRuntimeBinary) -> tuple[dict[str, Any], list[RuntimeFile]]:
    if not binary.crate.exists():
        raise FileNotFoundError(f"Rust crate not found for {binary.package}: {binary.crate}")

    cargo = shutil.which("cargo")
    if cargo is None:
        raise FileNotFoundError(f"cargo not found for Rust {binary.package} build")
    linker = find_android_linker()
    if linker is None:
        raise FileNotFoundError(f"no Android NDK clang linker found for Rust {binary.package}")

    command = [
        cargo,
        "build",
        "--release",
        "--target",
        binary.target,
        "--manifest-path",
        str(PROJECT_ROOT / "Cargo.toml"),
        "-p",
        binary.package,
    ]
    env = os.environ.copy()
    env["CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"] = linker
    try:
        subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    except subprocess.CalledProcessError as exc:
        message = exc.stderr.strip() or exc.stdout.strip() or str(exc)
        raise RuntimeError(f"failed to build Rust {binary.package}: {message}") from exc

    built = PROJECT_ROOT / "target" / binary.target / "release" / binary.package
    if not built.exists():
        raise FileNotFoundError(f"Rust {binary.package} build output missing: {built}")
    dst = root / binary.dest
    copy_file(built, dst, 0o755)
    return {
        "source": str(binary.crate.relative_to(PROJECT_ROOT)),
        "implementation": "rust",
        "builder": cargo,
        "target": binary.target,
        "linker": linker,
        "path": binary.dest,
    }, [RuntimeFile(str(dst.relative_to(root)), str(binary.crate.relative_to(PROJECT_ROOT)), True, category=binary.category)]


def webui_dist_for_target(target: str) -> Path:
    if target == "lxc":
        return WEBUI_DIST_ROOT / "lxc"
    return WEBUI_DIST_ROOT / "docker"


def install_webui(root: Path, spec: ModuleSpec) -> list[RuntimeFile]:
    webui_dist = webui_dist_for_target(spec.target)
    index = webui_dist / "index.html"
    if not index.exists():
        raise FileNotFoundError(f"WebUI dist not found: {webui_dist}; run npm install && npm run build in webui")

    files: list[RuntimeFile] = []
    for src in sorted(webui_dist.rglob("*")):
        if src.is_dir():
            continue
        rel = src.relative_to(webui_dist)
        dst = root / "webroot" / rel
        copy_file(src, dst, 0o644)
        files.append(RuntimeFile(str(dst.relative_to(root)), str(src.relative_to(PROJECT_ROOT)), False, category="webui"))

    config = {
        "moduleTarget": spec.target,
        "moduleId": spec.module_id,
        "api": f"/data/adb/modules/{spec.module_id}/achost/bin/achost-webui-api.sh",
    }
    inline_config = json.dumps(config, separators=(",", ":"), sort_keys=True)
    index_dst = root / "webroot" / "index.html"
    index_text = index_dst.read_text()
    config_meta = f'<meta name="achost-webui-config" content="{html.escape(inline_config, quote=True)}" />'
    if "<head>" in index_text:
        index_text = index_text.replace("<head>", f"<head>\n    {config_meta}", 1)
    elif "</head>" in index_text:
        index_text = index_text.replace("</head>", f"  {config_meta}\n  </head>", 1)
    else:
        index_text = config_meta + index_text
    index_dst.write_text(index_text)

    dst = root / "webroot" / "achost-webui-config.json"
    dst.write_text(json.dumps(config, indent=2, sort_keys=True) + "\n")
    files.append(RuntimeFile(str(dst.relative_to(root)), None, False, category="webui"))
    return files


def find_android_linker() -> str | None:
    explicit = os.environ.get("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER") or os.environ.get("ACHOST_ANDROID_LINKER")
    if explicit:
        explicit_path = Path(explicit).expanduser()
        if explicit_path.exists():
            return str(explicit_path.resolve())
        found = shutil.which(explicit)
        if found:
            return found
        raise FileNotFoundError(f"Android linker not found: {explicit}")

    names = (
        "aarch64-linux-android35-clang",
        "aarch64-linux-android34-clang",
        "aarch64-linux-android33-clang",
        "aarch64-linux-android31-clang",
        "aarch64-linux-android29-clang",
        "aarch64-linux-android28-clang",
        "aarch64-linux-android26-clang",
        "aarch64-linux-android24-clang",
        "aarch64-linux-android23-clang",
        "aarch64-linux-android21-clang",
    )
    for name in names:
        found = shutil.which(name)
        if found:
            return found

    roots: list[Path] = []
    for env_name in ("ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"):
        value = os.environ.get(env_name)
        if value:
            roots.append(Path(value).expanduser())
    android_dir = Path.home() / "Android"
    if android_dir.exists():
        roots.extend(sorted(android_dir.glob("android-ndk-*"), reverse=True))

    for root in roots:
        bin_dir = root / "toolchains" / "llvm" / "prebuilt" / "linux-x86_64" / "bin"
        for name in names:
            candidate = bin_dir / name
            if candidate.exists():
                return str(candidate.resolve())
    return None


def install_docker_asset(
    asset: str | Path,
    root: Path,
    expected_sha256: str | None,
    include_embedded_compose: bool = True,
) -> tuple[dict[str, Any], list[RuntimeFile], dict[str, Any] | None, list[RuntimeFile]]:
    asset_path = resolve_asset_file(asset)
    digest = sha256_file(asset_path)
    verify_sha256(asset_path, digest, expected_sha256)

    accepted = set(DOCKER_REQUIRED_BINARIES) | set(DOCKER_OPTIONAL_BINARIES)
    extracted: dict[str, str] = {}
    files: list[RuntimeFile] = []
    compose_member: tarfile.TarInfo | None = None
    with tarfile.open(asset_path, "r:*") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            name = PurePosixPath(member.name).name
            if include_embedded_compose and is_compose_member(member.name):
                if compose_member is None or "cli-plugins" in member.name:
                    compose_member = member
            if name not in accepted or name in extracted:
                continue
            dst = root / "achost" / "bin" / name
            copy_tar_member(archive, member, dst, 0o755)
            extracted[name] = member.name
            files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), True, "docker", "docker-asset"))

        if compose_member is not None:
            compose_report, compose_files = install_compose_tar_member(
                archive,
                compose_member,
                asset_path,
                root,
                digest,
                embedded_in="docker",
            )
        else:
            compose_report = None
            compose_files = []

    missing = [name for name in DOCKER_REQUIRED_BINARIES if name not in extracted]
    if missing:
        raise ValueError(f"docker asset missing required binaries: {', '.join(missing)}")

    return {
        "source": str(asset_path),
        "sha256": digest,
        "required_binaries": list(DOCKER_REQUIRED_BINARIES),
        "optional_binaries": [name for name in DOCKER_OPTIONAL_BINARIES if name in extracted],
        "files": {name: extracted[name] for name in sorted(extracted)},
    }, files, compose_report, compose_files


def is_compose_member(name: str) -> bool:
    parts = normalized_tar_parts(name)
    if not parts:
        return False
    return parts[-1] in COMPOSE_ASSET_NAMES


def install_compose_tar_member(
    archive: tarfile.TarFile,
    member: tarfile.TarInfo,
    asset_path: Path,
    root: Path,
    digest: str,
    embedded_in: str,
) -> tuple[dict[str, Any], list[RuntimeFile]]:
    files: list[RuntimeFile] = []
    for rel_path in (COMPOSE_PLUGIN_REL, COMPOSE_STANDALONE_REL):
        dst = root / rel_path
        copy_tar_member(archive, member, dst, 0o755)
        files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), True, "compose", "docker-asset"))
    return {
        "source": str(asset_path),
        "sha256": digest,
        "embedded_in": embedded_in,
        "member": member.name,
        "plugin_path": COMPOSE_PLUGIN_REL,
        "standalone_path": COMPOSE_STANDALONE_REL,
    }, files


def is_buildx_member(name: str) -> bool:
    parts = normalized_tar_parts(name)
    if not parts:
        return False
    basename = parts[-1]
    return basename in ("buildx", "docker-buildx") or basename.startswith("buildx-") or basename.startswith("docker-buildx-")


def install_compose_asset(asset: str | Path, root: Path, expected_sha256: str | None) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_cli_plugin_asset(
        asset,
        root,
        expected_sha256,
        asset_label="compose",
        plugin_rel=COMPOSE_PLUGIN_REL,
        standalone_rel=COMPOSE_STANDALONE_REL,
        tar_member_match=is_compose_member,
    )


def install_buildx_asset(asset: str | Path, root: Path, expected_sha256: str | None) -> tuple[dict[str, Any], list[RuntimeFile]]:
    return install_cli_plugin_asset(
        asset,
        root,
        expected_sha256,
        asset_label="buildx",
        plugin_rel=BUILDX_PLUGIN_REL,
        standalone_rel=BUILDX_STANDALONE_REL,
        tar_member_match=is_buildx_member,
    )


def install_cli_plugin_asset(
    asset: str | Path,
    root: Path,
    expected_sha256: str | None,
    asset_label: str,
    plugin_rel: str,
    standalone_rel: str,
    tar_member_match,
) -> tuple[dict[str, Any], list[RuntimeFile]]:
    asset_path = resolve_asset_file(asset)
    digest = sha256_file(asset_path)
    verify_sha256(asset_path, digest, expected_sha256)

    files: list[RuntimeFile] = []
    member_name: str | None = None
    if tarfile.is_tarfile(asset_path):
        with tarfile.open(asset_path, "r:*") as archive:
            chosen: tarfile.TarInfo | None = None
            for member in archive.getmembers():
                if not member.isfile() or not tar_member_match(member.name):
                    continue
                if chosen is None or "cli-plugins" in member.name or "/bin/" in member.name:
                    chosen = member
            if chosen is None:
                raise ValueError(f"{asset_label} asset contained no supported binary")
            for rel_path in (plugin_rel, standalone_rel):
                dst = root / rel_path
                copy_tar_member(archive, chosen, dst, 0o755)
                files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), True, asset_label, "docker-asset"))
            member_name = chosen.name
    else:
        for rel_path in (plugin_rel, standalone_rel):
            dst = root / rel_path
            copy_file(asset_path, dst, 0o755)
            files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), True, asset_label, "docker-asset"))

    return {
        "source": str(asset_path),
        "sha256": digest,
        "member": member_name,
        "plugin_path": plugin_rel,
        "standalone_path": standalone_rel,
    }, files


def install_buildkit_asset(asset: str | Path, root: Path, expected_sha256: str | None) -> tuple[dict[str, Any], list[RuntimeFile]]:
    asset_path = resolve_asset_file(asset)
    digest = sha256_file(asset_path)
    verify_sha256(asset_path, digest, expected_sha256)
    if not tarfile.is_tarfile(asset_path):
        raise ValueError("buildkit asset must be a tar archive containing buildctl and buildkitd")

    files: list[RuntimeFile] = []
    extracted: dict[str, str] = {}
    with tarfile.open(asset_path, "r:*") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            name = PurePosixPath(member.name).name
            if name not in BUILDKIT_REQUIRED_BINARIES or name in extracted:
                continue
            dst = root / "achost" / "bin" / name
            copy_tar_member(archive, member, dst, 0o755)
            extracted[name] = member.name
            files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), True, "buildkit", "docker-asset"))

    missing = [name for name in BUILDKIT_REQUIRED_BINARIES if name not in extracted]
    if missing:
        raise ValueError(f"buildkit asset missing required binaries: {', '.join(missing)}")

    return {
        "source": str(asset_path),
        "sha256": digest,
        "required_binaries": list(BUILDKIT_REQUIRED_BINARIES),
        "files": {name: extracted[name] for name in sorted(extracted)},
    }, files


def install_lxc_asset(asset: str | Path, root: Path, expected_sha256: str | None) -> tuple[dict[str, Any], list[RuntimeFile]]:
    asset_path = resolve_asset_file(asset)
    digest = sha256_file(asset_path)
    verify_sha256(asset_path, digest, expected_sha256)

    files: list[RuntimeFile] = []
    extracted_paths: list[str] = []
    extracted_binaries: dict[str, str] = {}
    with tarfile.open(asset_path, "r:*") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            rel_path = lxc_destination(member.name)
            if rel_path is None:
                continue
            dst = root / rel_path
            executable = lxc_file_is_executable(rel_path)
            mode = 0o755 if executable else (member.mode & 0o777 or 0o644)
            copy_tar_member(archive, member, dst, mode)
            extracted_paths.append(rel_path)
            if rel_path.startswith("achost/lxc/bin/"):
                name = PurePosixPath(rel_path).name
                if name not in extracted_binaries:
                    extracted_binaries[name] = member.name
            files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), executable, "lxc", "lxc-asset"))

    if not extracted_paths:
        raise ValueError("lxc asset contained no supported files")
    missing = [name for name in LXC_REQUIRED_BINARIES if name not in extracted_binaries]
    if missing:
        raise ValueError(f"lxc asset missing required binaries: {', '.join(missing)}")

    return {
        "source": str(asset_path),
        "sha256": digest,
        "required_binaries": list(LXC_REQUIRED_BINARIES),
        "optional_binaries": [name for name in LXC_OPTIONAL_BINARIES if name in extracted_binaries],
        "files": {name: extracted_binaries[name] for name in sorted(extracted_binaries)},
        "paths": sorted(extracted_paths),
    }, files


def resolve_asset_file(asset: str | Path) -> Path:
    asset_path = Path(asset).expanduser().resolve()
    if not asset_path.exists():
        raise FileNotFoundError(f"asset not found: {asset_path}")
    if not asset_path.is_file():
        raise ValueError(f"asset is not a file: {asset_path}")
    return asset_path


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sha256(path: Path, actual: str, expected: str | None) -> None:
    if expected is None:
        return
    if actual.lower() != expected.lower():
        raise ValueError(f"sha256 mismatch for {path}: expected {expected.lower()} got {actual}")


def copy_tar_member(archive: tarfile.TarFile, member: tarfile.TarInfo, dst: Path, mode: int) -> None:
    source = archive.extractfile(member)
    if source is None:
        raise ValueError(f"could not read tar member: {member.name}")
    dst.parent.mkdir(parents=True, exist_ok=True)
    with source, dst.open("wb") as output:
        shutil.copyfileobj(source, output)
    os.chmod(dst, mode)


def copy_file(src: Path, dst: Path, mode: int) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dst)
    os.chmod(dst, mode)


def create_runtime_zip(root: str | Path, zip_output: str | Path | None = None) -> Path:
    root_path = Path(root).expanduser().resolve()
    if not root_path.is_dir():
        raise FileNotFoundError(f"runtime package directory not found: {root_path}")
    zip_path = Path(zip_output).expanduser().resolve() if zip_output is not None else root_path.with_name(f"{root_path.name}.zip")
    if zip_path.exists():
        raise FileExistsError(f"zip output exists: {zip_path}")
    zip_path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(root_path.rglob("*")):
            if path.is_dir():
                continue
            archive.write(path, path.relative_to(root_path).as_posix())
    return zip_path


def lxc_destination(name: str) -> str | None:
    parts = normalized_tar_parts(name)
    if not parts:
        return None
    parts = strip_lxc_asset_root(parts)
    if not parts:
        return None
    if parts[0] in LXC_ALLOWED_ROOTS:
        return str(PurePosixPath("achost/lxc", *parts))
    if parts[0] == "etc" and len(parts) > 1 and parts[1] == "lxc":
        return str(PurePosixPath("achost/etc/lxc", *parts[2:]))
    return None


def lxc_file_is_executable(rel_path: str) -> bool:
    path = PurePosixPath(rel_path)
    return "/bin/" in f"/{rel_path}" or (
        path.parts[:5] == ("achost", "lxc", "share", "lxc", "templates")
        and path.name.startswith("lxc-")
    )


def normalized_tar_parts(name: str) -> list[str]:
    path = PurePosixPath(name)
    if path.is_absolute():
        return []
    parts = [part for part in path.parts if part not in ("", ".")]
    if any(part == ".." for part in parts):
        return []
    return parts


def strip_lxc_asset_root(parts: list[str]) -> list[str]:
    if parts[0] == "lxc":
        return parts[1:]
    if parts[0] in LXC_ALLOWED_ROOTS or parts[0] == "etc":
        return parts
    if len(parts) > 1 and (parts[1] in LXC_ALLOWED_ROOTS or parts[1] == "etc"):
        return parts[1:]
    return parts


def write_mode_files(
    root: Path,
    mode: str,
    spec: ModuleSpec,
    files: list[RuntimeFile],
    start_docker_on_boot: bool = False,
) -> list[str]:
    if mode == "kernelsu-module":
        generated = {
            "module.prop": module_prop(spec),
            "post-fs-data.sh": post_fs_data_script(),
            "service.sh": service_script(spec, start_docker_on_boot=start_docker_on_boot),
            "customize.sh": customize_script(spec, start_docker_on_boot=start_docker_on_boot),
            "uninstall.sh": uninstall_script(spec),
        }
        entrypoints = ["post-fs-data.sh", "service.sh", "customize.sh", "uninstall.sh"]
    else:
        generated = {"install.sh": manual_install_script()}
        entrypoints = ["install.sh"]

    for rel_path, content in generated.items():
        dst = root / rel_path
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_text(content)
        executable = rel_path.endswith(".sh")
        if executable:
            os.chmod(dst, 0o755)
        files.append(RuntimeFile(rel_path, None, executable, category="entrypoint"))
    return entrypoints


def manual_install_script() -> str:
    return """#!/system/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
DEST="${DEST:-/data/adb/achost}"

copy_runtime_tree() {
    src="$1"
    dst="$2"
    find "$src" -type d | while read -r dir; do
        rel="${dir#$src}"
        rel="${rel#/}"
        [ -n "$rel" ] || continue
        mkdir -p "$dst/$rel"
    done
    find "$src" -type f | while read -r file; do
        rel="${file#$src}"
        rel="${rel#/}"
        target="$dst/$rel"
        tmp="$target.achost-new.$$"
        mkdir -p "${target%/*}"
        cp "$file" "$tmp"
        mv -f "$tmp" "$target"
    done
}

stop_old_watchdog() {
    pid_file="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
    [ -r "$pid_file" ] || return 0
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    case "$pid" in
        ''|*[!0-9]*) rm -f "$pid_file" 2>/dev/null || true; return 0 ;;
    esac
    kill "$pid" 2>/dev/null || true
    rm -f "$pid_file" 2>/dev/null || true
}

@PRUNE_STALE_RUNTIME_ENTRYPOINTS_FUNCTION@
stop_old_watchdog
mkdir -p "$DEST"
copy_runtime_tree "$SCRIPT_DIR/achost" "$DEST"
prune_stale_runtime_entrypoints "$DEST/bin"
chmod 0755 "$DEST"/bin/* 2>/dev/null || true
chmod 0755 "$DEST"/wrappers/* 2>/dev/null || true
chmod 0755 "$DEST"/etc/docker/cli-plugins/* 2>/dev/null || true
chmod 0755 "$DEST"/lxc/bin/* 2>/dev/null || true
chmod 0755 "$DEST"/lxc/share/lxc/templates/lxc-* 2>/dev/null || true

printf 'ACHOST runtime installed to %s\n' "$DEST"
printf 'Run %s/bin/achost-container-validate.sh to check installed Docker/LXC userland.\n' "$DEST"
printf 'Run %s/bin/achost-docker-runtime start after installing a Docker userland asset.\n' "$DEST"
printf 'For plain docker in this shell: export PATH=%s/wrappers:%s/bin:$PATH\n' "$DEST" "$DEST"
""".replace("@PRUNE_STALE_RUNTIME_ENTRYPOINTS_FUNCTION@", prune_stale_runtime_entrypoints_function().rstrip())


def module_prop(spec: ModuleSpec) -> str:
    requires = "".join(f"requires={item}\n" for item in spec.requires)
    return f"""id={spec.module_id}
name={spec.name}
version=0.1.1
versionCode=2
author=ccc007
{requires}description={spec.description}
"""


def post_fs_data_script() -> str:
    return """#!/system/bin/sh
MODDIR="${0%/*}"
CONF="$MODDIR/achost/etc/sysctl.d/99-container-host.conf"

[ -r "$CONF" ] || exit 0

while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
        ''|\\#*) continue ;;
    esac
    key="${line%%=*}"
    value="${line#*=}"
    proc="/proc/sys/$(printf '%s' "$key" | tr . /)"
    [ -w "$proc" ] || continue
    printf '%s\n' "$value" > "$proc" 2>/dev/null || true
done < "$CONF"
"""


def module_service_data_dirs(spec: ModuleSpec) -> str:
    dirs = [
        '"$ACHOST_VAR"',
        '"$ACHOST_CONFIG"',
        '"$ACHOST_RUN"',
        '"$ACHOST_LOG_DIR"',
    ]
    if spec.include_common or spec.include_docker:
        dirs.extend(('"$ACHOST_NATIVE_ROOT"', '"$ACHOST_VAR/bind-mounts"'))
    if spec.include_docker:
        dirs.extend(('"$ACHOST_VAR/docker"', '"$ACHOST_VAR/containerd/root"', '"$ACHOST_CONTAINERD_STATE"'))
    if spec.include_lxc:
        dirs.extend(
            (
                '"$ACHOST_VAR/lxc"',
                '"$ACHOST_VAR/lxc/rootfs"',
                '"$ACHOST_VAR/lxc/containers"',
                '"$ACHOST_VAR/run/lxc"',
                '"$ACHOST_VAR/log/lxc"',
            )
        )
    if spec.target == "legacy":
        dirs.append('"$ACHOST_CHROOT"')
    return " ".join(dirs)


def module_customize_data_dirs(spec: ModuleSpec) -> str:
    dirs = [
        '"$ACHOST_DATA"',
        '"$ACHOST_DATA/config"',
        '"$ACHOST_DATA/run"',
        '"$ACHOST_DATA/log"',
    ]
    if spec.include_common or spec.include_docker:
        dirs.extend(('"$ACHOST_DATA/native-root"', '"$ACHOST_DATA/bind-mounts"'))
    if spec.include_docker:
        dirs.extend(('"$ACHOST_DATA/docker"', '"$ACHOST_DATA/containerd/root"', '"$ACHOST_DATA/containerd/state"'))
    if spec.include_lxc:
        dirs.extend(
            (
                '"$ACHOST_DATA/lxc"',
                '"$ACHOST_DATA/lxc/rootfs"',
                '"$ACHOST_DATA/lxc/containers"',
                '"$ACHOST_DATA/run/lxc"',
                '"$ACHOST_DATA/log/lxc"',
            )
        )
    if spec.target == "legacy":
        dirs.append('"$ACHOST_DATA/chroot"')
    return " ".join(dirs)


def module_uninstall_preserve_dirs(spec: ModuleSpec) -> str:
    if spec.include_docker:
        return 'mkdir -p "$ACHOST_DATA/docker" "$ACHOST_DATA/containerd/root" 2>/dev/null || true\n'
    return ""


def base_service_guard(spec: ModuleSpec) -> str:
    if "achost-base" not in spec.requires:
        return ""
    return f'''if [ ! -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    printf 'ACHost: {spec.module_id} requires achost-base module; skipping service startup\n' >&2
    exit 0
fi

'''


def base_customize_guard(spec: ModuleSpec) -> str:
    if "achost-base" not in spec.requires:
        return ""
    return f'''if [ ! -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    print_msg "ACHost: {spec.module_id} requires achost-base module; install/enable achost-base first."
    exit 1
fi

'''


def service_script(spec: ModuleSpec, start_docker_on_boot: bool = False) -> str:
    seed_autostart = "1" if start_docker_on_boot else "0"
    mkdir_args = module_service_data_dirs(spec)
    base_guard = base_service_guard(spec)
    common_start = ""
    if spec.include_common:
        common_start = """
COMMON_BIN="${ACHOST_COMMON_BIN:-$ACHOST/bin}"
ACHOST_RUNTIME_CORE="${ACHOST_RUNTIME_CORE:-$COMMON_BIN/achost-runtime-core}"
if [ -x "$ACHOST_RUNTIME_CORE" ]; then
    "$ACHOST_RUNTIME_CORE" protect-daemons >/dev/null 2>&1 || true
    "$ACHOST_RUNTIME_CORE" net-watchdog >/dev/null 2>&1 &
fi
"""
    docker_start = ""
    if spec.include_docker:
        docker_start = ksu_docker_wrapper_install_script(install_prefix_for_mode("kernelsu-module", spec), spec) + f"""
AUTOSTART_FILE="$ACHOST_CONFIG/docker.autostart"
[ -e "$AUTOSTART_FILE" ] || printf '{seed_autostart}\n' > "$AUTOSTART_FILE" 2>/dev/null || true
if [ "$(cat "$AUTOSTART_FILE" 2>/dev/null || printf '0')" = "1" ] && [ -x "$ACHOST/bin/achost-docker-runtime" ]; then
    "$ACHOST/bin/achost-docker-runtime" start >> "$ACHOST_LOG_DIR/dockerd-start.log" 2>&1 &
fi
"""
    lxc_start = ""
    if spec.include_lxc:
        lxc_start = ksu_lxc_wrapper_install_script(install_prefix_for_mode("kernelsu-module", spec)) + """
chmod 0755 "$ACHOST/lxc/bin"/* 2>/dev/null || true
chmod 0755 "$ACHOST/lxc/share/lxc/templates"/lxc-* 2>/dev/null || true
if [ -x "$ACHOST/bin/achost-lxc-runtime" ]; then
    "$ACHOST/bin/achost-lxc-runtime" autostart >> "$ACHOST_LOG_DIR/lxc-autostart.log" 2>&1 &
fi
"""
    return f"""#!/system/bin/sh
MODDIR="${{0%/*}}"
ACHOST="${{ACHOST:-$MODDIR/achost}}"
ACHOST_DATA="{spec.data_root}"
PATH=/system/bin:/system/xbin:/vendor/bin:/product/bin:/data/adb/magisk:$PATH

{base_guard}if [ -r "$ACHOST/bin/achost-container-env.sh" ]; then
    . "$ACHOST/bin/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${{ACHOST_BASE:-/data/adb/modules/achost-base/achost}}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
fi

ACHOST_VAR="${{ACHOST_VAR:-$ACHOST_DATA}}"
ACHOST_CONFIG="${{ACHOST_CONFIG:-$ACHOST_VAR/config}}"
ACHOST_RUN="${{ACHOST_RUN:-$ACHOST_VAR/run}}"
ACHOST_LOG_DIR="${{ACHOST_LOG_DIR:-$ACHOST_VAR/log}}"
ACHOST_CHROOT="${{ACHOST_CHROOT:-$ACHOST_VAR/chroot}}"
ACHOST_NATIVE_ROOT="${{ACHOST_NATIVE_ROOT:-$ACHOST_VAR/native-root}}"
ACHOST_CONTAINERD_STATE="${{ACHOST_CONTAINERD_STATE:-$ACHOST_VAR/containerd/state}}"
mkdir -p {mkdir_args} 2>/dev/null || true
{prune_stale_runtime_entrypoints_function()}prune_stale_runtime_entrypoints "$ACHOST/bin"
{common_start}{docker_start}{lxc_start}"""


def customize_script(spec: ModuleSpec, start_docker_on_boot: bool = False) -> str:
    seed_autostart = "1" if start_docker_on_boot else "0"
    mkdir_args = module_customize_data_dirs(spec)
    base_guard = base_customize_guard(spec)
    docker_setup = ""
    if spec.include_docker:
        docker_setup = ksu_docker_wrapper_install_script(install_prefix_for_mode("kernelsu-module", spec), spec) + f"""
chmod 0755 "$ACHOST/wrappers"/* 2>/dev/null || true
chmod 0755 "$ACHOST/etc/docker/cli-plugins"/* 2>/dev/null || true
AUTOSTART_FILE="$ACHOST_DATA/config/docker.autostart"
[ -e "$AUTOSTART_FILE" ] || printf '{seed_autostart}\n' > "$AUTOSTART_FILE" 2>/dev/null || true

if [ -d "$ACHOST/var/docker" ] && [ -n "$(ls -A "$ACHOST/var/docker" 2>/dev/null)" ]; then
    print_msg "ACHost: found old module-local Docker data at $ACHOST/var/docker; leaving it untouched."
    print_msg "ACHost: persistent Docker data now lives at $ACHOST_DATA/docker."
fi
"""
    lxc_setup = ""
    if spec.include_lxc:
        lxc_setup = ksu_lxc_wrapper_install_script(install_prefix_for_mode("kernelsu-module", spec)) + """
chmod 0755 "$ACHOST/lxc/bin"/* 2>/dev/null || true
chmod 0755 "$ACHOST/lxc/share/lxc/templates"/lxc-* 2>/dev/null || true
"""
    return f"""#!/system/bin/sh
MODDIR="${{MODPATH:-${{0%/*}}}}"
ACHOST="$MODDIR/achost"
ACHOST_DATA="{spec.data_root}"

print_msg() {{
    if command -v ui_print >/dev/null 2>&1; then
        ui_print "$1"
    else
        printf '%s\n' "$1"
    fi
}}

{base_guard}mkdir -p {mkdir_args} 2>/dev/null || true
chmod 0755 "$MODDIR/post-fs-data.sh" "$MODDIR/service.sh" "$MODDIR/uninstall.sh" 2>/dev/null || true
chmod 0755 "$ACHOST/bin"/* 2>/dev/null || true
{lxc_setup}{prune_stale_runtime_entrypoints_function()}prune_stale_runtime_entrypoints "$ACHOST/bin"
{docker_setup}"""


def uninstall_script(spec: ModuleSpec) -> str:
    preserve_data_dirs = module_uninstall_preserve_dirs(spec)
    docker_stop = ""
    docker_cleanup = ""
    if spec.include_docker:
        docker_stop = """
if [ -x "$ACHOST/bin/achost-docker-runtime" ]; then
    "$ACHOST/bin/achost-docker-runtime" stop >/dev/null 2>&1 || true
fi
"""
        docker_cleanup = """
if [ -r /data/adb/ksu/bin/docker ] && grep -q 'ACHOST_DOCKER_WRAPPER' /data/adb/ksu/bin/docker 2>/dev/null; then
    rm -f /data/adb/ksu/bin/docker 2>/dev/null || true
fi
"""
    lxc_cleanup = ""
    if spec.include_lxc:
        lxc_cleanup = """
if [ -d /data/adb/ksu/bin ]; then
    for wrapper in /data/adb/ksu/bin/lxc* /data/adb/ksu/bin/lxd*; do
        [ -e "$wrapper" ] || continue
        if [ -r "$wrapper" ] && grep -q 'ACHOST_LXC_WRAPPER' "$wrapper" 2>/dev/null; then
            rm -f "$wrapper" 2>/dev/null || true
        fi
    done
fi
"""
    common_stop = ""
    if spec.include_common:
        common_stop = """
pid_file="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
if [ -r "$pid_file" ]; then
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    case "$pid" in
        ''|*[!0-9]*) ;;
        *) kill "$pid" 2>/dev/null || true ;;
    esac
fi
rm -f "$pid_file" /data/local/tmp/achost-network-watchdog.pid /data/local/tmp/achost-network-watchdog.log 2>/dev/null || true
"""
    return f"""#!/system/bin/sh
MODDIR="${{0%/*}}"
ACHOST="${{ACHOST:-$MODDIR/achost}}"
ACHOST_DATA="{spec.data_root}"
PATH=/system/bin:/system/xbin:/vendor/bin:/product/bin:/data/adb/magisk:$PATH

if [ -r "$ACHOST/bin/achost-container-env.sh" ]; then
    . "$ACHOST/bin/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${{ACHOST_BASE:-/data/adb/modules/achost-base/achost}}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
fi
{docker_stop}{common_stop}{docker_cleanup}{lxc_cleanup}
rm -rf "${{ACHOST_RUN:-$ACHOST_DATA/run}}" "${{ACHOST_LOG_DIR:-$ACHOST_DATA/log}}" "${{ACHOST_CHROOT:-$ACHOST_DATA/chroot}}" "${{ACHOST_NATIVE_ROOT:-$ACHOST_DATA/native-root}}" "${{ACHOST_CONTAINERD_STATE:-$ACHOST_DATA/containerd/state}}" 2>/dev/null || true
{preserve_data_dirs}"""


def file_entry(item: RuntimeFile) -> dict[str, Any]:
    entry: dict[str, Any] = {
        "path": item.path,
        "source": item.source,
        "executable": item.executable,
        "category": item.category,
    }
    if item.asset is not None:
        entry["asset"] = item.asset
    return entry


def format_runtime_install_report(report: dict[str, Any]) -> str:
    lines = [
        f"Runtime package: {report['output']}",
        f"mode: {report['mode']}",
        f"cgroup_mode: {report['cgroup_mode']}",
        f"docker_runtime_mode: {report['docker_runtime_mode']}",
        f"install_prefix: {report['install_prefix']}",
    ]
    if report.get("zip"):
        lines.append(f"zip: {report['zip']}")
    lines.append("entrypoints:")
    lines.extend(f"  - {entrypoint}" for entrypoint in report["entrypoints"])
    assets = report.get("assets", {})
    if assets:
        lines.append("assets:")
        for name in ("docker", "compose", "buildx", "buildkit", "lxc"):
            asset = assets.get(name)
            if asset:
                lines.append(f"  - {name}: {asset['source']} sha256={asset['sha256']}")
            else:
                lines.append(f"  - {name}: not included")
        for name in ("supervisor", "webui_api"):
            rust_binary = assets.get(name)
            if rust_binary:
                lines.append(
                    f"  - {name}: {rust_binary['path']} implementation={rust_binary['implementation']} "
                    f"target={rust_binary['target']} linker={rust_binary['linker']}"
                )
            else:
                lines.append(f"  - {name}: not included")
        if assets.get("start_docker_on_boot"):
            lines.append("  - start_docker_on_boot: enabled")
    lines.append("files:")
    for item in report["files"]:
        suffix = " executable" if item["executable"] else ""
        asset_suffix = f" asset={item['asset']}" if "asset" in item else ""
        lines.append(f"  - {item['path']}{suffix}{asset_suffix}")
    return "\n".join(lines)
