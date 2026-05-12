from __future__ import annotations

from pathlib import Path
from typing import Any


def load_device_profile(path: str | Path) -> dict[str, Any]:
    profile_path = Path(path).expanduser().resolve()
    if not profile_path.exists():
        raise FileNotFoundError(f"device profile not found: {profile_path}")

    data: dict[str, Any] = {}
    current_section: str | None = None
    current_list_key: str | None = None

    for raw_line in profile_path.read_text(errors="replace").splitlines():
        if not raw_line.strip() or raw_line.lstrip().startswith("#"):
            continue
        indent = len(raw_line) - len(raw_line.lstrip(" "))
        line = raw_line.strip()

        if indent == 0:
            key, value = split_key_value(line)
            current_section = key
            current_list_key = None
            data[key] = {} if value is None else parse_scalar(value)
            continue

        if current_section is None:
            continue

        if indent == 2 and line.startswith("- "):
            if not isinstance(data.get(current_section), list):
                data[current_section] = []
            data[current_section].append(parse_scalar(line[2:].strip()))
            continue

        if indent == 2:
            key, value = split_key_value(line)
            section = data.setdefault(current_section, {})
            if not isinstance(section, dict):
                continue
            if value is None:
                section[key] = []
                current_list_key = key
            else:
                section[key] = parse_scalar(value)
                current_list_key = None
            continue

        if indent == 4 and line.startswith("- ") and current_list_key:
            section = data.get(current_section)
            if isinstance(section, dict) and isinstance(section.get(current_list_key), list):
                section[current_list_key].append(parse_scalar(line[2:].strip()))

    return data


def split_key_value(line: str) -> tuple[str, str | None]:
    if ":" not in line:
        raise ValueError(f"invalid device profile line: {line}")
    key, value = line.split(":", 1)
    key = key.strip()
    value = value.strip()
    return key, value if value else None


def parse_scalar(value: str) -> Any:
    if value in ("true", "True"):
        return True
    if value in ("false", "False"):
        return False
    if (value.startswith("'") and value.endswith("'")) or (value.startswith('"') and value.endswith('"')):
        return value[1:-1]
    return value


def device_default_defconfig(profile: dict[str, Any] | None) -> str | None:
    if not profile:
        return None
    kernel = profile.get("kernel", {})
    return kernel.get("defconfig") if isinstance(kernel, dict) else None


def device_default_profiles(profile: dict[str, Any] | None) -> str | None:
    if not profile:
        return None
    container = profile.get("container", {})
    if not isinstance(container, dict):
        return None
    profiles = container.get("profiles")
    if isinstance(profiles, list):
        return ",".join(str(item) for item in profiles)
    if isinstance(profiles, str):
        return profiles
    return None
