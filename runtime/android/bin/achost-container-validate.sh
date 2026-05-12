#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
. "$SCRIPT_DIR/achost-container-env.sh"

section() {
    printf '\n## %s\n' "$1"
}

binary_status() {
    name="$1"
    required="$2"
    path="$ACHOST_BIN/$name"
    if [ -e "$path" ]; then
        if [ -x "$path" ]; then
            printf '%s=found path=%s executable=1 required=%s\n' "$name" "$path" "$required"
        else
            printf '%s=found path=%s executable=0 required=%s\n' "$name" "$path" "$required"
        fi
    elif command -v "$name" >/dev/null 2>&1; then
        printf '%s=found path=%s executable=1 required=%s\n' "$name" "$(command -v "$name")" "$required"
    else
        printf '%s=missing required=%s\n' "$name" "$required"
    fi
}

section "achost paths"
printf 'ACHOST=%s\n' "$ACHOST"
printf 'ACHOST_BIN=%s\n' "$ACHOST_BIN"
printf 'ACHOST_CHROOT=%s\n' "$ACHOST_CHROOT"
printf 'ACHOST_USE_CHROOT=%s\n' "$ACHOST_USE_CHROOT"
printf 'ACHOST_CGROUP_MODE=%s\n' "$ACHOST_CGROUP_MODE"
printf 'DOCKER_CONFIG=%s\n' "$DOCKER_CONFIG"
printf 'DOCKER_HOST=%s\n' "$DOCKER_HOST"
printf 'CONTAINERD_ADDRESS=%s\n' "$CONTAINERD_ADDRESS"
printf 'ACHOST_EXTERNAL_CONTAINERD=%s\n' "$ACHOST_EXTERNAL_CONTAINERD"
printf 'CONTAINER_BRIDGE=%s\n' "$CONTAINER_BRIDGE"

section "docker binaries"
for name in docker dockerd containerd containerd-shim-runc-v2 ctr runc; do
    binary_status "$name" 1
done
for name in containerd-shim docker-init docker-proxy containerd-stress; do
    binary_status "$name" 0
done

section "daemon supervisor"
binary_status achost-supervise 0
printf 'ACHOST_USE_SUPERVISOR=%s\n' "$ACHOST_USE_SUPERVISOR"
printf 'ACHOST_SUPERVISE=%s\n' "$ACHOST_SUPERVISE"
printf 'ACHOST_SUPERVISOR_PID=%s\n' "$ACHOST_SUPERVISOR_PID"
printf 'ACHOST_SUPERVISOR_SOCKET=%s\n' "$ACHOST_SUPERVISOR_SOCKET"
printf 'ACHOST_SUPERVISOR_LOG=%s\n' "$ACHOST_SUPERVISOR_LOG"
printf 'ACHOST_DOCKERD_LAUNCH_PID=%s\n' "$ACHOST_DOCKERD_LAUNCH_PID"

section "docker compose plugin"
compose_plugin="$DOCKER_CONFIG/cli-plugins/docker-compose"
if [ -x "$compose_plugin" ]; then
    printf 'docker_compose_plugin=found path=%s executable=1\n' "$compose_plugin"
else
    printf 'docker_compose_plugin=missing path=%s\n' "$compose_plugin"
fi
binary_status docker-compose 0

section "docker buildx plugin"
buildx_plugin="$DOCKER_CONFIG/cli-plugins/docker-buildx"
if [ -x "$buildx_plugin" ]; then
    printf 'docker_buildx_plugin=found path=%s executable=1\n' "$buildx_plugin"
else
    printf 'docker_buildx_plugin=missing path=%s\n' "$buildx_plugin"
fi
binary_status docker-buildx 0

section "buildkit binaries"
binary_status buildctl 0
binary_status buildkitd 0

section "lxc binaries"
if [ -x "$ACHOST_BIN/achost-lxc-validate.sh" ]; then
    "$ACHOST_BIN/achost-lxc-validate.sh"
else
    printf 'achost-lxc-validate.sh=missing\n'
fi

section "docker sockets"
case "$DOCKER_HOST" in
    unix://*) docker_socket="${DOCKER_HOST#unix://}" ;;
    *) docker_socket="" ;;
esac
if [ -n "$docker_socket" ]; then
    if [ -S "$docker_socket" ]; then
        printf 'docker_socket=present path=%s\n' "$docker_socket"
    else
        printf 'docker_socket=missing path=%s\n' "$docker_socket"
    fi
fi
if [ -S "$CONTAINERD_ADDRESS" ]; then
    printf 'containerd_socket=present path=%s\n' "$CONTAINERD_ADDRESS"
else
    printf 'containerd_socket=missing path=%s\n' "$CONTAINERD_ADDRESS"
fi

section "daemon processes"
{ ps -A 2>/dev/null || ps 2>/dev/null; } | grep -iE '[a]chost-supervis|[d]ockerd|[c]ontainerd|[r]unc|[d]ocker-proxy' || true

section "cgroup mounts"
grep cgroup /proc/mounts 2>/dev/null || true

section "proc cgroups"
cat /proc/cgroups 2>/dev/null || true

section "overlayfs"
grep -w overlay /proc/filesystems 2>/dev/null || printf 'overlay=missing\n'

section "network watchdog"
pid_file="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
log_file="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}"
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
ip addr show "$CONTAINER_BRIDGE" 2>/dev/null || printf 'bridge_missing=%s\n' "$CONTAINER_BRIDGE"
if [ -r "$log_file" ]; then
    tail -n 40 "$log_file" 2>/dev/null || true
else
    printf 'watchdog log not found: %s\n' "$log_file"
fi
