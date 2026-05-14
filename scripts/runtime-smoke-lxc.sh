#!/system/bin/sh
set -u

STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo now)"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
    ACHOST_BIN="$SCRIPT_DIR"
    . "$SCRIPT_DIR/achost-container-env.sh"
fi

CONTAINER_NAME="${CONTAINER_NAME:-achost-lxc-smoke-$STAMP}"
ROOTFS_ASSET="${ROOTFS_ASSET:-${LXC_ROOTFS_ASSET:-${ACHOST_LXC_ROOTFS_ASSET:-}}}"
ROOTFS_SHA256="${ROOTFS_SHA256:-${LXC_ROOTFS_SHA256:-${ACHOST_LXC_ROOTFS_SHA256:-}}}"
DISTRO="${DISTRO:-unknown}"
RELEASE="${RELEASE:-unknown}"
ARCH="${ARCH:-arm64}"
OUT_DIR="${OUT_DIR:-}"
LXC_RUNTIME="${ACHOST_LXC_RUNTIME:-$SCRIPT_DIR/achost-lxc-runtime}"
FAILURES=0
IMPORTED=0

if [ -n "$OUT_DIR" ]; then
    mkdir -p "$OUT_DIR"
    LOG_FILE="$OUT_DIR/runtime-smoke-lxc.txt"
    exec >"$LOG_FILE" 2>&1
fi

section() {
    printf '\n## %s\n' "$1"
}

run_required() {
    title="$1"
    shift
    section "$title"
    "$@" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        printf 'FAIL: %s exit=%s\n' "$title" "$rc"
        FAILURES=$((FAILURES + 1))
    else
        printf 'OK: %s\n' "$title"
    fi
}

run_optional() {
    title="$1"
    shift
    section "$title"
    "$@" 2>&1 || true
}

cleanup() {
    if [ "$IMPORTED" = "1" ]; then
        "$LXC_RUNTIME" stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
        "$LXC_RUNTIME" destroy "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
}

if [ ! -x "$LXC_RUNTIME" ]; then
    printf 'missing executable: %s\n' "$LXC_RUNTIME" >&2
    exit 2
fi

trap cleanup EXIT INT TERM

run_required "LXC write configs" "$LXC_RUNTIME" write-configs
run_required "LXC host validation" "$LXC_RUNTIME" validate-host
run_required "LXC asset validation" "$LXC_RUNTIME" validate-assets
run_required "LXC prepare bridge" "$LXC_RUNTIME" prepare-bridge
run_required "LXC list" "$LXC_RUNTIME" list --json

if [ -z "$ROOTFS_ASSET" ]; then
    section "LXC container smoke"
    printf 'rootfs_asset=missing\n'
    printf 'skipped=1 reason=no ROOTFS_ASSET/LXC_ROOTFS_ASSET/ACHOST_LXC_ROOTFS_ASSET provided\n'
else
    if [ ! -r "$ROOTFS_ASSET" ]; then
        section "LXC rootfs asset"
        printf 'missing rootfs asset: %s\n' "$ROOTFS_ASSET" >&2
        FAILURES=$((FAILURES + 1))
    else
        if [ -n "$ROOTFS_SHA256" ]; then
            run_required "LXC import rootfs" "$LXC_RUNTIME" import-rootfs --name "$CONTAINER_NAME" --rootfs-asset "$ROOTFS_ASSET" --distro "$DISTRO" --release "$RELEASE" --arch "$ARCH" --sha256 "$ROOTFS_SHA256"
        else
            run_required "LXC import rootfs" "$LXC_RUNTIME" import-rootfs --name "$CONTAINER_NAME" --rootfs-asset "$ROOTFS_ASSET" --distro "$DISTRO" --release "$RELEASE" --arch "$ARCH"
        fi
        IMPORTED=1
        run_required "LXC start" "$LXC_RUNTIME" start "$CONTAINER_NAME"
        run_required "LXC status" "$LXC_RUNTIME" status "$CONTAINER_NAME" --json
        run_required "LXC smoke exec" "$LXC_RUNTIME" smoke "$CONTAINER_NAME"
        run_optional "LXC logs" "$LXC_RUNTIME" logs "$CONTAINER_NAME" --lines 200
        run_required "LXC stop" "$LXC_RUNTIME" stop "$CONTAINER_NAME"
        run_required "LXC destroy" "$LXC_RUNTIME" destroy "$CONTAINER_NAME"
        IMPORTED=0
    fi
fi

run_optional "recent dmesg" sh -c 'dmesg 2>/dev/null | tail -200'
run_optional "recent kernel logcat" sh -c 'logcat -b kernel -d 2>/dev/null | tail -200'

if [ -n "$OUT_DIR" ]; then
    printf 'LXC smoke log written to %s\n' "$LOG_FILE" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    printf 'LXC smoke failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'LXC smoke completed\n'
