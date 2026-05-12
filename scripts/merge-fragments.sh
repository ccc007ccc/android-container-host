#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="android-container-host-v1"
JSON=0
BASE_CONFIG=""
OUTPUT=""
EXTRA_ARGS=()

usage() {
    printf 'Usage: %s --base-config /path/.config --output /path/merged.config [--profile PROFILE] [--fragment FILE] [--json]\n' "${0##*/}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --base-config)
            BASE_CONFIG="$2"
            shift 2
            ;;
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --profile)
            PROFILE="$2"
            shift 2
            ;;
        --fragment)
            EXTRA_ARGS+=(--fragment "$2")
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
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -z "$BASE_CONFIG" || -z "$OUTPUT" ]]; then
    usage >&2
    exit 2
fi

CMD=(python3 -m achost.cli merge-fragments --base-config "$BASE_CONFIG" --output "$OUTPUT" --profile "$PROFILE")
if [[ ${#EXTRA_ARGS[@]} -gt 0 ]]; then
    CMD+=("${EXTRA_ARGS[@]}")
fi
if [[ "$JSON" -eq 1 ]]; then
    CMD+=(--json)
fi

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" "${CMD[@]}"
