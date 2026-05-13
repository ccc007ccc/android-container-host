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

ACHOST_DOCKER_RUNTIME="${ACHOST_DOCKER_RUNTIME:-$ACHOST_BIN/achost-docker-runtime}"
if [ ! -x "$ACHOST_DOCKER_RUNTIME" ]; then
    printf 'missing executable: %s\n' "$ACHOST_DOCKER_RUNTIME" >&2
    exit 1
fi

exec "$ACHOST_DOCKER_RUNTIME" start "$@"
