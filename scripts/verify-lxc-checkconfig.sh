#!/system/bin/sh
set -u

OUT_DIR="${OUT_DIR:-}"
FAILURES=0

if [ -n "$OUT_DIR" ]; then
    mkdir -p "$OUT_DIR"
    LOG_FILE="$OUT_DIR/verify-lxc-checkconfig.txt"
    exec >"$LOG_FILE" 2>&1
fi

section() {
    printf '\n## %s\n' "$1"
}

pass() {
    printf 'OK: %s\n' "$1"
}

fail() {
    printf 'FAIL: %s\n' "$1"
    FAILURES=$((FAILURES + 1))
}

check_path() {
    title="$1"
    path="$2"
    if [ -e "$path" ]; then
        pass "$title"
    else
        fail "$title missing: $path"
    fi
}

run_optional() {
    title="$1"
    shift
    section "$title"
    "$@" 2>&1 || true
}

section "lxc-checkconfig"
if command -v lxc-checkconfig >/dev/null 2>&1; then
    lxc-checkconfig 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        fail "lxc-checkconfig exit=$rc"
    else
        pass "lxc-checkconfig"
    fi
else
    printf 'lxc-checkconfig command not found; running fallback checks\n'
fi

section "namespace files"
check_path "mnt namespace" /proc/self/ns/mnt
check_path "uts namespace" /proc/self/ns/uts
check_path "ipc namespace" /proc/self/ns/ipc
check_path "pid namespace" /proc/self/ns/pid
check_path "net namespace" /proc/self/ns/net
if [ -e /proc/self/ns/user ]; then
    pass "user namespace"
else
    printf 'WARN: user namespace missing\n'
fi

section "cgroups"
check_path "proc cgroups" /proc/cgroups
if grep -q cgroup /proc/mounts 2>/dev/null; then
    pass "cgroup mounts"
else
    fail "no cgroup mount found in /proc/mounts"
fi
run_optional "proc cgroups" cat /proc/cgroups
run_optional "cgroup mounts" sh -c 'grep cgroup /proc/mounts 2>/dev/null'

section "devices and tools"
check_path "devpts" /dev/pts
if command -v ip >/dev/null 2>&1; then
    pass "ip command"
else
    fail "ip command missing"
fi
if command -v lxc-start >/dev/null 2>&1; then
    pass "lxc-start command"
else
    printf 'WARN: lxc-start command missing\n'
fi

if [ -n "$OUT_DIR" ]; then
    printf 'LXC check log written to %s\n' "$LOG_FILE" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    printf 'LXC check failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'LXC check passed\n'
