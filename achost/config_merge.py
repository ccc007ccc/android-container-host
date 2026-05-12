from __future__ import annotations

from pathlib import Path
from typing import Any

from .config_rules import fragment_paths

PROJECT_ROOT = Path(__file__).resolve().parents[1]


def merge_profile_fragments(
    base_config: str | Path,
    output: str | Path,
    profile: str = "android-container-host-v1",
    extra_fragments: list[str] | tuple[str, ...] | None = None,
    project_root: Path = PROJECT_ROOT,
) -> dict[str, Any]:
    fragments = fragment_paths(project_root, profile)
    fragments.extend(Path(item).expanduser().resolve() for item in (extra_fragments or ()))
    return merge_config_files(base_config, output, fragments, profile=profile, project_root=project_root)


def merge_config_files(
    base_config: str | Path,
    output: str | Path,
    fragments: list[Path] | tuple[Path, ...],
    profile: str | None = None,
    project_root: Path = PROJECT_ROOT,
) -> dict[str, Any]:
    base = Path(base_config).expanduser().resolve()
    out = Path(output).expanduser().resolve()
    if not base.exists():
        raise FileNotFoundError(f"base config not found: {base}")

    resolved_fragments = [Path(item).expanduser().resolve() for item in fragments]
    for fragment in resolved_fragments:
        if not fragment.exists():
            raise FileNotFoundError(f"config fragment not found: {fragment}")

    values, order = read_config_values(base)
    changes: list[dict[str, str]] = []
    for fragment in resolved_fragments:
        fragment_values, fragment_order = read_config_values(fragment)
        for symbol in fragment_order:
            before = values.get(symbol, "missing")
            after = fragment_values[symbol]
            if symbol not in values:
                order.append(symbol)
            values[symbol] = after
            if before != after:
                changes.append(
                    {
                        "symbol": symbol,
                        "before": before,
                        "after": after,
                        "fragment": path_label(fragment, project_root),
                    }
                )

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(render_config(values, order))
    return {
        "profile": profile,
        "base_config": str(base),
        "output": str(out),
        "fragments": [path_label(fragment, project_root) for fragment in resolved_fragments],
        "summary": {
            "symbols": len(order),
            "fragments": len(resolved_fragments),
            "changes": len(changes),
            "added": sum(1 for item in changes if item["before"] == "missing"),
        },
        "changes": changes,
    }


def read_config_values(path: Path) -> tuple[dict[str, str], list[str]]:
    values: dict[str, str] = {}
    order: list[str] = []
    for raw_line in path.read_text(errors="replace").splitlines():
        parsed = parse_config_line(raw_line)
        if parsed is None:
            continue
        symbol, value = parsed
        if symbol not in values:
            order.append(symbol)
        values[symbol] = value
    return values, order


def parse_config_line(line: str) -> tuple[str, str] | None:
    stripped = line.strip()
    if stripped.startswith("CONFIG_") and "=" in stripped:
        symbol, value = stripped.split("=", 1)
        return symbol, value
    if stripped.startswith("# CONFIG_") and stripped.endswith(" is not set"):
        symbol = stripped[2:].split(" is not set", 1)[0]
        return symbol, "not set"
    return None


def render_config(values: dict[str, str], order: list[str]) -> str:
    lines: list[str] = []
    for symbol in order:
        value = values[symbol]
        if value == "not set":
            lines.append(f"# {symbol} is not set")
        else:
            lines.append(f"{symbol}={value}")
    return "\n".join(lines) + "\n"


def path_label(path: Path, project_root: Path) -> str:
    try:
        return str(path.resolve().relative_to(project_root.resolve()))
    except ValueError:
        return str(path)


def format_merge_report(report: dict[str, Any]) -> str:
    lines = [
        f"Merged config: {report['output']}",
        f"base_config: {report['base_config']}",
    ]
    if report.get("profile"):
        lines.append(f"profile: {report['profile']}")
    lines.append("fragments:")
    lines.extend(f"  - {fragment}" for fragment in report["fragments"])
    lines.append(
        "summary: "
        f"symbols={report['summary']['symbols']} "
        f"changes={report['summary']['changes']} "
        f"added={report['summary']['added']}"
    )
    if report["changes"]:
        lines.append("changes:")
        for item in report["changes"]:
            lines.append(f"  - {item['symbol']}: {item['before']} -> {item['after']} ({item['fragment']})")
    return "\n".join(lines)
