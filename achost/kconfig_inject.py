from __future__ import annotations

import shutil
from pathlib import Path
from typing import Any

PROJECT_ROOT = Path(__file__).resolve().parents[1]
PROJECT_KCONFIG = PROJECT_ROOT / "Kconfig" / "AndroidContainerHost.Kconfig"
DEFAULT_DESTINATION = "vendor/android-container-host/AndroidContainerHost.Kconfig"


def inject_kconfig_report(
    kernel_tree: str | Path,
    destination: str = DEFAULT_DESTINATION,
    apply: bool = False,
) -> dict[str, Any]:
    tree = Path(kernel_tree).expanduser().resolve()
    if not tree.exists():
        raise FileNotFoundError(f"kernel tree not found: {tree}")
    root_kconfig = tree / "Kconfig"
    if not root_kconfig.exists():
        raise FileNotFoundError(f"target root Kconfig not found: {root_kconfig}")
    if not PROJECT_KCONFIG.exists():
        raise FileNotFoundError(f"project Kconfig not found: {PROJECT_KCONFIG}")

    dest_rel = validate_destination(destination)
    dest_path = tree / dest_rel
    source_line = f'source "{dest_rel.as_posix()}"'
    root_text = root_kconfig.read_text(errors="replace")
    already_sourced = source_line in root_text
    actions = build_actions(dest_path, already_sourced, source_line)

    if apply:
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(PROJECT_KCONFIG, dest_path)
        if not already_sourced:
            with root_kconfig.open("a") as handle:
                if root_text and not root_text.endswith("\n"):
                    handle.write("\n")
                handle.write(f"\n{source_line}\n")

    return {
        "kernel_tree": str(tree),
        "mode": "apply" if apply else "dry-run",
        "project_kconfig": str(PROJECT_KCONFIG),
        "target_kconfig": str(root_kconfig),
        "destination": dest_rel.as_posix(),
        "source_line": source_line,
        "already_sourced": already_sourced,
        "destination_exists": dest_path.exists(),
        "actions": actions,
        "applied": apply,
    }


def validate_destination(destination: str) -> Path:
    path = Path(destination)
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("destination must be a relative path inside the kernel tree")
    if path.name != "AndroidContainerHost.Kconfig":
        raise ValueError("destination must end with AndroidContainerHost.Kconfig")
    return path


def build_actions(dest_path: Path, already_sourced: bool, source_line: str) -> list[dict[str, Any]]:
    actions = [
        {
            "action": "copy",
            "path": str(dest_path),
            "needed": True,
        },
        {
            "action": "append-source",
            "line": source_line,
            "needed": not already_sourced,
        },
    ]
    return actions


def rollback_kconfig_report(
    kernel_tree: str | Path,
    destination: str = DEFAULT_DESTINATION,
    apply: bool = False,
) -> dict[str, Any]:
    tree = Path(kernel_tree).expanduser().resolve()
    if not tree.exists():
        raise FileNotFoundError(f"kernel tree not found: {tree}")
    root_kconfig = tree / "Kconfig"
    if not root_kconfig.exists():
        raise FileNotFoundError(f"target root Kconfig not found: {root_kconfig}")

    dest_rel = validate_destination(destination)
    dest_path = tree / dest_rel
    source_line = f'source "{dest_rel.as_posix()}"'
    root_text = root_kconfig.read_text(errors="replace")
    source_present = any(line.strip() == source_line for line in root_text.splitlines())
    destination_exists = dest_path.exists()
    actions = [
        {"action": "remove-source", "line": source_line, "needed": source_present},
        {"action": "remove-file", "path": str(dest_path), "needed": destination_exists},
    ]

    if apply:
        if source_present:
            kept = [line for line in root_text.splitlines() if line.strip() != source_line]
            root_kconfig.write_text("\n".join(kept).rstrip() + "\n")
        if destination_exists:
            dest_path.unlink()

    return {
        "kernel_tree": str(tree),
        "mode": "apply" if apply else "dry-run",
        "target_kconfig": str(root_kconfig),
        "destination": dest_rel.as_posix(),
        "source_line": source_line,
        "source_present": source_present,
        "destination_exists": destination_exists,
        "actions": actions,
        "applied": apply,
    }


def format_kconfig_inject_report(report: dict[str, Any]) -> str:
    lines = [
        f"Kconfig injection: {report['mode']}",
        f"kernel_tree: {report['kernel_tree']}",
        f"destination: {report['destination']}",
        f"source_line: {report['source_line']}",
        f"already_sourced: {report['already_sourced']}",
        "actions:",
    ]
    for action in report["actions"]:
        status = "needed" if action["needed"] else "already satisfied"
        if action["action"] == "copy":
            lines.append(f"  - copy {report['project_kconfig']} -> {action['path']} ({status})")
        else:
            lines.append(f"  - append {action['line']} to {report['target_kconfig']} ({status})")
    return "\n".join(lines)


def format_kconfig_rollback_report(report: dict[str, Any]) -> str:
    lines = [
        f"Kconfig rollback: {report['mode']}",
        f"kernel_tree: {report['kernel_tree']}",
        f"destination: {report['destination']}",
        f"source_line: {report['source_line']}",
        "actions:",
    ]
    for action in report["actions"]:
        status = "needed" if action["needed"] else "already absent"
        if action["action"] == "remove-source":
            lines.append(f"  - remove {action['line']} from {report['target_kconfig']} ({status})")
        else:
            lines.append(f"  - remove {action['path']} ({status})")
    return "\n".join(lines)
