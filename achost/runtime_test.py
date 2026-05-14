from __future__ import annotations

import shlex
from typing import Any

DEFAULT_PACKAGE_ROOT = "/data/adb/achost"
DEFAULT_OUT_DIR = "/data/local/tmp/achost-runtime-test"
SUPPORTED_TARGETS = ("all", "network", "docker", "lxc")
LXC_ROOTFS_NOTE = "Set ROOTFS_ASSET to a tar/tar.gz rootfs to run LXC import/start/smoke; without it runtime-smoke-lxc skips container boot."

STEPS = {
    "network": ("runtime-net-debug", "collect-logs"),
    "docker": (
        "achost-runtime-core protect-daemons",
        "achost-docker-runtime start",
        "achost-runtime-core net-reconcile",
        "runtime-smoke-docker",
        "runtime-docker-feature-test",
        "runtime-net-debug",
        "achost-docker-runtime stop",
        "collect-logs",
    ),
    "lxc": (
        "achost-lxc-runtime write-configs",
        "achost-lxc-runtime validate-host",
        "achost-lxc-runtime validate-assets",
        "achost-lxc-runtime prepare-bridge",
        "runtime-smoke-lxc",
        "collect-logs",
    ),
    "all": (
        "runtime-net-debug",
        "achost-runtime-core protect-daemons",
        "achost-docker-runtime start",
        "achost-runtime-core net-reconcile",
        "runtime-smoke-docker",
        "runtime-docker-feature-test",
        "achost-docker-runtime stop",
        "achost-lxc-runtime write-configs",
        "achost-lxc-runtime validate-host",
        "achost-lxc-runtime validate-assets",
        "achost-lxc-runtime prepare-bridge",
        "runtime-smoke-lxc",
        "collect-logs",
    ),
}


def build_runtime_test_report(
    package_root: str = DEFAULT_PACKAGE_ROOT,
    target: str = "all",
    out_dir: str = DEFAULT_OUT_DIR,
) -> dict[str, Any]:
    if target not in SUPPORTED_TARGETS:
        raise ValueError(f"unsupported runtime test target: {target}")
    if not package_root.startswith("/"):
        raise ValueError("package root must be an Android absolute path")
    if not out_dir.startswith("/"):
        raise ValueError("output directory must be an Android absolute path")

    root = package_root.rstrip("/")
    script = f"{root}/bin/runtime-test.sh"
    command = " ".join(
        (
            f"MODE={shlex.quote(target)}",
            f"OUT_DIR={shlex.quote(out_dir)}",
            shlex.quote(script),
        )
    )
    notes = []
    if target in ("all", "lxc"):
        notes.append(LXC_ROOTFS_NOTE)

    return {
        "target": target,
        "package_root": root,
        "out_dir": out_dir,
        "script": script,
        "command": command,
        "steps": list(STEPS[target]),
        "notes": notes,
    }


def format_runtime_test_report(report: dict[str, Any]) -> str:
    lines = [
        "Runtime test command:",
        f"  {report['command']}",
        f"package_root: {report['package_root']}",
        f"out_dir: {report['out_dir']}",
        f"target: {report['target']}",
        "steps:",
    ]
    lines.extend(f"  - {step}" for step in report["steps"])
    notes = report.get("notes") or []
    if notes:
        lines.append("notes:")
        lines.extend(f"  - {note}" for note in notes)
    return "\n".join(lines)
