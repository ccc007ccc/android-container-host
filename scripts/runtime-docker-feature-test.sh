#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
if [ -r "$ACHOST/bin/achost-container-env.sh" ]; then
    . "$ACHOST/bin/achost-container-env.sh"
fi

DOCKER="$ACHOST_BIN/docker"
STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo now)"
REPORT="${REPORT:-/data/local/tmp/achost-docker-feature-$STAMP.txt}"
IMAGE="${IMAGE:-achost-dockertest:local}"
ROOTFS="${ROOTFS:-/data/local/tmp/achost-dockertest-rootfs.tar}"
BIND_SRC="${BIND_SRC:-/data/local/tmp/achost-bind-visible}"

pass_count=0
fail_count=0
limit_count=0
skip_count=0

mkdir -p "$(dirname -- "$REPORT")"
: > "$REPORT"
exec >> "$REPORT" 2>&1

record() {
    status="$1"
    name="$2"
    note="${3:-}"
    printf 'RESULT\t%s\t%s\t%s\n' "$status" "$name" "$note"
    case "$status" in
        PASS) pass_count=$((pass_count + 1)) ;;
        FAIL) fail_count=$((fail_count + 1)) ;;
        LIMIT) limit_count=$((limit_count + 1)) ;;
        SKIP) skip_count=$((skip_count + 1)) ;;
    esac
}

section() {
    printf '\n## TEST %s\n' "$1"
}

run_test() {
    name="$1"
    shift
    section "$name"
    if "$@"; then
        record PASS "$name" ""
    else
        record FAIL "$name" "rc=$?"
    fi
}

require_docker() {
    if [ ! -x "$DOCKER" ]; then
        record FAIL "docker cli present" "missing $DOCKER"
        return 1
    fi
    return 0
}

cleanup_containers() {
    "$DOCKER" rm -f achost-feature-life achost-feature-cp >/dev/null 2>&1 || true
}

have_rootfs() {
    [ -r "$ROOTFS" ]
}

import_image() {
    have_rootfs || return 1
    "$DOCKER" import "$ROOTFS" "$IMAGE" >/dev/null
}

proxy_env_absent() {
    "$DOCKER" run --rm --network none "$IMAGE" /bin/dockertest env > /data/local/tmp/achost-feature-env.txt
    if grep -E '^(HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|NO_PROXY|http_proxy|https_proxy|all_proxy|no_proxy)=' /data/local/tmp/achost-feature-env.txt; then
        return 1
    fi
    return 0
}

bind_mount_check() {
    rm -rf "$BIND_SRC"
    mkdir -p "$BIND_SRC"
    "$DOCKER" run --rm -v "$BIND_SRC:/mnt/bind" "$IMAGE" /bin/dockertest write-file /mnt/bind/from-container.txt bind-ok || return 1
    if grep -q 'bind-ok' "$BIND_SRC/from-container.txt" 2>/dev/null; then
        record PASS "bind mount host path visibility" ""
        return 0
    fi
    if [ "${ACHOST_USE_CHROOT:-0}" = "1" ] && grep -q 'bind-ok' "$ACHOST_CHROOT$BIND_SRC/from-container.txt" 2>/dev/null; then
        record LIMIT "bind mount host path visibility" "source resolved inside dockerd chroot"
        return 0
    fi
    record FAIL "bind mount host path visibility" "file not visible at host or chroot source"
    return 0
}

feature_matrix() {
    require_docker || return 0
    cleanup_containers

    run_test "docker version" "$DOCKER" version
    run_test "docker info" "$DOCKER" info

    section "import dockertest image"
    if import_image; then
        record PASS "import dockertest image" ""
    else
        record SKIP "import dockertest image" "missing $ROOTFS"
        return 0
    fi

    run_test "container run none" sh -c "$DOCKER run --rm --network none '$IMAGE' /bin/dockertest info | grep -q 'dockertest=ok'"

    section "detached lifecycle logs"
    if cid="$($DOCKER run -d --name achost-feature-life "$IMAGE" /bin/dockertest hold 60)" && sleep 2 && "$DOCKER" ps | grep -q achost-feature-life && "$DOCKER" logs achost-feature-life | grep -q 'hold=ready'; then
        printf 'cid=%s\n' "$cid"
        record PASS "detached lifecycle logs" ""
    else
        record FAIL "detached lifecycle logs" "rc=$?"
    fi

    section "docker exec"
    if "$DOCKER" exec achost-feature-life /bin/dockertest info; then
        record PASS "docker exec" ""
    else
        record FAIL "docker exec" "rc=$?"
    fi
    "$DOCKER" rm -f achost-feature-life >/dev/null 2>&1 || true

    section "docker cp in and out"
    if printf 'copy-ok' > /data/local/tmp/achost-feature-cp-in.txt && cid="$($DOCKER run -d --name achost-feature-cp "$IMAGE" /bin/dockertest hold 60)" && sleep 1 && "$DOCKER" cp /data/local/tmp/achost-feature-cp-in.txt achost-feature-cp:/from-host.txt && rm -f /data/local/tmp/achost-feature-cp-out.txt && "$DOCKER" cp achost-feature-cp:/from-host.txt /data/local/tmp/achost-feature-cp-out.txt && grep -q 'copy-ok' /data/local/tmp/achost-feature-cp-out.txt; then
        printf 'cid=%s\n' "$cid"
        record PASS "docker cp in and out" ""
    else
        record FAIL "docker cp in and out" "rc=$?"
    fi
    "$DOCKER" rm -f achost-feature-cp >/dev/null 2>&1 || true

    section "bind mount host path visibility"
    bind_mount_check

    run_test "proxy env absent" proxy_env_absent
}

feature_matrix
cleanup_containers
printf '\n# SUMMARY pass=%s fail=%s limit=%s skip=%s\n' "$pass_count" "$fail_count" "$limit_count" "$skip_count"
printf 'REPORT=%s\n' "$REPORT"
exit 0
