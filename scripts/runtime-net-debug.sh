#!/system/bin/sh
set -u

OUT_DIR="${OUT_DIR:-}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
    ACHOST_BIN="$SCRIPT_DIR"
    . "$SCRIPT_DIR/achost-container-env.sh"
else
    ACHOST="${ACHOST:-/data/adb/achost}"
fi
ACHOST_COMMON="${ACHOST_COMMON:-${ACHOST_BASE:-$ACHOST}}"
ACHOST_COMMON_BIN="${ACHOST_COMMON_BIN:-$ACHOST_COMMON/bin}"
ACHOST_RUNTIME_CORE="${ACHOST_RUNTIME_CORE:-$ACHOST_COMMON_BIN/achost-runtime-core}"
DOCKER_BRIDGE="${DOCKER_BRIDGE:-${CONTAINER_BRIDGE:-docker0}}"
CONTAINER_BRIDGE="$DOCKER_BRIDGE"
ACHOST_NET_LOG="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}"
ACHOST_NET_PID="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"

if [ -n "$OUT_DIR" ]; then
    mkdir -p "$OUT_DIR"
    LOG_FILE="$OUT_DIR/runtime-net-debug.txt"
    exec >"$LOG_FILE" 2>&1
fi

section() {
    printf '\n## %s\n' "$1"
}

run_cmd() {
    section "$*"
    "$@" 2>&1 || true
}

run_shell() {
    title="$1"
    shift
    section "$title"
    sh -c "$*" 2>&1 || true
}

section "runtime-net-debug metadata"
date 2>/dev/null || true
uname -a 2>/dev/null || true
id 2>/dev/null || true

run_cmd ip addr
run_cmd ip route
run_cmd ip rule
run_cmd ip addr show "$DOCKER_BRIDGE"
run_shell "docker bridge IPv4" "ip -4 addr show '$DOCKER_BRIDGE' | awk '/ inet / {print \$2}'"
run_shell "default uplink" "ip route get 1.1.1.1"
if [ -x "$ACHOST_RUNTIME_CORE" ]; then
    run_cmd "$ACHOST_RUNTIME_CORE" detect-uplink 1.1.1.1
else
    run_shell "detected uplink" "ip route get 1.1.1.1"
fi
run_shell "network watchdog status" "if [ -r '$ACHOST_NET_PID' ]; then pid=\$(cat '$ACHOST_NET_PID' 2>/dev/null); printf 'pid=%s\\n' \"\$pid\"; if kill -0 \"\$pid\" 2>/dev/null; then printf 'running=1\\n'; else printf 'running=0\\n'; fi; else printf 'pid file not found: %s\\n' '$ACHOST_NET_PID'; fi; { ps -A 2>/dev/null || ps 2>/dev/null; } | grep -E '[a]chost-runtime-core.*net-watchdog|[n]et-watchdog' || true"
run_shell "network watchdog log" "if [ -r '$ACHOST_NET_LOG' ]; then tail -n 80 '$ACHOST_NET_LOG'; else printf 'watchdog log not found: %s\\n' '$ACHOST_NET_LOG'; fi"
run_shell "docker daemon hints" "{ ps -A 2>/dev/null || ps 2>/dev/null; } | grep -iE '[d]ockerd|[c]ontainerd|[r]unc|[d]ocker-proxy' || true; ls -l '$ACHOST/var/run/docker.sock' '$ACHOST/var/run/containerd.sock' /var/run/docker.sock /run/containerd/containerd.sock 2>/dev/null || true"
run_shell "achost userland validation" "if [ -x '$ACHOST/bin/achost-container-validate.sh' ]; then '$ACHOST/bin/achost-container-validate.sh'; else printf 'validation script not found: %s\\n' '$ACHOST/bin/achost-container-validate.sh'; fi"

if command -v iptables >/dev/null 2>&1; then
    run_cmd iptables -S
    run_cmd iptables -t nat -S
    run_cmd iptables -t mangle -S
    run_cmd iptables -L FORWARD -n -v
    run_cmd iptables -t nat -L POSTROUTING -n -v
else
    section "iptables"
    printf 'iptables command not found\n'
fi

if command -v ip6tables >/dev/null 2>&1; then
    run_cmd ip6tables -S
    run_cmd ip6tables -t nat -S
else
    section "ip6tables"
    printf 'ip6tables command not found\n'
fi

if command -v nft >/dev/null 2>&1; then
    run_cmd nft list ruleset
else
    section "nft"
    printf 'nft command not found\n'
fi

run_shell "ip_forward" "cat /proc/sys/net/ipv4/ip_forward 2>/dev/null"
run_shell "ipv6_forwarding" "cat /proc/sys/net/ipv6/conf/all/forwarding 2>/dev/null"
run_shell "cgroup mounts" "grep cgroup /proc/mounts 2>/dev/null"
run_shell "proc cgroups" "cat /proc/cgroups 2>/dev/null"

if command -v conntrack >/dev/null 2>&1; then
    run_cmd conntrack -L
else
    section "conntrack"
    printf 'conntrack command not found\n'
fi

if command -v docker >/dev/null 2>&1; then
    run_cmd docker info
    run_cmd docker network inspect bridge
else
    section "docker"
    printf 'docker command not found\n'
fi

if [ -n "$OUT_DIR" ]; then
    printf 'runtime net debug written to %s\n' "$LOG_FILE" >&2
fi
