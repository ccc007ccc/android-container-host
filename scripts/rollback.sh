#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_TREE=""
DESTINATION="vendor/android-container-host/AndroidContainerHost.Kconfig"
APPLY=0
JSON=0

usage() {
    cat <<'EOF'
Usage: rollback.sh --kernel-tree /path/to/kernel [--destination PATH] [--apply] [--json]

Default mode is dry-run. --apply removes the ACHKL Kconfig source line and injected Kconfig file.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --kernel-tree)
            KERNEL_TREE="$2"
            shift 2
            ;;
        --destination)
            DESTINATION="$2"
            shift 2
            ;;
        --apply)
            APPLY=1
            shift
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

if [[ -z "$KERNEL_TREE" ]]; then
    usage >&2
    exit 2
fi

CMD=(python3 -m achost.cli rollback-kconfig --kernel-tree "$KERNEL_TREE" --destination "$DESTINATION")
if [[ "$APPLY" -eq 1 ]]; then
    CMD+=(--apply)
fi
if [[ "$JSON" -eq 1 ]]; then
    CMD+=(--json)
fi

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" "${CMD[@]}"
