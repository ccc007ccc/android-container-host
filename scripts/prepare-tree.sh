#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
KERNEL_TREE=""
OUT_DIR=""
DEVICE=""
PROFILE=""
KERNEL_VERSION=""
APPLY=0

usage() {
    cat <<'EOF'
Usage: prepare-tree.sh --kernel-tree /path/to/kernel [--out /path/to/out] [--device FILE] [--profile PROFILE] [--kernel-version linux-4.19] [--apply]

Default mode is dry-run. --apply copies ACHKL Kconfig into the target tree and applies selected ready patches after dry-run checks pass.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --kernel-tree)
            KERNEL_TREE="$2"
            shift 2
            ;;
        --out)
            OUT_DIR="$2"
            shift 2
            ;;
        --device)
            DEVICE="$2"
            shift 2
            ;;
        --profile)
            PROFILE="$2"
            shift 2
            ;;
        --kernel-version)
            KERNEL_VERSION="$2"
            shift 2
            ;;
        --apply)
            APPLY=1
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

run_achost() {
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 -m achost.cli "$@"
}

PLAN_ARGS=(plan --kernel-tree "$KERNEL_TREE")
if [[ -n "$OUT_DIR" ]]; then
    PLAN_ARGS+=(--out "$OUT_DIR")
fi
if [[ -n "$DEVICE" ]]; then
    PLAN_ARGS+=(--device "$DEVICE")
fi
if [[ -n "$PROFILE" ]]; then
    PLAN_ARGS+=(--profile "$PROFILE")
fi

printf '\n## ACHKL plan\n'
set +e
run_achost "${PLAN_ARGS[@]}"
PLAN_RC=$?
set -e
if [[ "$PLAN_RC" -gt 1 ]]; then
    exit "$PLAN_RC"
fi
if [[ "$PLAN_RC" -eq 1 ]]; then
    printf 'plan reported required config gaps; continuing with source dry-run checks\n'
fi

printf '\n## Kconfig injection\n'
if [[ "$APPLY" -eq 1 ]]; then
    run_achost inject-kconfig --kernel-tree "$KERNEL_TREE" --apply
else
    run_achost inject-kconfig --kernel-tree "$KERNEL_TREE"
fi

PATCH_ARGS=(apply-patches --kernel-tree "$KERNEL_TREE")
if [[ -n "$KERNEL_VERSION" ]]; then
    PATCH_ARGS+=(--kernel-version "$KERNEL_VERSION")
fi
if [[ "$APPLY" -eq 1 ]]; then
    PATCH_ARGS+=(--apply)
else
    PATCH_ARGS+=(--dry-run)
fi

printf '\n## Patch validation\n'
run_achost "${PATCH_ARGS[@]}"
