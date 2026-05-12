#!/system/bin/sh
set -u

CONTAINER_NAME="${CONTAINER_NAME:-achost-alpine}"
DISTRO="${DISTRO:-alpine}"
RELEASE="${RELEASE:-edge}"
ARCH="${ARCH:-arm64}"
PING_TARGET="${PING_TARGET:-1.1.1.1}"
OUT_DIR="${OUT_DIR:-}"
FAILURES=0

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
    lxc-stop -n "$CONTAINER_NAME" >/dev/null 2>&1 || true
}

command -v lxc-start >/dev/null 2>&1 || {
    printf 'lxc tools not found\n' >&2
    exit 2
}

trap cleanup EXIT INT TERM

if command -v lxc-checkconfig >/dev/null 2>&1; then
    run_required "lxc-checkconfig" lxc-checkconfig
else
    run_optional "lxc-checkconfig unavailable" sh -c 'echo lxc-checkconfig command not found'
fi

if lxc-info -n "$CONTAINER_NAME" >/dev/null 2>&1; then
    run_optional "existing container" lxc-info -n "$CONTAINER_NAME"
else
    if command -v lxc-create >/dev/null 2>&1; then
        run_required "lxc-create download" lxc-create -n "$CONTAINER_NAME" -t download -- -d "$DISTRO" -r "$RELEASE" -a "$ARCH"
    else
        printf 'lxc-create not found and container %s does not exist\n' "$CONTAINER_NAME" >&2
        exit 2
    fi
fi

run_required "lxc-start" lxc-start -n "$CONTAINER_NAME" -d
run_required "lxc-info" lxc-info -n "$CONTAINER_NAME"
run_required "lxc uname" lxc-attach -n "$CONTAINER_NAME" -- uname -a
run_required "lxc ip addr" lxc-attach -n "$CONTAINER_NAME" -- ip addr
run_required "lxc ping" lxc-attach -n "$CONTAINER_NAME" -- ping -c 3 "$PING_TARGET"
run_required "lxc-stop" lxc-stop -n "$CONTAINER_NAME"
run_optional "recent dmesg" sh -c 'dmesg 2>/dev/null | tail -200'
run_optional "recent kernel logcat" sh -c 'logcat -b kernel -d 2>/dev/null | tail -200'

if [ -n "$OUT_DIR" ]; then
    printf 'LXC smoke log written to %s\n' "$LOG_FILE" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    printf 'LXC smoke failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'LXC smoke passed\n'
