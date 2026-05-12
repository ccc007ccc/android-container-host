from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ConfigRule:
    symbol: str
    expected: str
    level: str
    reason: str

    @property
    def config_symbol(self) -> str:
        return self.symbol if self.symbol.startswith("CONFIG_") else f"CONFIG_{self.symbol}"


PROFILE_FRAGMENTS = {
    "android-container-host-v1": [
        "common/namespaces.config",
        "common/cgroups-v1.config",
        "common/lxc-base.config",
        "common/docker-bridge-net.config",
        "common/docker-overlay2.config",
        "common/android-compat.config",
        "kernel-version/linux-4.19.config",
        "device/xiaomi-sm8250-lmi.config",
    ],
    "docker-bridge-overlay2": [
        "common/namespaces.config",
        "common/cgroups-v1.config",
        "common/docker-bridge-net.config",
        "common/docker-overlay2.config",
        "common/android-compat.config",
    ],
}

PROFILE_RULES = {
    "android-container-host-v1": [
        ConfigRule("NAMESPACES", "y", "required", "namespace support base"),
        ConfigRule("UTS_NS", "y", "required", "container hostname isolation"),
        ConfigRule("IPC_NS", "y", "required", "container IPC isolation"),
        ConfigRule("PID_NS", "y", "required", "container process namespace"),
        ConfigRule("NET_NS", "y", "required", "container network namespace"),
        ConfigRule("USER_NS", "y", "recommended", "unprivileged LXC and rootless paths"),
        ConfigRule("CGROUPS", "y", "required", "cgroup hierarchy base"),
        ConfigRule("CGROUP_DEVICE", "y", "required", "LXC device access policy"),
        ConfigRule("CGROUP_PIDS", "y", "required", "process count limits"),
        ConfigRule("CGROUP_FREEZER", "y", "required", "LXC freezer controller"),
        ConfigRule("CGROUP_CPUACCT", "y", "required", "CPU accounting"),
        ConfigRule("CGROUP_SCHED", "y", "required", "CPU cgroup scheduling"),
        ConfigRule("CPUSETS", "y", "required", "cpuset controller"),
        ConfigRule("MEMCG", "y", "required", "memory controller"),
        ConfigRule("MEMCG_SWAP", "y", "recommended", "Docker memory plus swap limits"),
        ConfigRule("POSIX_MQUEUE", "y", "required", "OCI and LXC process expectations"),
        ConfigRule("FHANDLE", "y", "required", "container runtime file handle support"),
        ConfigRule("DEVPTS_MULTIPLE_INSTANCES", "y", "recommended", "older kernels expose devpts newinstance as a config symbol"),
        ConfigRule("SECCOMP", "y", "required", "container seccomp support"),
        ConfigRule("SECCOMP_FILTER", "y", "required", "container seccomp filter mode"),
        ConfigRule("KEYS", "y", "required", "LXC keyring support"),
        ConfigRule("VETH", "y", "required", "container veth pairs"),
        ConfigRule("BRIDGE", "y", "required", "Docker bridge network"),
        ConfigRule("BRIDGE_NETFILTER", "y", "required", "iptables visibility for bridged traffic"),
        ConfigRule("TUN", "y", "required", "container VPN and tunnel support"),
        ConfigRule("OVERLAY_FS", "y", "required", "Docker overlay2 driver"),
        ConfigRule("EXT4_FS_POSIX_ACL", "y", "required", "overlay2 backing filesystem ACLs"),
        ConfigRule("EXT4_FS_SECURITY", "y", "required", "overlay2 backing filesystem xattrs"),
        ConfigRule("TMPFS_XATTR", "y", "required", "tmpfs xattrs for runtime mounts"),
        ConfigRule("TMPFS_POSIX_ACL", "y", "required", "tmpfs ACL support"),
        ConfigRule("NETFILTER", "y", "required", "iptables base"),
        ConfigRule("NETFILTER_XTABLES", "y", "required", "xtables base"),
        ConfigRule("NF_CONNTRACK", "y", "required", "NAT connection tracking"),
        ConfigRule("NF_NAT", "y", "required", "NAT base"),
        ConfigRule("IP_NF_IPTABLES", "y", "required", "IPv4 iptables"),
        ConfigRule("IP_NF_FILTER", "y", "required", "IPv4 filter table"),
        ConfigRule("IP_NF_MANGLE", "y", "required", "IPv4 mangle table"),
        ConfigRule("IP_NF_NAT", "y", "required", "IPv4 NAT table"),
        ConfigRule("IP_NF_TARGET_MASQUERADE", "y", "required", "Docker bridge masquerade"),
        ConfigRule("NETFILTER_XT_MATCH_ADDRTYPE", "y", "required", "Docker bridge addrtype rules"),
        ConfigRule("NETFILTER_XT_MATCH_CONNTRACK", "y", "required", "Docker established traffic rules"),
        ConfigRule("NETFILTER_XT_MATCH_CGROUP", "y", "recommended", "Android/container traffic classification"),
        ConfigRule("NETFILTER_XT_TARGET_CHECKSUM", "y", "required", "Docker DHCP checksum target"),
        ConfigRule("NETFILTER_XT_TARGET_MARK", "y", "required", "Android and Docker mark rules"),
        ConfigRule("NETFILTER_XT_MATCH_MARK", "y", "required", "Android and Docker mark matches"),
        ConfigRule("ANDROID_PARANOID_NETWORK", "not set", "required", "non-Android UIDs need normal network access"),
        ConfigRule("PSI", "y", "recommended", "lmkd and pressure diagnostics"),
        ConfigRule("CGROUP_BPF", "y", "recommended", "Android eBPF traffic monitoring"),
        ConfigRule("BPF", "y", "recommended", "BPF base"),
        ConfigRule("BPF_SYSCALL", "y", "recommended", "BPF syscall support"),
        ConfigRule("NETFILTER_XT_MATCH_BPF", "y", "recommended", "Android eBPF traffic filters"),
        ConfigRule("INET_UDP_DIAG", "y", "recommended", "Android traffic monitoring diagnostics"),
    ],
    "docker-bridge-overlay2": [
        ConfigRule("NAMESPACES", "y", "required", "namespace support base"),
        ConfigRule("UTS_NS", "y", "required", "container hostname isolation"),
        ConfigRule("IPC_NS", "y", "required", "container IPC isolation"),
        ConfigRule("PID_NS", "y", "required", "container process namespace"),
        ConfigRule("NET_NS", "y", "required", "container network namespace"),
        ConfigRule("CGROUPS", "y", "required", "cgroup hierarchy base"),
        ConfigRule("CGROUP_DEVICE", "y", "required", "device cgroup controller"),
        ConfigRule("CGROUP_PIDS", "y", "required", "process count limits"),
        ConfigRule("MEMCG", "y", "required", "memory controller"),
        ConfigRule("SECCOMP", "y", "required", "container seccomp support"),
        ConfigRule("SECCOMP_FILTER", "y", "required", "container seccomp filter mode"),
        ConfigRule("VETH", "y", "required", "container veth pairs"),
        ConfigRule("BRIDGE", "y", "required", "Docker bridge network"),
        ConfigRule("BRIDGE_NETFILTER", "y", "required", "iptables visibility for bridged traffic"),
        ConfigRule("OVERLAY_FS", "y", "required", "Docker overlay2 driver"),
        ConfigRule("NETFILTER", "y", "required", "iptables base"),
        ConfigRule("NF_CONNTRACK", "y", "required", "NAT connection tracking"),
        ConfigRule("NF_NAT", "y", "required", "NAT base"),
        ConfigRule("IP_NF_NAT", "y", "required", "IPv4 NAT table"),
        ConfigRule("IP_NF_TARGET_MASQUERADE", "y", "required", "Docker bridge masquerade"),
        ConfigRule("ANDROID_PARANOID_NETWORK", "not set", "required", "non-Android UIDs need normal network access"),
    ],
}


def split_profiles(profile: str | list[str] | tuple[str, ...]) -> list[str]:
    if isinstance(profile, str):
        names = [part.strip() for part in profile.split(",")]
    else:
        names = [str(part).strip() for part in profile]
    return [name for name in names if name]


def get_profile_rules(profile: str | list[str] | tuple[str, ...]) -> list[ConfigRule]:
    rules: dict[str, ConfigRule] = {}
    for name in split_profiles(profile):
        if name not in PROFILE_RULES:
            raise KeyError(f"unknown profile: {name}")
        for rule in PROFILE_RULES[name]:
            existing = rules.get(rule.config_symbol)
            if existing is None or _level_rank(rule.level) < _level_rank(existing.level):
                rules[rule.config_symbol] = rule
    return list(rules.values())


def get_profile_fragments(profile: str | list[str] | tuple[str, ...]) -> list[str]:
    fragments: list[str] = []
    seen: set[str] = set()
    for name in split_profiles(profile):
        if name not in PROFILE_FRAGMENTS:
            raise KeyError(f"unknown profile: {name}")
        for fragment in PROFILE_FRAGMENTS[name]:
            if fragment not in seen:
                fragments.append(fragment)
                seen.add(fragment)
    return fragments


def fragment_paths(project_root: Path, profile: str | list[str] | tuple[str, ...]) -> list[Path]:
    base = project_root / "config" / "fragments"
    return [base / fragment for fragment in get_profile_fragments(profile)]


def _level_rank(level: str) -> int:
    return {"required": 0, "recommended": 1, "optional": 2, "risky": 3, "unavailable": 4}.get(level, 9)
