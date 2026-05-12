from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def to_json(data: Any) -> str:
    return json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True)


def write_json(path: str | Path, data: Any) -> None:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(to_json(data) + "\n")


def plan_markdown(plan: dict[str, Any]) -> str:
    lines = [
        "# Android Container Host plan",
        "",
        f"- Kernel tree: `{plan['detect']['kernel_tree']}`",
        f"- Kernel version: `{plan['detect']['kernel_version']}`",
        f"- Arch: `{plan['detect']['arch']}`",
        f"- Profile: `{plan['profile']}`",
        "",
        "## Fragments",
        "",
    ]
    lines.extend(f"- `{fragment}`" for fragment in plan["fragments"])
    lines.extend(["", "## Config summary", ""])
    summary = plan["config_summary"]
    lines.extend(
        [
            f"- OK: {summary['ok']}",
            f"- Required failed: {summary['required_failed']}",
            f"- Recommended failed: {summary['recommended_failed']}",
            f"- Optional failed: {summary['optional_failed']}",
            "",
            "## Required gaps",
            "",
        ]
    )

    required_gaps = [item for item in plan["config_results"] if item["level"] == "required" and not item["ok"]]
    if required_gaps:
        lines.extend(
            f"- `{item['symbol']}` expected `{item['expected']}`, found `{item['actual']}`: {item['reason']}"
            for item in required_gaps
        )
    else:
        lines.append("- None")

    lines.extend(["", "## Risks", ""])
    if plan["risks"]:
        lines.extend(f"- {risk}" for risk in plan["risks"])
    else:
        lines.append("- None")

    return "\n".join(lines) + "\n"


def write_plan_reports(output_dir: str | Path, plan: dict[str, Any]) -> dict[str, str]:
    report_dir = Path(output_dir) / "achost"
    report_dir.mkdir(parents=True, exist_ok=True)
    json_path = report_dir / "plan.json"
    md_path = report_dir / "plan.md"
    write_json(json_path, plan)
    md_path.write_text(plan_markdown(plan))
    return {"json": str(json_path), "markdown": str(md_path)}
