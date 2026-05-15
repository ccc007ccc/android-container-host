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
COMMON_BIN="${ACHOST_COMMON_BIN:-$SCRIPT_DIR}"
if [ -x "$SCRIPT_DIR/achost-docker-runtime" ] || [ -x "$SCRIPT_DIR/runtime-smoke-docker.sh" ]; then
    DOCKER_ROOT="$ACHOST"
    DOCKER_BIN="$SCRIPT_DIR"
else
    DOCKER_ROOT="${ACHOST_DOCKER_MODULE:-$ACHOST}"
    DOCKER_BIN="$DOCKER_ROOT/bin"
fi
if [ -x "$SCRIPT_DIR/achost-lxc-runtime" ] || [ -x "$SCRIPT_DIR/runtime-smoke-lxc.sh" ]; then
    LXC_ROOT="$ACHOST"
    LXC_BIN="$SCRIPT_DIR"
else
    LXC_ROOT="${ACHOST_LXC_MODULE:-$ACHOST}"
    LXC_BIN="$LXC_ROOT/bin"
fi

section() {
    printf '\n## %s\n' "$1"
}

run_script() {
    title="$1"
    script="$2"
    required="$3"
    shift 3

    section "$title"
    if [ ! -x "$script" ]; then
        printf 'missing executable: %s\n' "$script"
        if [ "$required" = "1" ]; then
            FAILURES=$((FAILURES + 1))
        fi
        return 0
    fi

    OUT_DIR="$OUT_DIR" "$script" "$@" 2>&1
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
    { ps -A -o PID,ARGS 2>/dev/null || ps -A 2>/dev/null || ps 2>/dev/null; } | grep -E '[a]chost-runtime-core.*net-watchdog|[n]et-watchdog' || true
    if [ -r "$log_file" ]; then
        tail -n 40 "$log_file" 2>/dev/null || true
    else
        printf 'watchdog log not found: %s\n' "$log_file"
    fi
}

docker_daemon_ready() {
    [ -x "$DOCKER_BIN/docker" ] || return 1
    "$DOCKER_BIN/docker" --host "${DOCKER_HOST:-unix://${ACHOST_RUN:-/data/adb/achost/run}/docker.sock}" info >/dev/null 2>&1
}

docker_runtime_status() {
    printf 'docker_root=%s\n' "$DOCKER_ROOT"
    printf 'docker_bin=%s\n' "$DOCKER_BIN"
    printf 'docker_host=%s\n' "${DOCKER_HOST:-unix://${ACHOST_RUN:-/data/adb/achost/run}/docker.sock}"
    for file in "${ACHOST_DOCKERD_PID:-${ACHOST_RUN:-/data/adb/achost/run}/dockerd.pid}" "${ACHOST_CONTAINERD_PID:-${ACHOST_RUN:-/data/adb/achost/run}/containerd.pid}" "${ACHOST_SUPERVISOR_PID:-${ACHOST_RUN:-/data/adb/achost/run}/achost-supervise.pid}"; do
        if [ -r "$file" ]; then
            pid="$(cat "$file" 2>/dev/null || true)"
            if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
                printf 'pid_file=%s pid=%s running=1\n' "$file" "$pid"
            else
                printf 'pid_file=%s pid=%s running=0\n' "$file" "$pid"
            fi
        else
            printf 'pid_file=%s missing=1\n' "$file"
        fi
    done
    if docker_daemon_ready; then
        printf 'docker_api=ready\n'
        "$DOCKER_BIN/docker" --host "${DOCKER_HOST:-unix://${ACHOST_RUN:-/data/adb/achost/run}/docker.sock}" info --format 'CgroupVersion={{.CgroupVersion}} CgroupDriver={{.CgroupDriver}} Driver={{.Driver}}' 2>/dev/null || true
    else
        printf 'docker_api=not-ready\n'
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
    ACHOST="$DOCKER_ROOT"
    ACHOST_BIN="$DOCKER_BIN"
    ACHOST_ETC="$DOCKER_ROOT/etc"
    ACHOST_COMMON="${ACHOST_COMMON:-$ACHOST_BASE}"
    ACHOST_COMMON_BIN="$COMMON_BIN"
    DOCKER_CONFIG="$ACHOST_ETC/docker"
    DOCKER_CLI_PLUGIN_EXTRA_DIRS="$DOCKER_CONFIG/cli-plugins:$DOCKER_ROOT/libexec/docker/cli-plugins"
    ACHOST_CONTAINERD_CONFIG="$ACHOST_ETC/containerd/config.toml"
    PATH="$DOCKER_BIN:$COMMON_BIN:$PATH"
    export ACHOST ACHOST_BIN ACHOST_ETC ACHOST_COMMON ACHOST_COMMON_BIN DOCKER_CONFIG DOCKER_CLI_PLUGIN_EXTRA_DIRS ACHOST_CONTAINERD_CONFIG PATH
    section "Docker status before tests"
    docker_runtime_status
    DOCKER_WAS_RUNNING=0
    if docker_daemon_ready; then
        DOCKER_WAS_RUNNING=1
    fi
    run_script "protect container daemons" "$COMMON_BIN/achost-runtime-core" 0 protect-daemons
    run_script "Docker daemon start" "$DOCKER_BIN/achost-docker-runtime" 1 start
    run_script "container network reconcile" "$COMMON_BIN/achost-runtime-core" 0 net-reconcile
    run_script "Docker runtime smoke" "$DOCKER_BIN/runtime-smoke-docker.sh" 1
    run_script "Docker feature matrix" "$DOCKER_BIN/runtime-docker-feature-test.sh" 1
    run_script "network debug after Docker" "$COMMON_BIN/runtime-net-debug.sh" 0
    if [ "$DOCKER_WAS_RUNNING" = "1" ]; then
        section "Docker daemon restore"
        printf 'Docker was running before runtime-test; leaving it running.\n'
    else
        run_script "Docker daemon stop" "$DOCKER_BIN/achost-docker-runtime" 0 stop
    fi
    section "Docker status after tests"
    docker_runtime_status
fi

if [ "$MODE" = "all" ] || [ "$MODE" = "lxc" ]; then
    ACHOST="$LXC_ROOT"
    ACHOST_BIN="$LXC_BIN"
    ACHOST_ETC="$LXC_ROOT/etc"
    ACHOST_COMMON="${ACHOST_COMMON:-$ACHOST_BASE}"
    ACHOST_COMMON_BIN="$COMMON_BIN"
    ACHOST_MODULE_TARGET="lxc"
    ACHOST_LXC_MODULE="$LXC_ROOT"
    ACHOST_LXC_RUNTIME="$LXC_BIN/achost-lxc-runtime"
    ACHOST_LXC="$LXC_ROOT/lxc"
    ACHOST_LXC_BIN="$ACHOST_LXC/bin"
    ACHOST_LXC_ETC="$ACHOST_ETC/lxc"
    ACHOST_LXC_VAR="${ACHOST_LXC_VAR:-/data/adb/achost/lxc}"
    ACHOST_LXC_RUN="${ACHOST_LXC_RUN:-/data/adb/achost/run/lxc}"
    ACHOST_LXC_LOG="${ACHOST_LXC_LOG:-/data/adb/achost/log/lxc}"
    ACHOST_LXC_ROOTFS="${ACHOST_LXC_ROOTFS:-$ACHOST_LXC_VAR/rootfs}"
    ACHOST_LXC_CONTAINERS="${ACHOST_LXC_CONTAINERS:-$ACHOST_LXC_VAR/containers}"
    LXC_BRIDGE="${LXC_BRIDGE:-lxcbr0}"
    LXC_SUBNET="${LXC_SUBNET:-172.32.0.0/16}"
    CONTAINER_BRIDGE="$LXC_BRIDGE"
    PATH="$LXC_BIN:$ACHOST_LXC_BIN:$COMMON_BIN:$PATH"
    LD_LIBRARY_PATH="$ACHOST_LXC/lib:$LXC_ROOT/lib:${LD_LIBRARY_PATH:-}"
    export ACHOST ACHOST_BIN ACHOST_ETC ACHOST_COMMON ACHOST_COMMON_BIN ACHOST_MODULE_TARGET ACHOST_LXC_MODULE ACHOST_LXC_RUNTIME ACHOST_LXC ACHOST_LXC_BIN ACHOST_LXC_ETC ACHOST_LXC_VAR ACHOST_LXC_RUN ACHOST_LXC_LOG ACHOST_LXC_ROOTFS ACHOST_LXC_CONTAINERS LXC_BRIDGE LXC_SUBNET CONTAINER_BRIDGE PATH LD_LIBRARY_PATH
    run_script "LXC write configs" "$LXC_BIN/achost-lxc-runtime" 1 write-configs
    run_script "LXC host validation" "$LXC_BIN/achost-lxc-runtime" 1 validate-host
    run_script "LXC asset validation" "$LXC_BIN/achost-lxc-runtime" 1 validate-assets
    run_script "LXC prepare bridge" "$LXC_BIN/achost-lxc-runtime" 1 prepare-bridge
    run_script "LXC checkconfig" "$LXC_BIN/verify-lxc-checkconfig.sh" 0
    run_script "LXC runtime smoke" "$LXC_BIN/runtime-smoke-lxc.sh" 1
fi

run_script "collect runtime logs" "$SCRIPT_DIR/collect-logs.sh" 0

if [ "$FAILURES" -ne 0 ]; then
    printf 'runtime test failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'runtime test completed\n'
