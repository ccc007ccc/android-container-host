#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    cat <<'EOF'
Usage: apply-patches.sh --kernel-tree /path/to/kernel [--dry-run|--apply] [--patch NAME] [--json]

Default mode is dry-run. --apply only runs after the same git apply --check validation.
EOF
}

ARGS=()
if [[ $# -eq 0 ]]; then
    usage >&2
    exit 2
fi

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        *)
            ARGS+=("$1")
            shift
            ;;
    esac
done

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" \
    python3 -m achost.cli apply-patches "${ARGS[@]}"
