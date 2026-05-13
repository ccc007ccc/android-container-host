#!/system/bin/sh
set -u

MODE="${MODE:-all}"
STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo now)"
OUT_DIR="${OUT_DIR:-/data/local/tmp/achost-runtime-test-$STAMP}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
    ACHOST_BIN="$SCRIPT_DIR"
    . "$SCRIPT_DIR/achost-container-env.sh"
fi
FAILURES=0

section() {
    printf '\n## %s\n' "$1"
}

run_script() {
    title="$1"
    script="$2"
    required="$3"

    section "$title"
    if [ ! -x "$script" ]; then
        printf 'missing script: %s\n' "$script"
        if [ "$required" = "1" ]; then
            FAILURES=$((FAILURES + 1))
        fi
        return 0
    fi

    OUT_DIR="$OUT_DIR" "$script" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        printf 'FAIL: %s exit=%s\n' "$title" "$rc"
        if [ "$required" = "1" ]; then
            FAILURES=$((FAILURES + 1))
        fi
    else
        printf 'OK: %s\n' "$title"
    fi
}

check_network_watchdog() {
    log_file="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}"
    pid_file="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
    section "network watchdog status"
    if [ -r "$pid_file" ]; then
        pid="$(cat "$pid_file" 2>/dev/null || true)"
        printf 'pid=%s\n' "$pid"
        if kill -0 "$pid" 2>/dev/null; then
            printf 'running=1\n'
        else
            printf 'running=0\n'
        fi
    else
        printf 'pid file not found: %s\n' "$pid_file"
    fi
    { ps -A 2>/dev/null || ps 2>/dev/null; } | grep '[c]ontainer-network-watchdog' || true
    if [ -r "$log_file" ]; then
        tail -n 40 "$log_file" 2>/dev/null || true
    else
        printf 'watchdog log not found: %s\n' "$log_file"
    fi
}

case "$MODE" in
    all|network|docker|lxc) ;;
    *)
        printf 'unsupported MODE: %s\n' "$MODE" >&2
        exit 2
        ;;
esac

mkdir -p "$OUT_DIR"
printf 'runtime_test_mode=%s\n' "$MODE"
printf 'runtime_test_out_dir=%s\n' "$OUT_DIR"

check_network_watchdog
run_script "container userland validation" "$SCRIPT_DIR/achost-container-validate.sh" 0

if [ "$MODE" = "all" ] || [ "$MODE" = "network" ]; then
    run_script "network debug before tests" "$SCRIPT_DIR/runtime-net-debug.sh" 0
fi

if [ "$MODE" = "all" ] || [ "$MODE" = "docker" ]; then
    run_script "protect container daemons" "$SCRIPT_DIR/protect-container-daemons.sh" 0
    run_script "Docker daemon start" "$SCRIPT_DIR/achost-docker-start.sh" 1
    run_script "container network reconcile" "$SCRIPT_DIR/container-nat-manager.sh" 0
    run_script "Docker runtime smoke" "$SCRIPT_DIR/runtime-smoke-docker.sh" 1
    run_script "network debug after Docker" "$SCRIPT_DIR/runtime-net-debug.sh" 0
    run_script "Docker daemon stop" "$SCRIPT_DIR/achost-docker-stop.sh" 0
fi

if [ "$MODE" = "all" ] || [ "$MODE" = "lxc" ]; then
    run_script "LXC checkconfig" "$SCRIPT_DIR/verify-lxc-checkconfig.sh" 1
    run_script "LXC runtime smoke" "$SCRIPT_DIR/runtime-smoke-lxc.sh" 1
fi

run_script "collect runtime logs" "$SCRIPT_DIR/collect-logs.sh" 0

if [ "$FAILURES" -ne 0 ]; then
    printf 'runtime test failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'runtime test completed\n'
