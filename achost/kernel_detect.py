from __future__ import annotations

import re
from pathlib import Path
from typing import Any


def parse_config(config_path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not config_path.exists():
        return values

    for raw_line in config_path.read_text(errors="replace").splitlines():
        line = raw_line.strip()
        if line.startswith("CONFIG_") and "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
        elif line.startswith("# CONFIG_") and line.endswith(" is not set"):
            key = line[2:].split(" is not set", 1)[0]
            values[key] = "not set"
    return values


def config_value(config: dict[str, str], symbol: str) -> str:
    key = symbol if symbol.startswith("CONFIG_") else f"CONFIG_{symbol}"
    return config.get(key, "missing")


def detect_kernel(kernel_tree: str | Path, out: str | Path | None = None) -> dict[str, Any]:
    tree = Path(kernel_tree).expanduser().resolve()
    out_dir = Path(out).expanduser().resolve() if out else tree / "out"
    config_path = out_dir / ".config"
    if not config_path.exists() and (tree / ".config").exists():
        config_path = tree / ".config"

    config = parse_config(config_path)
    version = _detect_version(tree)
    arch = _detect_arch(config, tree)
    defconfigs = _find_defconfig_candidates(tree, arch)
    android_markers = _android_markers(tree, config)
    features = _detect_features(tree, config)
    risks = _detect_risks(features, config)

    return {
        "kernel_tree": str(tree),
        "out": str(out_dir),
        "kernel_version": version,
        "arch": arch,
        "android_kernel": bool(android_markers),
        "android_markers": android_markers,
        "gki": _detect_gki(tree, config),
        "defconfig_candidates": defconfigs,
        "generated_config": str(config_path) if config_path.exists() else None,
        "recommended_profile": "android-container-host-v1",
        "features": features,
        "risk": risks,
    }


def _detect_version(tree: Path) -> str:
    assignments = _read_makefile_assignments(tree / "Makefile")
    parts = [assignments.get("VERSION"), assignments.get("PATCHLEVEL"), assignments.get("SUBLEVEL")]
    if all(parts):
        version = ".".join(parts)
        extra = assignments.get("EXTRAVERSION", "")
        return f"{version}{extra}" if extra else version
    return "unknown"


def _read_makefile_assignments(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    pattern = re.compile(r"^([A-Z][A-Z0-9_]*)\s*=\s*(.*)$")
    for line in path.read_text(errors="replace").splitlines():
        match = pattern.match(line)
        if match:
            values[match.group(1)] = match.group(2).strip()
    return values


def _detect_arch(config: dict[str, str], tree: Path) -> str:
    if config_value(config, "ARM64") == "y":
        return "arm64"
    if config_value(config, "ARM") == "y":
        return "arm"
    if (tree / "arch" / "arm64").exists():
        return "arm64"
    if (tree / "arch" / "arm").exists():
        return "arm"
    return "unknown"


def _find_defconfig_candidates(tree: Path, arch: str) -> list[str]:
    roots: list[Path] = []
    if arch != "unknown":
        roots.append(tree / "arch" / arch / "configs")
    roots.extend(sorted((tree / "arch").glob("*/configs")))

    seen: set[Path] = set()
    candidates: list[Path] = []
    for root in roots:
        if not root.exists() or root in seen:
            continue
        seen.add(root)
        candidates.extend(root.glob("*defconfig"))

    def sort_key(path: Path) -> tuple[int, str]:
        name = path.name
        priority = 0 if name == "lmi_defconfig" else 1 if "lmi" in name else 2
        return priority, str(path)

    return [_rel(path, tree) for path in sorted(candidates, key=sort_key)]


def _android_markers(tree: Path, config: dict[str, str]) -> list[str]:
    markers: list[str] = []
    if (tree / "drivers" / "android").exists():
        markers.append("drivers/android")
    for symbol in ("ANDROID_BINDER_IPC", "ANDROID_BINDERFS", "ASHMEM", "ANDROID_LOW_MEMORY_KILLER"):
        if config_value(config, symbol) == "y":
            markers.append(f"CONFIG_{symbol}=y")
    if (tree / "net" / "netfilter" / "xt_qtaguid.c").exists():
        markers.append("net/netfilter/xt_qtaguid.c")
    if config_value(config, "ANDROID_PARANOID_NETWORK") != "missing":
        markers.append("CONFIG_ANDROID_PARANOID_NETWORK")
    return markers


def _detect_gki(tree: Path, config: dict[str, str]) -> bool:
    if (tree / "common").exists() and config_value(config, "GKI_HACKS_TO_FIX") != "missing":
        return True
    if config_value(config, "TRIM_UNUSED_KSYMS") == "y" and (tree / "android" / "abi_gki_aarch64").exists():
        return True
    return False


def _detect_features(tree: Path, config: dict[str, str]) -> dict[str, Any]:
    return {
        "overlayfs": _feature(tree, config, "fs/overlayfs/Kconfig", "OVERLAY_FS"),
        "veth": _feature(tree, config, "drivers/net/veth.c", "VETH"),
        "bridge": _feature(tree, config, "net/bridge/Kconfig", "BRIDGE"),
        "bridge_netfilter": _feature(tree, config, "net/bridge/br_netfilter_hooks.c", "BRIDGE_NETFILTER"),
        "netfilter": _feature(tree, config, "net/netfilter/Kconfig", "NETFILTER"),
        "ipv4_nat": _feature(tree, config, "net/ipv4/netfilter/Kconfig", "IP_NF_NAT"),
        "qtaguid": {
            "source": (tree / "net" / "netfilter" / "xt_qtaguid.c").exists(),
            "config": config_value(config, "NETFILTER_XT_MATCH_QTAGUID"),
            "owner_config": config_value(config, "NETFILTER_XT_MATCH_OWNER"),
        },
        "android_paranoid_network": {
            "source": _text_contains(tree / "net" / "Kconfig", "ANDROID_PARANOID_NETWORK"),
            "config": config_value(config, "ANDROID_PARANOID_NETWORK"),
        },
        "cgroup_v1": {
            "source": (tree / "kernel" / "cgroup" / "cgroup-v1.c").exists(),
            "config": config_value(config, "CGROUPS"),
        },
        "cgroup_v2": {
            "source": _text_contains(tree / "kernel" / "cgroup" / "cgroup.c", "cgroup2"),
            "config": config_value(config, "CGROUPS"),
        },
        "cgroup_noprefix": {
            "source": _text_contains(tree / "kernel" / "cgroup" / "cgroup-v1.c", "noprefix"),
            "cpuset_legacy_mount": _text_contains(tree / "kernel" / "cgroup" / "cpuset.c", "cpuset,noprefix"),
        },
        "seccomp": {"config": config_value(config, "SECCOMP")},
        "seccomp_filter": {"config": config_value(config, "SECCOMP_FILTER")},
    }


def _feature(tree: Path, config: dict[str, str], source: str, symbol: str) -> dict[str, Any]:
    return {
        "source": (tree / source).exists(),
        "config": config_value(config, symbol),
    }


def _detect_risks(features: dict[str, Any], config: dict[str, str]) -> list[str]:
    risks: list[str] = []
    if features["android_paranoid_network"]["config"] == "y":
        risks.append("CONFIG_ANDROID_PARANOID_NETWORK=y may block networking for non-Android UIDs")
    if features["qtaguid"]["source"] and features["qtaguid"]["config"] != "y":
        risks.append("xt_qtaguid source exists but CONFIG_NETFILTER_XT_MATCH_QTAGUID is not enabled")
    if features["qtaguid"]["source"] and features["qtaguid"]["owner_config"] == "y":
        risks.append("CONFIG_NETFILTER_XT_MATCH_OWNER=y conflicts with qtaguid Kconfig dependency in this tree")
    if config_value(config, "BRIDGE_NETFILTER") != "y":
        risks.append("CONFIG_BRIDGE_NETFILTER is disabled; Docker bridge traffic may bypass iptables expectations")
    if config_value(config, "PID_NS") != "y":
        risks.append("CONFIG_PID_NS is missing; OCI containers cannot get normal PID namespaces")
    if features["cgroup_noprefix"]["cpuset_legacy_mount"]:
        risks.append("cpuset legacy mount uses noprefix; runc/LXC cpuset file names need runtime validation")
    return risks


def _text_contains(path: Path, needle: str) -> bool:
    if not path.exists():
        return False
    return needle in path.read_text(errors="replace")


def _rel(path: Path, tree: Path) -> str:
    try:
        return path.relative_to(tree).as_posix()
    except ValueError:
        return str(path)
