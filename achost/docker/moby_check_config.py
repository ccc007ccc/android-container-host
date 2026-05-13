from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import Any

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
ENTRY_RE = re.compile(r"^[-*]\s+([^:]+):\s*(.+)$")

ANDROID_SPECIFIC_SYMBOLS = {
    "CONFIG_ANDROID_PARANOID_NETWORK",
    "CONFIG_CGROUP_BPF",
    "CONFIG_BPF",
    "CONFIG_BPF_SYSCALL",
    "CONFIG_NETFILTER_XT_MATCH_BPF",
    "CONFIG_INET_UDP_DIAG",
}

ANDROID_IGNORED_SYMBOLS = {
    "CONFIG_AUFS_FS",
    "CONFIG_BTRFS_FS",
    "CONFIG_BLK_DEV_THROTTLING",
    "CONFIG_CFQ_GROUP_IOSCHED",
    "CONFIG_CGROUP_HUGETLB",
    "CONFIG_DM_THIN_PROVISIONING",
    "CONFIG_IP_VS",
    "CONFIG_NF_NAT_IPV6",
    "CONFIG_NF_TABLES",
    "CONFIG_VXLAN",
}


def run_moby_check(script: str | Path, config: str | Path) -> dict[str, Any]:
    script_path = Path(script).expanduser().resolve()
    config_path = Path(config).expanduser().resolve()
    if not script_path.exists():
        raise FileNotFoundError(f"moby check-config script not found: {script_path}")
    if not config_path.exists():
        raise FileNotFoundError(f"kernel config not found: {config_path}")

    proc = subprocess.run(
        ["bash", str(script_path), str(config_path)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    parsed = parse_moby_output(proc.stdout)
    parsed.update(
        {
            "script": str(script_path),
            "config": str(config_path),
            "exit_code": proc.returncode,
            "stderr": proc.stderr.strip(),
        }
    )
    return parsed


def parse_moby_output(output: str) -> dict[str, Any]:
    current_section = "unsectioned"
    entries: list[dict[str, str]] = []

    for raw_line in output.splitlines():
        line = ANSI_RE.sub("", raw_line).strip()
        if not line:
            continue
        if _is_section(line):
            current_section = line.rstrip(":")
            continue

        match = ENTRY_RE.match(line)
        if not match:
            continue

        name = match.group(1).strip()
        status = match.group(2).strip()
        entries.append(
            {
                "name": name,
                "status": status,
                "section": current_section,
                "category": classify_entry(name, status, current_section),
            }
        )

    summary: dict[str, int] = {}
    for entry in entries:
        summary[entry["category"]] = summary.get(entry["category"], 0) + 1

    missing = [entry for entry in entries if _is_missing(entry["status"])]
    return {
        "summary": summary,
        "entries": entries,
        "missing": missing,
        "raw_output": output,
    }


def classify_entry(name: str, status: str, section: str) -> str:
    symbol = _extract_symbol(name)
    section_l = section.lower()

    if symbol in ANDROID_SPECIFIC_SYMBOLS:
        return "Android-specific missing" if _is_missing(status) else "Android-specific present"
    if symbol in ANDROID_IGNORED_SYMBOLS:
        return "ignored because Android"
    if "generally necessary" in section_l:
        return "Docker required"
    if "network drivers" in section_l or "storage drivers" in section_l:
        return "Docker recommended"
    if "optional" in section_l:
        return "Docker recommended"
    return "Docker other"


def print_human(report: dict[str, Any]) -> str:
    lines = ["Moby check-config summary:"]
    for key in sorted(report["summary"]):
        lines.append(f"- {key}: {report['summary'][key]}")
    if report.get("missing"):
        lines.append("")
        lines.append("Missing or unavailable items:")
        for entry in report["missing"]:
            lines.append(
                f"[{entry['category']}] {entry['name']}: {entry['status']} ({entry['section']})"
            )
    return "\n".join(lines)


def print_json(report: dict[str, Any]) -> str:
    return json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True)


def _is_section(line: str) -> bool:
    if not line.endswith(":"):
        return False
    return not line.startswith(("-", "*"))


def _extract_symbol(name: str) -> str:
    for part in re.split(r"\s+", name):
        if part.startswith("CONFIG_"):
            return part.rstrip(",")
    return name


def _is_missing(status: str) -> bool:
    status_l = status.lower()
    return any(token in status_l for token in ("missing", "not set", "disabled", "unavailable"))
