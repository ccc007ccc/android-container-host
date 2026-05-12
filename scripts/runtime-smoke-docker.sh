#!/system/bin/sh
set -u

PING_TARGET="${PING_TARGET:-1.1.1.1}"
DNS_TARGET="${DNS_TARGET:-google.com}"
PUBLISHED_PORT="${PUBLISHED_PORT:-18080}"
RUN_PUBLISHED_PORT="${RUN_PUBLISHED_PORT:-1}"
DOCKER_SMOKE_MODE="${DOCKER_SMOKE_MODE:-local}"
DOCKER_LOCAL_BRIDGE="${DOCKER_LOCAL_BRIDGE:-0}"
STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo now)"
LOCAL_IMAGE="${LOCAL_IMAGE:-achost-local-smoke:$STAMP}"
LOCAL_ROOTFS="${LOCAL_ROOTFS:-/data/local/tmp/achost-local-rootfs-$STAMP}"
LOCAL_ROOTFS_TAR="${LOCAL_ROOTFS_TAR:-/data/local/tmp/achost-local-rootfs-$STAMP.tar}"
OUT_DIR="${OUT_DIR:-}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
    ACHOST_BIN="$SCRIPT_DIR"
    . "$SCRIPT_DIR/achost-container-env.sh"
fi
FAILURES=0
IMPORTED_LOCAL_IMAGE=0

if [ -n "$OUT_DIR" ]; then
    mkdir -p "$OUT_DIR"
    LOG_FILE="$OUT_DIR/runtime-smoke-docker.txt"
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
    return "$rc"
}

run_optional() {
    title="$1"
    shift
    section "$title"
    "$@" 2>&1 || true
}

safe_remove_local_path() {
    path="$1"
    case "$path" in
        /data/local/tmp/achost-local-rootfs*) rm -rf "$path" 2>/dev/null || true ;;
        '') ;;
        *) printf 'skip removing non-generated path: %s\n' "$path" ;;
    esac
}

cleanup() {
    docker rm -f achost-nginx >/dev/null 2>&1 || true
    if [ "$IMPORTED_LOCAL_IMAGE" = "1" ]; then
        docker rmi "$LOCAL_IMAGE" >/dev/null 2>&1 || true
    fi
    safe_remove_local_path "$LOCAL_ROOTFS"
    safe_remove_local_path "$LOCAL_ROOTFS_TAR"
}

make_local_rootfs() {
    docker_bin="$(command -v docker 2>/dev/null || true)"
    if [ -z "$docker_bin" ]; then
        printf 'docker command not found\n' >&2
        return 1
    fi
    case "$LOCAL_ROOTFS" in
        /data/local/tmp/achost-local-rootfs*) ;;
        *) printf 'unsafe LOCAL_ROOTFS: %s\n' "$LOCAL_ROOTFS" >&2; return 1 ;;
    esac
    case "$LOCAL_ROOTFS_TAR" in
        /data/local/tmp/achost-local-rootfs*.tar) ;;
        *) printf 'unsafe LOCAL_ROOTFS_TAR: %s\n' "$LOCAL_ROOTFS_TAR" >&2; return 1 ;;
    esac
    safe_remove_local_path "$LOCAL_ROOTFS"
    safe_remove_local_path "$LOCAL_ROOTFS_TAR"
    mkdir -p "$LOCAL_ROOTFS/bin" || return 1
    cp "$docker_bin" "$LOCAL_ROOTFS/bin/docker" || return 1
    chmod 755 "$LOCAL_ROOTFS/bin/docker" 2>/dev/null || true
    (cd "$LOCAL_ROOTFS" && tar -cf "$LOCAL_ROOTFS_TAR" .)
}

check_local_image_absent() {
    if docker image inspect "$LOCAL_IMAGE" >/dev/null 2>&1; then
        printf 'local smoke image tag already exists: %s\n' "$LOCAL_IMAGE" >&2
        return 1
    fi
    printf 'local_smoke_image=%s\n' "$LOCAL_IMAGE"
    return 0
}

run_local_smoke() {
    run_required "local smoke image tag" check_local_image_absent || return 1
    run_required "local smoke rootfs" make_local_rootfs || return 1
    if run_required "docker import local smoke image" docker import "$LOCAL_ROOTFS_TAR" "$LOCAL_IMAGE"; then
        IMPORTED_LOCAL_IMAGE=1
    else
        return 1
    fi
    run_required "local container no network" docker run --rm --network none "$LOCAL_IMAGE" /bin/docker --version
    if [ "$DOCKER_SMOKE_MODE" = "local-bridge" ] || [ "$DOCKER_LOCAL_BRIDGE" = "1" ]; then
        run_required "local container bridge attach" docker run --rm --network bridge "$LOCAL_IMAGE" /bin/docker --version
    fi
    if run_required "remove local smoke image" docker rmi "$LOCAL_IMAGE"; then
        IMPORTED_LOCAL_IMAGE=0
    fi
}

run_full_smoke() {
    run_required "docker hello-world" docker run --rm hello-world
    run_required "busybox uname" docker run --rm busybox uname -a
    run_required "busybox echo" docker run --rm busybox sh -c 'echo ok'
    run_required "host network ping" docker run --rm --network host busybox ping -c 3 "$PING_TARGET"
    run_required "bridge network ping" docker run --rm --network bridge busybox ping -c 3 "$PING_TARGET"
    run_required "bridge DNS" docker run --rm --network bridge busybox nslookup "$DNS_TARGET"
    run_required "memory limit" docker run --rm -m 128m busybox true
    run_required "cpu limit" docker run --rm --cpus=0.5 busybox true
    run_required "volume write" docker run --rm -v /data/local/tmp:/mnt busybox sh -c 'echo ok > /mnt/docker-volume-test && cat /mnt/docker-volume-test'

    if [ "$RUN_PUBLISHED_PORT" = "1" ]; then
        run_required "published port start" docker run -d --name achost-nginx -p "$PUBLISHED_PORT:80" nginx:alpine
        if command -v curl >/dev/null 2>&1; then
            run_required "published port curl" curl -fsS "http://127.0.0.1:$PUBLISHED_PORT"
        else
            run_optional "published port skipped" sh -c 'echo curl command not found'
        fi
    fi
}

command -v docker >/dev/null 2>&1 || {
    printf 'docker command not found\n' >&2
    exit 2
}

trap cleanup EXIT INT TERM

printf 'docker_smoke_mode=%s\n' "$DOCKER_SMOKE_MODE"
run_required "docker version" docker version
if docker compose version >/dev/null 2>&1; then
    run_optional "docker compose version" docker compose version
elif command -v docker-compose >/dev/null 2>&1; then
    run_optional "docker-compose version" docker-compose version
else
    run_optional "docker compose skipped" sh -c 'echo compose plugin not found'
fi
run_required "docker info" docker info
run_required "overlay2 storage driver" sh -c "docker info 2>/dev/null | grep -i 'Storage Driver: overlay2'"

case "$DOCKER_SMOKE_MODE" in
    local|local-bridge)
        run_local_smoke
        ;;
    full|pull|network)
        run_full_smoke
        ;;
    *)
        printf 'unsupported DOCKER_SMOKE_MODE: %s\n' "$DOCKER_SMOKE_MODE" >&2
        exit 2
        ;;
esac

run_optional "docker0" ip addr show docker0
run_optional "iptables nat" iptables -t nat -S
run_optional "iptables forward" iptables -S FORWARD
run_optional "docker bridge inspect" docker network inspect bridge
run_optional "recent kernel log" sh -c 'dmesg 2>/dev/null | tail -200'

if [ -n "$OUT_DIR" ]; then
    printf 'docker smoke log written to %s\n' "$LOG_FILE" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    printf 'Docker smoke failures: %s\n' "$FAILURES" >&2
    exit 1
fi

printf 'Docker smoke passed\n'
