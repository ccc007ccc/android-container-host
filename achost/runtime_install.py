from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tarfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any

PROJECT_ROOT = Path(__file__).resolve().parents[1]
RUNTIME_ROOT = PROJECT_ROOT / "runtime" / "android"
SCRIPT_ROOT = PROJECT_ROOT / "scripts"
MODULE_ID = "achost-runtime"
SUPPORTED_MODES = ("manual", "kernelsu-module")
SUPPORTED_CGROUP_MODES = ("v1", "v2")
SUPPORTED_DOCKER_RUNTIME_MODES = ("chroot", "native")

COMMON_RUNTIME_FILES = (
    (RUNTIME_ROOT / "net" / "detect-uplink.sh", "achost/bin/detect-uplink.sh"),
    (RUNTIME_ROOT / "net" / "container-nat-manager.sh", "achost/bin/container-nat-manager.sh"),
    (RUNTIME_ROOT / "net" / "container-network-watchdog.sh", "achost/bin/container-network-watchdog.sh"),
    (RUNTIME_ROOT / "memory" / "protect-container-daemons.sh", "achost/bin/protect-container-daemons.sh"),
    (RUNTIME_ROOT / "bin" / "achost-container-env.sh", "achost/bin/achost-container-env.sh"),
    (RUNTIME_ROOT / "bin" / "achost-container-validate.sh", "achost/bin/achost-container-validate.sh"),
    (SCRIPT_ROOT / "runtime-net-debug.sh", "achost/bin/runtime-net-debug.sh"),
    (SCRIPT_ROOT / "runtime-test.sh", "achost/bin/runtime-test.sh"),
    (SCRIPT_ROOT / "collect-logs.sh", "achost/bin/collect-logs.sh"),
)
DOCKER_RUNTIME_FILES = (
    (RUNTIME_ROOT / "docker" / "bin" / "achost-docker-start.sh", "achost/bin/achost-docker-start.sh"),
    (RUNTIME_ROOT / "docker" / "bin" / "achost-docker-stop.sh", "achost/bin/achost-docker-stop.sh"),
    (RUNTIME_ROOT / "docker" / "net" / "restore-docker-iptables.sh", "achost/bin/restore-docker-iptables.sh"),
    (SCRIPT_ROOT / "docker" / "runtime-smoke-docker.sh", "achost/bin/runtime-smoke-docker.sh"),
    (SCRIPT_ROOT / "docker" / "runtime-docker-feature-test.sh", "achost/bin/runtime-docker-feature-test.sh"),
)
LXC_RUNTIME_FILES = (
    (RUNTIME_ROOT / "bin" / "achost-lxc-validate.sh", "achost/bin/achost-lxc-validate.sh"),
    (SCRIPT_ROOT / "runtime-smoke-lxc.sh", "achost/bin/runtime-smoke-lxc.sh"),
    (SCRIPT_ROOT / "verify-lxc-checkconfig.sh", "achost/bin/verify-lxc-checkconfig.sh"),
)

LXC_FILES = ("android-common.conf", "default.conf", "unprivileged.conf")
DOCKER_REQUIRED_BINARIES = ("docker", "dockerd", "containerd", "containerd-shim-runc-v2", "ctr", "runc")
DOCKER_OPTIONAL_BINARIES = ("containerd-shim", "docker-init", "docker-proxy", "containerd-stress")
COMPOSE_ASSET_NAMES = ("docker-compose", "docker-compose-linux-aarch64", "docker-compose-linux-arm64")
COMPOSE_PLUGIN_REL = "achost/etc/docker/cli-plugins/docker-compose"
COMPOSE_STANDALONE_REL = "achost/bin/docker-compose"
BUILDX_PLUGIN_REL = "achost/etc/docker/cli-plugins/docker-buildx"
BUILDX_STANDALONE_REL = "achost/bin/docker-buildx"
BUILDKIT_REQUIRED_BINARIES = ("buildctl", "buildkitd")
SUPERVISOR_SOURCE = RUNTIME_ROOT / "native" / "achost-supervise.c"
SUPERVISOR_DEST = "achost/bin/achost-supervise"
LXC_ALLOWED_ROOTS = ("bin", "lib", "lib64", "share")


@dataclass(frozen=True)
class RuntimeFile:
    path: str
    source: str | None
    executable: bool
    asset: str | None = None
    category: str = "common"


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
    docker_runtime_mode: str = "chroot",
) -> dict[str, Any]:
    if mode not in SUPPORTED_MODES:
        raise ValueError(f"unsupported runtime package mode: {mode}")
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

    root = Path(output).expanduser().resolve()
    ensure_empty_output(root)
    root.mkdir(parents=True, exist_ok=True)

    install_prefix = install_prefix_for_mode(mode)
    replacements = {"@ACHOST_PREFIX@": install_prefix}
    files: list[RuntimeFile] = []

    for category, runtime_files in (
        ("common", COMMON_RUNTIME_FILES),
        ("docker", DOCKER_RUNTIME_FILES),
        ("lxc", LXC_RUNTIME_FILES),
    ):
        for src, dst in runtime_files:
            files.append(copy_text_file(src, root / dst, root, executable=True, category=category))

    files.append(
        copy_text_file(
            RUNTIME_ROOT / "docker" / "etc" / f"daemon.cgroup-{cgroup_mode}.json",
            root / "achost" / "etc" / "docker" / "daemon.json",
            root,
            category="docker",
        )
    )
    files.append(write_runtime_config(root, docker_runtime_mode, cgroup_mode))
    files.append(
        copy_text_file(
            RUNTIME_ROOT / "sysctl" / "99-container-host.conf",
            root / "achost" / "etc" / "sysctl.d" / "99-container-host.conf",
            root,
            category="common",
        )
    )
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

    ensure_runtime_dirs(root)
    files.extend(write_docker_wrappers(root, mode, install_prefix))
    supervisor_report, supervisor_files = install_supervisor_helper(root)
    files.extend(supervisor_files)
    assets: dict[str, Any] = {
        "docker": None,
        "compose": None,
        "buildx": None,
        "buildkit": None,
        "lxc": None,
        "supervisor": supervisor_report,
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

    entrypoints = write_mode_files(root, mode, files, start_docker_on_boot=start_docker_on_boot)
    files.append(RuntimeFile("manifest.json", None, False))
    report = {
        "mode": mode,
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


def install_prefix_for_mode(mode: str) -> str:
    if mode == "kernelsu-module":
        return f"/data/adb/modules/{MODULE_ID}/achost"
    return "/data/adb/achost"


def ensure_empty_output(root: Path) -> None:
    if root.exists() and not root.is_dir():
        raise FileExistsError(f"output exists and is not a directory: {root}")
    if root.exists() and any(root.iterdir()):
        raise FileExistsError(f"output directory is not empty: {root}")


def ensure_runtime_dirs(root: Path) -> None:
    for rel_path in (
        "achost/etc/docker/cli-plugins",
        "achost/var/docker",
        "achost/var/run",
        "achost/var/log",
        "achost/var/containerd/root",
        "achost/var/containerd/state",
    ):
        (root / rel_path).mkdir(parents=True, exist_ok=True)


def write_runtime_config(root: Path, docker_runtime_mode: str, cgroup_mode: str) -> RuntimeFile:
    use_chroot = "0" if docker_runtime_mode == "native" else "1"
    dst = root / "achost" / "etc" / "achost-runtime.conf"
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(
        f"ACHOST_RUNTIME_MODE={docker_runtime_mode}\n"
        f"ACHOST_USE_CHROOT={use_chroot}\n"
        f"ACHOST_CGROUP_MODE={cgroup_mode}\n"
    )
    return RuntimeFile(str(dst.relative_to(root)), None, False)


def write_executable_text(root: Path, rel_path: str, text: str, category: str = "common") -> RuntimeFile:
    dst = root / rel_path
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text)
    os.chmod(dst, 0o755)
    return RuntimeFile(str(dst.relative_to(root)), None, True, category=category)


def write_docker_wrappers(root: Path, mode: str, install_prefix: str) -> list[RuntimeFile]:
    files = [
        write_executable_text(
            root,
            "achost/wrappers/docker",
            manual_docker_wrapper(),
            category="docker",
        )
    ]
    if mode == "kernelsu-module":
        files.append(
            write_executable_text(
                root,
                "system/bin/docker",
                module_docker_wrapper(install_prefix),
                category="docker",
            )
        )
    return files


def manual_docker_wrapper() -> str:
    return '''#!/system/bin/sh
set -u
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
. "$ACHOST/bin/achost-container-env.sh"
exec "$ACHOST/bin/docker" "$@"
'''


def module_docker_wrapper(install_prefix: str) -> str:
    return f'''#!/system/bin/sh
set -u
ACHOST="${{ACHOST:-{install_prefix}}}"
. "$ACHOST/bin/achost-container-env.sh"
exec "$ACHOST/bin/docker" "$@"
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


def install_supervisor_helper(root: Path) -> tuple[dict[str, Any] | None, list[RuntimeFile]]:
    if not SUPERVISOR_SOURCE.exists():
        return None, []

    compiler = find_aarch64_compiler()
    if compiler is None:
        if os.environ.get("ACHOST_REQUIRE_SUPERVISOR") == "1":
            raise FileNotFoundError("no Android arm64 compiler found for achost-supervise")
        return None, []

    dst = root / SUPERVISOR_DEST
    dst.parent.mkdir(parents=True, exist_ok=True)
    command = [
        compiler,
        "-O2",
        "-Wall",
        "-Wextra",
        "-fPIE",
        "-pie",
        "-o",
        str(dst),
        str(SUPERVISOR_SOURCE),
    ]
    try:
        subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except subprocess.CalledProcessError as exc:
        message = exc.stderr.strip() or exc.stdout.strip() or str(exc)
        if os.environ.get("ACHOST_AARCH64_CC") or os.environ.get("ACHOST_REQUIRE_SUPERVISOR") == "1":
            raise RuntimeError(f"failed to build achost-supervise: {message}") from exc
        return None, []

    os.chmod(dst, 0o755)
    return {
        "source": str(SUPERVISOR_SOURCE.relative_to(PROJECT_ROOT)),
        "compiler": compiler,
        "path": SUPERVISOR_DEST,
    }, [RuntimeFile(str(dst.relative_to(root)), str(SUPERVISOR_SOURCE.relative_to(PROJECT_ROOT)), True, category="supervisor")]


def find_aarch64_compiler() -> str | None:
    explicit = os.environ.get("ACHOST_AARCH64_CC")
    if explicit:
        explicit_path = Path(explicit).expanduser()
        if explicit_path.exists():
            return str(explicit_path.resolve())
        found = shutil.which(explicit)
        if found:
            return found
        raise FileNotFoundError(f"ACHOST_AARCH64_CC not found: {explicit}")

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
        "aarch64-linux-gnu-gcc",
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
    extracted: list[str] = []
    with tarfile.open(asset_path, "r:*") as archive:
        for member in archive.getmembers():
            if not member.isfile():
                continue
            rel_path = lxc_destination(member.name)
            if rel_path is None:
                continue
            dst = root / rel_path
            executable = "/bin/" in f"/{rel_path}"
            mode = 0o755 if executable else (member.mode & 0o777 or 0o644)
            copy_tar_member(archive, member, dst, mode)
            extracted.append(rel_path)
            files.append(RuntimeFile(str(dst.relative_to(root)), str(asset_path), executable, "lxc", "lxc-asset"))

    if not extracted:
        raise ValueError("lxc asset contained no supported files")

    return {
        "source": str(asset_path),
        "sha256": digest,
        "files": sorted(extracted),
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


def write_mode_files(root: Path, mode: str, files: list[RuntimeFile], start_docker_on_boot: bool = False) -> list[str]:
    if mode == "kernelsu-module":
        generated = {
            "module.prop": module_prop(),
            "post-fs-data.sh": post_fs_data_script(),
            "service.sh": service_script(start_docker_on_boot=start_docker_on_boot),
        }
        entrypoints = ["post-fs-data.sh", "service.sh"]
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
        files.append(RuntimeFile(rel_path, None, executable))
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

stop_old_watchdog
mkdir -p "$DEST"
copy_runtime_tree "$SCRIPT_DIR/achost" "$DEST"
chmod 0755 "$DEST"/bin/* 2>/dev/null || true
chmod 0755 "$DEST"/wrappers/* 2>/dev/null || true
chmod 0755 "$DEST"/etc/docker/cli-plugins/* 2>/dev/null || true
chmod 0755 "$DEST"/lxc/bin/* 2>/dev/null || true

printf 'ACHOST runtime installed to %s\n' "$DEST"
printf 'Run %s/bin/achost-container-validate.sh to check installed Docker/LXC userland.\n' "$DEST"
printf 'Run %s/bin/achost-docker-start.sh after installing a Docker userland asset.\n' "$DEST"
printf 'For plain docker in this shell: export PATH=%s/wrappers:%s/bin:$PATH\n' "$DEST" "$DEST"
"""


def module_prop() -> str:
    return """id=achost-runtime
name=Android Container Host Runtime
version=0.1.0
versionCode=1
author=ccc007
description=Runtime scripts and default configs for Docker/LXC on Android container host kernels.
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


def service_script(start_docker_on_boot: bool = False) -> str:
    if start_docker_on_boot:
        docker_start = """
if [ -x "$ACHOST/bin/achost-docker-start.sh" ]; then
    "$ACHOST/bin/achost-docker-start.sh" >/dev/null 2>&1 &
fi
"""
    else:
        docker_start = """
if [ -x "$ACHOST/bin/protect-container-daemons.sh" ]; then
    "$ACHOST/bin/protect-container-daemons.sh" >/dev/null 2>&1 || true
fi

if [ -x "$ACHOST/bin/container-network-watchdog.sh" ]; then
    ACHOST_NET_LOG="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}" "$ACHOST/bin/container-network-watchdog.sh" >/dev/null 2>&1 &
fi
"""
    return f"""#!/system/bin/sh
MODDIR="${{0%/*}}"
ACHOST="$MODDIR/achost"
PATH=/system/bin:/system/xbin:/vendor/bin:/product/bin:/data/adb/magisk:$PATH
{docker_start}"""


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
        "entrypoints:",
    ]
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
        supervisor = assets.get("supervisor")
        if supervisor:
            lines.append(f"  - supervisor: {supervisor['path']} compiler={supervisor['compiler']}")
        else:
            lines.append("  - supervisor: not included")
        if assets.get("start_docker_on_boot"):
            lines.append("  - start_docker_on_boot: enabled")
    lines.append("files:")
    for item in report["files"]:
        suffix = " executable" if item["executable"] else ""
        asset_suffix = f" asset={item['asset']}" if "asset" in item else ""
        lines.append(f"  - {item['path']}{suffix}{asset_suffix}")
    return "\n".join(lines)
