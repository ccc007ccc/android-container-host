from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .kernel_detect import detect_kernel


@dataclass(frozen=True)
class PatchModule:
    name: str
    kernel_family: str
    patch_path: Path
    doc_path: Path
    status: str
    default_enabled: bool
    risk: str
    affected_files: tuple[str, ...]
    summary: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "kernel_family": self.kernel_family,
            "patch": str(self.patch_path),
            "doc": str(self.doc_path),
            "status": self.status,
            "default_enabled": self.default_enabled,
            "risk": self.risk,
            "affected_files": list(self.affected_files),
            "summary": self.summary,
        }


PROJECT_ROOT = Path(__file__).resolve().parents[1]
PATCH_ROOT = PROJECT_ROOT / "patches"
READY_STATUS = "ready"


def kernel_family_from_version(version: str) -> str:
    parts = version.split(".")
    if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
        return f"linux-{parts[0]}.{parts[1]}"
    if version.startswith("linux-"):
        return version
    return "unknown"


def resolve_kernel_family(kernel_tree: str | Path, kernel_version: str | None = None) -> str:
    if kernel_version:
        return kernel_version if kernel_version.startswith("linux-") else kernel_family_from_version(kernel_version)
    detected = detect_kernel(kernel_tree)
    return kernel_family_from_version(detected["kernel_version"])


def discover_patches(kernel_family: str, patch_root: str | Path = PATCH_ROOT) -> list[PatchModule]:
    family_dir = Path(patch_root) / kernel_family
    if not family_dir.exists():
        return []

    modules: list[PatchModule] = []
    for patch_path in sorted(family_dir.glob("*.patch")):
        doc_path = patch_path.with_suffix(".md")
        metadata = parse_patch_doc(doc_path) if doc_path.exists() else {}
        name = metadata.get("name", patch_path.stem)
        modules.append(
            PatchModule(
                name=name,
                kernel_family=metadata.get("kernel_family", kernel_family),
                patch_path=patch_path,
                doc_path=doc_path,
                status=metadata.get("status", "undocumented"),
                default_enabled=_parse_bool(metadata.get("default_enabled", "false")),
                risk=metadata.get("risk", "unknown"),
                affected_files=tuple(_parse_list(metadata.get("affected_files", ""))),
                summary=metadata.get("summary", ""),
            )
        )
    return modules


def list_patch_report(kernel_tree: str | Path, kernel_version: str | None = None) -> dict[str, Any]:
    family = resolve_kernel_family(kernel_tree, kernel_version)
    modules = discover_patches(family)
    return {
        "kernel_tree": str(Path(kernel_tree).expanduser().resolve()),
        "kernel_family": family,
        "patches": [module.to_dict() | {"skip_reason": skip_reason(module)} for module in modules],
    }


def apply_patch_report(
    kernel_tree: str | Path,
    kernel_version: str | None = None,
    patch_names: list[str] | None = None,
    apply: bool = False,
) -> dict[str, Any]:
    tree = Path(kernel_tree).expanduser().resolve()
    family = resolve_kernel_family(tree, kernel_version)
    modules = discover_patches(family)
    selected, selection_errors = select_patches(modules, patch_names)

    results: list[dict[str, Any]] = []
    for module in selected:
        reason = skip_reason(module, explicit=bool(patch_names))
        if reason:
            results.append(
                {
                    **module.to_dict(),
                    "selected": True,
                    "skipped": True,
                    "skip_reason": reason,
                    "check": None,
                    "apply": None,
                }
            )
            continue

        check = run_git_apply(tree, module.patch_path, check=True)
        apply_result = None
        if apply and check["ok"]:
            apply_result = run_git_apply(tree, module.patch_path, check=False)

        results.append(
            {
                **module.to_dict(),
                "selected": True,
                "skipped": False,
                "skip_reason": None,
                "check": check,
                "apply": apply_result,
            }
        )

    ok = not selection_errors and all(
        item.get("skipped") is False and item["check"]["ok"] and (not apply or item["apply"]["ok"])
        for item in results
    )

    return {
        "kernel_tree": str(tree),
        "kernel_family": family,
        "mode": "apply" if apply else "dry-run",
        "selection_errors": selection_errors,
        "results": results,
        "ok": ok,
    }


def select_patches(modules: list[PatchModule], patch_names: list[str] | None = None) -> tuple[list[PatchModule], list[str]]:
    by_name = {module.name: module for module in modules}
    if patch_names:
        selected: list[PatchModule] = []
        errors: list[str] = []
        for name in patch_names:
            module = by_name.get(name)
            if module is None:
                errors.append(f"unknown patch: {name}")
            else:
                selected.append(module)
        return selected, errors

    return [module for module in modules if module.status == READY_STATUS and module.default_enabled], []


def skip_reason(module: PatchModule, explicit: bool = False) -> str | None:
    if not module.doc_path.exists():
        return "missing patch documentation"
    if module.status != READY_STATUS:
        return f"status is {module.status}; not runnable by default"
    if not module.default_enabled and not explicit:
        return "default_enabled is false"
    return None


def run_git_apply(kernel_tree: Path, patch_path: Path, check: bool) -> dict[str, Any]:
    command = ["git", "-C", str(kernel_tree), "apply"]
    if check:
        command.append("--check")
    command.append(str(patch_path))

    proc = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    return {
        "ok": proc.returncode == 0,
        "exit_code": proc.returncode,
        "stdout": proc.stdout.strip(),
        "stderr": proc.stderr.strip(),
        "command": command,
    }


def parse_patch_doc(doc_path: str | Path) -> dict[str, str]:
    path = Path(doc_path)
    if not path.exists():
        return {}

    lines = path.read_text(errors="replace").splitlines()
    if not lines or lines[0].strip() != "---":
        return {}

    metadata: dict[str, str] = {}
    for line in lines[1:]:
        if line.strip() == "---":
            break
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        metadata[key.strip()] = value.strip().strip('"\'')
    return metadata


def format_patch_list(report: dict[str, Any]) -> str:
    lines = [f"Patch modules for {report['kernel_family']}:"]
    if not report["patches"]:
        lines.append("- none")
        return "\n".join(lines)

    for patch in report["patches"]:
        suffix = f" ({patch['skip_reason']})" if patch.get("skip_reason") else ""
        lines.append(
            f"- {patch['name']}: status={patch['status']} default={patch['default_enabled']} risk={patch['risk']}{suffix}"
        )
    return "\n".join(lines)


def format_apply_report(report: dict[str, Any]) -> str:
    lines = [f"Patch {report['mode']} report for {report['kernel_family']}:"]
    for error in report["selection_errors"]:
        lines.append(f"[ERROR] {error}")
    if not report["results"]:
        lines.append("- no patches selected")
        return "\n".join(lines)

    for item in report["results"]:
        if item["skipped"]:
            lines.append(f"[SKIP] {item['name']}: {item['skip_reason']}")
            continue
        check = item["check"]
        prefix = "OK" if check["ok"] else "FAIL"
        lines.append(f"[{prefix}] {item['name']}: git apply --check exit={check['exit_code']}")
        if check["stderr"]:
            lines.append(check["stderr"])
        if item.get("apply") is not None:
            apply_result = item["apply"]
            prefix = "OK" if apply_result["ok"] else "FAIL"
            lines.append(f"[{prefix}] {item['name']}: git apply exit={apply_result['exit_code']}")
            if apply_result["stderr"]:
                lines.append(apply_result["stderr"])
    return "\n".join(lines)


def _parse_bool(value: str) -> bool:
    return value.lower() in {"1", "true", "yes", "y"}


def _parse_list(value: str) -> list[str]:
    return [item.strip() for item in value.split(",") if item.strip()]
