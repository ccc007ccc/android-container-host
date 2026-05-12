from __future__ import annotations

from pathlib import Path
from typing import Any

from .config_rules import ConfigRule, get_profile_rules
from .kernel_detect import config_value, parse_config


def evaluate_config(config_path: str | Path, profile: str) -> list[dict[str, Any]]:
    path = Path(config_path).expanduser().resolve()
    config = parse_config(path)
    results: list[dict[str, Any]] = []

    for rule in get_profile_rules(profile):
        actual = config_value(config, rule.config_symbol)
        ok = actual == rule.expected
        results.append(_result(rule, actual, ok))

    return results


def has_required_failures(results: list[dict[str, Any]]) -> bool:
    return any(item["level"] == "required" and not item["ok"] for item in results)


def summarize_results(results: list[dict[str, Any]]) -> dict[str, int]:
    summary = {"ok": 0, "required_failed": 0, "recommended_failed": 0, "optional_failed": 0}
    for item in results:
        if item["ok"]:
            summary["ok"] += 1
        elif item["level"] == "required":
            summary["required_failed"] += 1
        elif item["level"] == "recommended":
            summary["recommended_failed"] += 1
        else:
            summary["optional_failed"] += 1
    return summary


def format_human(results: list[dict[str, Any]]) -> str:
    lines: list[str] = []
    for item in results:
        if item["ok"]:
            lines.append(f"[OK] {item['symbol']}={item['actual']}")
        elif item["level"] == "required":
            lines.append(_format_failure("FAIL", item))
        elif item["level"] == "recommended":
            lines.append(_format_failure("WARN", item))
        else:
            lines.append(_format_failure("INFO", item))
    return "\n".join(lines)


def _result(rule: ConfigRule, actual: str, ok: bool) -> dict[str, Any]:
    return {
        "symbol": rule.config_symbol,
        "expected": rule.expected,
        "actual": actual,
        "level": rule.level,
        "reason": rule.reason,
        "ok": ok,
    }


def _format_failure(kind: str, item: dict[str, Any]) -> str:
    return (
        f"[{kind}][{item['level']}] {item['symbol']} expected {item['expected']} "
        f"but found {item['actual']} - {item['reason']}"
    )
