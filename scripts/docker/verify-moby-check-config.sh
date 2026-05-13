#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_PATH="${MOBY_CHECK_CONFIG:-$PROJECT_ROOT/third_party/moby-check-config/check-config.sh}"
JSON=0
CONFIG=""

usage() {
    printf 'Usage: %s [--json] /path/to/.config\n' "${0##*/}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --json)
            JSON=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            if [[ -n "$CONFIG" ]]; then
                usage >&2
                exit 2
            fi
            CONFIG="$1"
            shift
            ;;
    esac
done

if [[ -z "$CONFIG" ]]; then
    usage >&2
    exit 2
fi

if [[ ! -f "$SCRIPT_PATH" ]]; then
    printf 'Moby check-config not found: %s\n' "$SCRIPT_PATH" >&2
    printf 'Run third_party/moby-check-config/fetch.sh or set MOBY_CHECK_CONFIG.\n' >&2
    exit 2
fi

if [[ "$JSON" -eq 1 ]]; then
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 -m achost.cli verify-moby-check-config --script "$SCRIPT_PATH" --config "$CONFIG" --json
else
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 -m achost.cli verify-moby-check-config --script "$SCRIPT_PATH" --config "$CONFIG"
fi
