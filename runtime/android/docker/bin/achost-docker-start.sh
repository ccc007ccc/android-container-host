#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
else
    printf 'achost-container-env.sh not found\n' >&2
    exit 1
fi

if [ "$(id -u 2>/dev/null || echo 1)" != "0" ]; then
    printf 'achost-docker-start requires root\n' >&2
    exit 1
fi

require_executable() {
    name="$1"
    path="$ACHOST_BIN/$name"
    if [ ! -x "$path" ]; then
        printf 'missing executable: %s\n' "$path" >&2
        exit 1
    fi
}

ACHOST_DOCKER_RUNTIME="${ACHOST_DOCKER_RUNTIME:-$ACHOST_BIN/achost-docker-runtime}"

pid_running() {
    pid_running_file="$1"
    [ -r "$pid_running_file" ] || return 1
    pid_running_pid="$(cat "$pid_running_file" 2>/dev/null || true)"
    case "$pid_running_pid" in
        ''|*[!0-9]*) return 1 ;;
    esac
    kill -0 "$pid_running_pid" 2>/dev/null
}

supervisor_enabled() {
    [ "$ACHOST_USE_SUPERVISOR" = "1" ] && [ -x "$ACHOST_SUPERVISE" ]
}

supervisor_server_running() {
    pid_running "$ACHOST_SUPERVISOR_PID" && [ -S "$ACHOST_SUPERVISOR_SOCKET" ]
}

ensure_supervisor_server() {
    supervisor_enabled || return 1
    if supervisor_server_running; then
        return 0
    fi
    rm -f "$ACHOST_SUPERVISOR_PID" "$ACHOST_SUPERVISOR_SOCKET" 2>/dev/null || true
    if [ "$ACHOST_USE_CHROOT" = "1" ] && [ "$ACHOST_CHROOT_LAUNCH_MODE" = "pivot" ]; then
        "$ACHOST_SUPERVISE" --server --socket "$ACHOST_SUPERVISOR_SOCKET" --pid-file "$ACHOST_SUPERVISOR_PID" --pivot-root "$ACHOST_CHROOT" >> "$ACHOST_SUPERVISOR_LOG" 2>&1 &
    elif [ "$ACHOST_RUNTIME_MODE" = "native" ]; then
        "$ACHOST_SUPERVISE" --server --socket "$ACHOST_SUPERVISOR_SOCKET" --pid-file "$ACHOST_SUPERVISOR_PID" --native-root "$ACHOST_NATIVE_ROOT" >> "$ACHOST_SUPERVISOR_LOG" 2>&1 &
    else
        "$ACHOST_SUPERVISE" --server --socket "$ACHOST_SUPERVISOR_SOCKET" --pid-file "$ACHOST_SUPERVISOR_PID" >> "$ACHOST_SUPERVISOR_LOG" 2>&1 &
    fi
    i=0
    while [ "$i" -lt 10 ]; do
        supervisor_server_running && return 0
        sleep 1
        i=$((i + 1))
    done
    return 1
}

start_daemon_command() {
    start_daemon_name="$1"
    start_daemon_pid_file="$2"
    start_daemon_log_file="$3"
    start_daemon_chroot="$4"
    shift 4
    if ensure_supervisor_server; then
        if [ "$start_daemon_chroot" = "-" ]; then
            "$ACHOST_SUPERVISE" --client --socket "$ACHOST_SUPERVISOR_SOCKET" --name "$start_daemon_name" --pid-file "$start_daemon_pid_file" -- \
                "$ACHOST_SUPERVISE" --launch --log-file "$start_daemon_log_file" -- "$@" && return 0
        else
            if [ "$ACHOST_CHROOT_LAUNCH_MODE" = "pivot" ]; then
                "$ACHOST_SUPERVISE" --client --socket "$ACHOST_SUPERVISOR_SOCKET" --name "$start_daemon_name" --pid-file "$start_daemon_pid_file" -- \
                    "$@" && return 0
            else
                "$ACHOST_SUPERVISE" --client --socket "$ACHOST_SUPERVISOR_SOCKET" --name "$start_daemon_name" --pid-file "$start_daemon_pid_file" -- \
                    "$ACHOST_SUPERVISE" --launch --log-file "$start_daemon_log_file" --chroot "$start_daemon_chroot" -- "$@" && return 0
            fi
        fi
        printf 'error: achost-supervise client failed for %s\n' "$start_daemon_name" >&2
        return 1
    fi
    printf 'error: achost-supervise server unavailable for %s\n' "$start_daemon_name" >&2
    return 1
}

dockerd_running() {
    pid_running "$ACHOST_DOCKERD_PID"
}

dockerd_pid_for_display() {
    pid_running "$ACHOST_DOCKERD_PID" && cat "$ACHOST_DOCKERD_PID"
}

is_mounted() {
    target="$1"
    while read -r _mount_src mount_dst _mount_type _mount_opts _rest; do
        [ "$mount_dst" = "$target" ] && return 0
    done < /proc/mounts
    return 1
}

chroot_path() {
    case "$1" in
        /*) printf '%s%s\n' "$ACHOST_CHROOT" "$1" ;;
        *) printf '%s/%s\n' "$ACHOST_CHROOT" "$1" ;;
    esac
}

make_mount_private() {
    [ "$1" = "$ACHOST_CHROOT" ] || return 0
    mount --make-rprivate "$1" 2>/dev/null || mount --make-private "$1" 2>/dev/null || true
}

ensure_chroot_mount_root() {
    mkdir -p "$ACHOST_CHROOT"
    if ! is_mounted "$ACHOST_CHROOT"; then
        if ! mount -t tmpfs -o mode=755,size=16m tmpfs "$ACHOST_CHROOT" 2>/dev/null; then
            printf 'warning: unable to mount tmpfs chroot root: %s\n' "$ACHOST_CHROOT" >&2
        fi
    fi
}

bind_mount() {
    src="$1"
    dst="$2"
    mode="${3:-bind}"
    [ -e "$src" ] || return 0
    mkdir -p "$dst"
    if ! is_mounted "$dst"; then
        if [ "$mode" = "rbind" ]; then
            mount --rbind "$src" "$dst" 2>/dev/null || mount --bind "$src" "$dst"
        else
            mount --bind "$src" "$dst"
        fi
    fi
    make_mount_private "$dst"
}

cgroup_v1_mount_point() {
    controller="$1"
    preferred="${2:-}"
    if [ -n "$preferred" ]; then
        while read -r _mount_src mount_dst mount_type mount_opts _rest; do
            [ "$mount_dst" = "$preferred" ] || continue
            [ "$mount_type" = "cgroup" ] || continue
            case ",$mount_opts," in
                *,"$controller",*) printf '%s\n' "$mount_dst"; return 0 ;;
            esac
        done < /proc/mounts
    fi
    while read -r _mount_src mount_dst mount_type mount_opts _rest; do
        [ "$mount_type" = "cgroup" ] || continue
        case ",$mount_opts," in
            *,"$controller",*) printf '%s\n' "$mount_dst"; return 0 ;;
        esac
    done < /proc/mounts
    return 1
}

has_devices_cgroup_mount() {
    cgroup_v1_mount_point devices >/dev/null
}

cgroup_controller_available() {
    controller="$1"
    while read -r name _hierarchy _num enabled; do
        [ "$name" = "$controller" ] && [ "$enabled" = "1" ] && return 0
    done < /proc/cgroups
    return 1
}

cgroup_devices_available() {
    cgroup_controller_available devices
}

setup_devices_cgroup() {
    cgroup_devices_available || return 0
    has_devices_cgroup_mount && return 0
    mkdir -p /dev/achost-cgroup/devices
    mount -t cgroup -o devices none /dev/achost-cgroup/devices 2>/dev/null || \
        printf 'warning: unable to mount devices cgroup\n' >&2
}

ensure_host_memory_cgroup() {
    if memory_mount="$(cgroup_v1_mount_point memory /dev/memcg)"; then
        printf '%s\n' "$memory_mount"
        return 0
    fi
    if ! cgroup_controller_available memory; then
        printf 'warning: memory cgroup controller unavailable\n' >&2
        return 1
    fi
    if [ -r /sys/fs/cgroup/cgroup.controllers ]; then
        cgroup2_controllers="$(cat /sys/fs/cgroup/cgroup.controllers 2>/dev/null || true)"
        case " $cgroup2_controllers " in
            *' memory '*) printf 'warning: memory still exposed in cgroup2; confirm cgroup_no_v2=memory is active\n' >&2 ;;
        esac
    fi
    mkdir -p /dev/memcg 2>/dev/null || {
        printf 'warning: unable to create /dev/memcg\n' >&2
        return 1
    }
    if ! mount -t cgroup -o memory none /dev/memcg 2>/dev/null; then
        printf 'warning: unable to mount memory cgroup at /dev/memcg\n' >&2
        return 1
    fi
    make_mount_private /dev/memcg
    printf '/dev/memcg\n'
}

bind_chroot_path() {
    src="$1"
    bind_mount "$src" "$(chroot_path "$src")"
}

write_chroot_resolv_conf() {
    mkdir -p "$ACHOST_CHROOT/etc"
    : > "$ACHOST_CHROOT/etc/resolv.conf"
    for server in $ACHOST_DNS_SERVERS; do
        printf 'nameserver %s\n' "$server" >> "$ACHOST_CHROOT/etc/resolv.conf"
    done
}

setup_chroot_ca_certs() {
    [ -d /system/etc/security/cacerts ] || return 0
    mkdir -p "$ACHOST_CHROOT/system/etc/security" "$ACHOST_CHROOT/etc/ssl"
    bind_mount /system/etc/security/cacerts "$ACHOST_CHROOT/system/etc/security/cacerts" bind || true
    rm -rf "$ACHOST_CHROOT/etc/ssl/certs" 2>/dev/null || true
    ln -s /system/etc/security/cacerts "$ACHOST_CHROOT/etc/ssl/certs" 2>/dev/null || true
}

mount_virtual_fs() {
    fs_type="$1"
    fs_src="$2"
    dst="$3"
    fs_opts="${4:-}"
    mkdir -p "$dst"
    if ! is_mounted "$dst"; then
        if [ -n "$fs_opts" ]; then
            mount -t "$fs_type" -o "$fs_opts" "$fs_src" "$dst" 2>/dev/null || return 1
        else
            mount -t "$fs_type" "$fs_src" "$dst" 2>/dev/null || return 1
        fi
    fi
    make_mount_private "$dst"
}

mount_chroot_cgroup() {
    controller="$1"
    cgroup_controller_available "$controller" || return 0
    dst="$ACHOST_CHROOT/sys/fs/cgroup/$controller"
    mkdir -p "$dst" 2>/dev/null || return 0
    if ! is_mounted "$dst"; then
        if ! mount -t cgroup -o "$controller" none "$dst" 2>/dev/null; then
            printf 'warning: unable to mount %s cgroup in chroot\n' "$controller" >&2
            return 0
        fi
    fi
    make_mount_private "$dst"
}

mount_chroot_memory_cgroup_v1() {
    dst="$ACHOST_CHROOT/sys/fs/cgroup/memory"
    mkdir -p "$dst" 2>/dev/null || return 0
    is_mounted "$dst" && return 0
    if memory_mount="$(ensure_host_memory_cgroup)"; then
        if mount --bind "$memory_mount" "$dst" 2>/dev/null; then
            make_mount_private "$dst"
            return 0
        fi
        printf 'warning: unable to bind memory cgroup from %s into chroot\n' "$memory_mount" >&2
    fi
    mount_chroot_cgroup memory
}

setup_chroot_cgroups_v1() {
    mount_virtual_fs tmpfs tmpfs "$ACHOST_CHROOT/sys/fs/cgroup" mode=755,size=1m || return 0
    for controller in devices pids cpu cpuacct cpuset blkio freezer; do
        mount_chroot_cgroup "$controller"
    done
    mount_chroot_memory_cgroup_v1
}

setup_chroot_cgroups_v2() {
    if ! cgroup2_root_available; then
        printf 'warning: cgroup mode v2 requested but /sys/fs/cgroup is not cgroup2; falling back to v1 layout\n' >&2
        setup_chroot_cgroups_v1
        return 0
    fi
    dst="$ACHOST_CHROOT/sys/fs/cgroup"
    mkdir -p "$dst" 2>/dev/null || return 0
    if ! is_mounted "$dst"; then
        mount --rbind /sys/fs/cgroup "$dst" 2>/dev/null || mount --bind /sys/fs/cgroup "$dst" 2>/dev/null || {
            printf 'warning: unable to bind cgroup2 into chroot; falling back to v1 layout\n' >&2
            setup_chroot_cgroups_v1
            return 0
        }
    fi
    cgroup2_diagnostics /sys/fs/cgroup
}

setup_chroot_cgroups() {
    case "$ACHOST_CGROUP_MODE" in
        v2) setup_chroot_cgroups_v2 ;;
        *) setup_chroot_cgroups_v1 ;;
    esac
}

setup_chroot() {
    [ "$ACHOST_USE_CHROOT" = "1" ] || return 0
    ensure_chroot_mount_root
    mkdir -p "$ACHOST_CHROOT/run" "$ACHOST_CHROOT/tmp" "$ACHOST_CHROOT/var"
    ln -sfn /run "$ACHOST_CHROOT/var/run" 2>/dev/null || true
    write_chroot_resolv_conf

    bind_mount /dev "$ACHOST_CHROOT/dev" bind || true
    mount_virtual_fs proc proc "$ACHOST_CHROOT/proc" || true
    mount_virtual_fs sysfs sysfs "$ACHOST_CHROOT/sys" || true
    setup_chroot_cgroups
    setup_chroot_ca_certs

    bind_chroot_path "$ACHOST_BIN"
    if [ "${ACHOST_COMMON_BIN:-$ACHOST_BIN}" != "$ACHOST_BIN" ]; then
        bind_chroot_path "$ACHOST_COMMON_BIN"
    fi
    bind_chroot_path "$ACHOST_ETC"
    bind_chroot_path "$ACHOST_RUN"
    bind_chroot_path "$ACHOST_LOG_DIR"
    bind_chroot_path "$ACHOST_DOCKER_ROOT"
    bind_chroot_path "$ACHOST_CONTAINERD_ROOT"
    bind_chroot_path "$ACHOST_CONTAINERD_STATE"
    for bind_path in $ACHOST_BIND_PATHS; do
        mkdir -p "$bind_path" 2>/dev/null || true
        bind_chroot_path "$bind_path"
    done
}

wait_for_socket() {
    path="$1"
    i=0
    while [ "$i" -lt 30 ]; do
        [ -S "$path" ] && return 0
        sleep 1
        i=$((i + 1))
    done
    return 1
}

wait_for_bridge() {
    bridge="$1"
    i=0
    while [ "$i" -lt 30 ]; do
        ip addr show "$bridge" >/dev/null 2>&1 && return 0
        sleep 1
        i=$((i + 1))
    done
    return 1
}

reconcile_network_once() {
    wait_for_bridge "$CONTAINER_BRIDGE" || return 1
    common_bin="${ACHOST_COMMON_BIN:-$ACHOST_BIN}"
    if [ -x "$common_bin/container-nat-manager.sh" ]; then
        "$common_bin/container-nat-manager.sh" >/dev/null 2>&1
        return $?
    fi
    [ -x "$ACHOST_BIN/container-nat-manager.sh" ] || return 0
    "$ACHOST_BIN/container-nat-manager.sh" >/dev/null 2>&1
}

write_dockerd_config() {
    dockerd_template="$DOCKER_CONFIG/daemon.json"
    if [ ! -r "$dockerd_template" ]; then
        printf 'missing dockerd config template: %s\n' "$dockerd_template" >&2
        exit 1
    fi
    mkdir -p "$(dirname -- "$ACHOST_DOCKERD_CONFIG")"
    awk -v prefix="$ACHOST" '{ gsub(/@ACHOST_PREFIX@/, prefix); print }' "$dockerd_template" > "$ACHOST_DOCKERD_CONFIG"
}

write_containerd_config() {
    cat > "$ACHOST_CONTAINERD_CONFIG" <<EOF
version = 3
root = '$ACHOST_CONTAINERD_ROOT'
state = '$ACHOST_CONTAINERD_STATE'
temp = '$ACHOST_RUN/containerd-tmp'
disabled_plugins = ['io.containerd.grpc.v1.cri', 'io.containerd.cri.v1.images', 'io.containerd.cri.v1.runtime']
required_plugins = []
oom_score = 0
imports = []

[grpc]
  address = '$CONTAINERD_ADDRESS'
  tcp_address = ''
  uid = 0
  gid = 0

[debug]
  address = ''
  uid = 0
  gid = 0
  level = 'debug'

[metrics]
  address = ''
  grpc_histogram = false

[plugins.'io.containerd.cri.v1.runtime']
  enable_cdi = false
  cdi_spec_dirs = []

[plugins.'io.containerd.nri.v1.nri']
  disable = true
  socket_path = '$ACHOST_RUN/nri.sock'
EOF
}

start_containerd_daemon() {
    if [ "$ACHOST_USE_CHROOT" = "1" ]; then
        start_daemon_command containerd "$ACHOST_CONTAINERD_PID" "$ACHOST_CONTAINERD_LOG" "$ACHOST_CHROOT" \
            "$ACHOST_BIN/containerd" --config "$ACHOST_CONTAINERD_CONFIG" --log-level debug
    else
        start_daemon_command containerd "$ACHOST_CONTAINERD_PID" "$ACHOST_CONTAINERD_LOG" - \
            "$ACHOST_BIN/containerd" --config "$ACHOST_CONTAINERD_CONFIG" --log-level debug
    fi
}

start_dockerd_external_containerd() {
    dockerd_pid_target="$ACHOST_DOCKERD_LAUNCH_PID"
    set -- "$ACHOST_BIN/dockerd" \
        --config-file "$ACHOST_DOCKERD_CONFIG" \
        --data-root "$ACHOST_DOCKER_ROOT" \
        --exec-root "$ACHOST_DOCKER_EXEC_ROOT" \
        --pidfile "$ACHOST_DOCKERD_PID" \
        --host "$DOCKER_HOST"
    [ -n "${ACHOST_DOCKER_COMPAT_HOST:-}" ] && set -- "$@" --host "$ACHOST_DOCKER_COMPAT_HOST"
    set -- "$@" --containerd "$CONTAINERD_ADDRESS"
    if [ "$ACHOST_USE_CHROOT" = "1" ]; then
        start_daemon_command dockerd "$dockerd_pid_target" "$ACHOST_DOCKERD_LOG" "$ACHOST_CHROOT" "$@"
    else
        start_daemon_command dockerd "$dockerd_pid_target" "$ACHOST_DOCKERD_LOG" - "$@"
    fi
}

start_dockerd_managed_containerd() {
    dockerd_pid_target="$ACHOST_DOCKERD_LAUNCH_PID"
    set -- "$ACHOST_BIN/dockerd" \
        --config-file "$ACHOST_DOCKERD_CONFIG" \
        --data-root "$ACHOST_DOCKER_ROOT" \
        --exec-root "$ACHOST_DOCKER_EXEC_ROOT" \
        --pidfile "$ACHOST_DOCKERD_PID" \
        --host "$DOCKER_HOST"
    [ -n "${ACHOST_DOCKER_COMPAT_HOST:-}" ] && set -- "$@" --host "$ACHOST_DOCKER_COMPAT_HOST"
    if [ "$ACHOST_USE_CHROOT" = "1" ]; then
        start_daemon_command dockerd "$dockerd_pid_target" "$ACHOST_DOCKERD_LOG" "$ACHOST_CHROOT" "$@"
    else
        start_daemon_command dockerd "$dockerd_pid_target" "$ACHOST_DOCKERD_LOG" - "$@"
    fi
}

for name in docker dockerd containerd containerd-shim-runc-v2 ctr runc achost-docker-runtime; do
    require_executable "$name"
done

mkdir -p "$ACHOST_DOCKER_ROOT" "$ACHOST_DOCKER_EXEC_ROOT" "$ACHOST_CONTAINERD_ROOT" "$ACHOST_CONTAINERD_STATE" "$ACHOST_RUN" "$ACHOST_LOG_DIR" "$ACHOST_NATIVE_ROOT" "$DOCKER_CONFIG/cli-plugins" "$(dirname -- "$ACHOST_CONTAINERD_CONFIG")"
printf 'runtime_mode=%s\n' "$ACHOST_RUNTIME_MODE"
printf 'use_chroot=%s\n' "$ACHOST_USE_CHROOT"
printf 'cgroup_mode=%s\n' "$ACHOST_CGROUP_MODE"
printf 'chroot_launch_mode=%s\n' "$ACHOST_CHROOT_LAUNCH_MODE"
if [ "$ACHOST_USE_SUPERVISOR" = "1" ] && [ ! -x "$ACHOST_SUPERVISE" ]; then
    printf 'warning: achost-supervise missing; daemon descendants may be reparented to Android init\n' >&2
fi
if [ "$ACHOST_USE_CHROOT" = "1" ]; then
    setup_chroot
else
    "$ACHOST_DOCKER_RUNTIME" prepare-native-root
    setup_devices_cgroup
    ensure_host_memory_cgroup >/dev/null || true
    ensure_supervisor_server || printf 'warning: native supervisor server not ready; private /run unavailable\n' >&2
    "$ACHOST_DOCKER_RUNTIME" native-preflight
fi

COMMON_BIN="${ACHOST_COMMON_BIN:-$ACHOST_BIN}"
if [ -x "$COMMON_BIN/protect-container-daemons.sh" ]; then
    "$COMMON_BIN/protect-container-daemons.sh" >/dev/null 2>&1 || true
fi

if [ -x "$COMMON_BIN/container-network-watchdog.sh" ]; then
    ACHOST_NET_LOG="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}" \
    ACHOST_NET_PID="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}" \
    "$COMMON_BIN/container-network-watchdog.sh" >/dev/null 2>&1 &
fi

if [ "$ACHOST_EXTERNAL_CONTAINERD" = "1" ]; then
    write_containerd_config
    if pid_running "$ACHOST_CONTAINERD_PID"; then
        printf 'containerd already running pid=%s\n' "$(cat "$ACHOST_CONTAINERD_PID")"
    else
        rm -f "$ACHOST_CONTAINERD_PID" "$CONTAINERD_ADDRESS" 2>/dev/null || true
        start_containerd_daemon
        if wait_for_socket "$CONTAINERD_ADDRESS"; then
            printf 'containerd started pid=%s\n' "$(cat "$ACHOST_CONTAINERD_PID")"
        else
            printf 'containerd socket not ready: %s\n' "$CONTAINERD_ADDRESS" >&2
        fi
    fi
else
    rm -f "$ACHOST_CONTAINERD_PID" "$CONTAINERD_ADDRESS" 2>/dev/null || true
    printf 'external containerd disabled; dockerd will manage containerd\n'
fi

write_dockerd_config
ACHOST_DOCKER_COMPAT_HOST="$("$ACHOST_DOCKER_RUNTIME" prepare-compat-socket)"

if dockerd_running; then
    printf 'dockerd already running pid=%s\n' "$(dockerd_pid_for_display)"
else
    rm -f "$ACHOST_DOCKERD_PID" "$ACHOST_DOCKERD_LAUNCH_PID" "${DOCKER_HOST#unix://}" "${ACHOST_DOCKER_COMPAT_HOST#unix://}" 2>/dev/null || true
    "$ACHOST_DOCKER_RUNTIME" cleanup-stale-iptables
    if [ "$ACHOST_EXTERNAL_CONTAINERD" = "1" ]; then
        start_dockerd_external_containerd
    else
        start_dockerd_managed_containerd
    fi
    if wait_for_socket "${DOCKER_HOST#unix://}"; then
        printf 'dockerd started pid=%s\n' "$(dockerd_pid_for_display)"
        if supervisor_server_running; then
            printf 'supervisor_pid=%s\n' "$(cat "$ACHOST_SUPERVISOR_PID")"
        fi
    else
        printf 'dockerd socket not ready: %s\n' "${DOCKER_HOST#unix://}" >&2
    fi
fi

"$ACHOST_DOCKER_RUNTIME" namespace-diagnostics

if reconcile_network_once; then
    printf 'network reconciled bridge=%s\n' "$CONTAINER_BRIDGE"
else
    printf 'warning: network reconciliation pending for bridge=%s\n' "$CONTAINER_BRIDGE" >&2
fi

printf 'DOCKER_HOST=%s\n' "$DOCKER_HOST"
[ -n "${ACHOST_DOCKER_COMPAT_HOST:-}" ] && printf 'DOCKER_COMPAT_HOST=%s\n' "$ACHOST_DOCKER_COMPAT_HOST"
printf 'dockerd_log=%s\n' "$ACHOST_DOCKERD_LOG"
printf 'containerd_log=%s\n' "$ACHOST_CONTAINERD_LOG"
