#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
. "$SCRIPT_DIR/achost-container-env.sh"

if [ "$(id -u 2>/dev/null || echo 1)" != "0" ]; then
    printf 'achost-docker-stop requires root\n' >&2
    exit 1
fi

stop_pid_file() {
    name="$1"
    pid_file="$2"
    if [ ! -r "$pid_file" ]; then
        printf '%s pid file missing: %s\n' "$name" "$pid_file"
        return 0
    fi
    pid="$(cat "$pid_file" 2>/dev/null || true)"
    case "$pid" in
        ''|*[!0-9]*)
            printf '%s pid invalid: %s\n' "$name" "$pid"
            rm -f "$pid_file" 2>/dev/null || true
            return 0
            ;;
    esac
    if ! kill -0 "$pid" 2>/dev/null; then
        printf '%s not running pid=%s\n' "$name" "$pid"
        rm -f "$pid_file" 2>/dev/null || true
        return 0
    fi
    kill "$pid" 2>/dev/null || true
    i=0
    while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 10 ]; do
        sleep 1
        i=$((i + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$pid_file" 2>/dev/null || true
    printf '%s stopped pid=%s\n' "$name" "$pid"
}

stop_named_processes() {
    name="$1"
    pids="$(pidof "$name" 2>/dev/null || true)"
    [ -n "$pids" ] || return 0
    for pid in $pids; do
        kill "$pid" 2>/dev/null || true
    done
    sleep 1
    for pid in $pids; do
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    done
    printf '%s stopped leftover pids=%s\n' "$name" "$pids"
}

unmount_chroot() {
    [ "$ACHOST_USE_CHROOT" = "1" ] || return 0
    [ -r /proc/mounts ] || return 0
    mount --make-rprivate "$ACHOST_CHROOT" 2>/dev/null || mount --make-private "$ACHOST_CHROOT" 2>/dev/null || true
    i=0
    while [ "$i" -lt 8 ]; do
        mounts="$(while read -r _mount_src mount_dst _mount_type _mount_opts _rest; do
            case "$mount_dst" in
                "$ACHOST_CHROOT"/*) printf '%s\n' "$mount_dst" ;;
            esac
        done < /proc/mounts)"
        [ -n "$mounts" ] || break
        printf '%s\n' "$mounts" | sort -r | while read -r mount_dst; do
            umount "$mount_dst" 2>/dev/null || umount -l "$mount_dst" 2>/dev/null || true
        done
        i=$((i + 1))
    done
    i=0
    while grep -q " $ACHOST_CHROOT " /proc/mounts 2>/dev/null && [ "$i" -lt 4 ]; do
        umount "$ACHOST_CHROOT" 2>/dev/null || umount -l "$ACHOST_CHROOT" 2>/dev/null || break
        i=$((i + 1))
    done
}

unmount_devices_cgroup() {
    [ -r /proc/mounts ] || return 0
    if grep -q ' /dev/achost-cgroup/devices ' /proc/mounts 2>/dev/null; then
        umount /dev/achost-cgroup/devices 2>/dev/null || umount -l /dev/achost-cgroup/devices 2>/dev/null || true
    fi
    rmdir /dev/achost-cgroup/devices /dev/achost-cgroup 2>/dev/null || true
}

stop_pid_file dockerd "$ACHOST_DOCKERD_PID"
stop_pid_file dockerd-launch "$ACHOST_DOCKERD_LAUNCH_PID"
stop_pid_file containerd "$ACHOST_CONTAINERD_PID"
stop_named_processes dockerd
stop_named_processes containerd
stop_pid_file achost-supervise "$ACHOST_SUPERVISOR_PID"
rm -f "$ACHOST_SUPERVISOR_SOCKET" 2>/dev/null || true
unmount_chroot
unmount_devices_cgroup
case "$DOCKER_HOST" in
    unix://*) rm -f "${DOCKER_HOST#unix://}" 2>/dev/null || true ;;
esac
rm -f "$CONTAINERD_ADDRESS" 2>/dev/null || true
