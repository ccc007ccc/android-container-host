#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="android-container-host-v1"
JSON=0
CONFIG=""

usage() {
    printf 'Usage: %s [--profile PROFILE] [--json] /path/to/.config\n' "${0##*/}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --profile)
            PROFILE="$2"
            shift 2
            ;;
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

CMD=(python3 -m achost.cli verify-config --config "$CONFIG" --profile "$PROFILE")
if [[ "$JSON" -eq 1 ]]; then
    CMD+=(--json)
fi

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" "${CMD[@]}"
