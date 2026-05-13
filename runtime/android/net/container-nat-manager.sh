#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="${ACHOST_BIN:-$SCRIPT_DIR}"

if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
elif [ -r "$SCRIPT_DIR/../bin/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/../bin/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
fi

ACHOST_COMMON="${ACHOST_COMMON:-${ACHOST_BASE:-$ACHOST}}"
ACHOST_COMMON_BIN="${ACHOST_COMMON_BIN:-$ACHOST_COMMON/bin}"
ACHOST_RUNTIME_CORE="${ACHOST_RUNTIME_CORE:-$ACHOST_COMMON_BIN/achost-runtime-core}"

if [ ! -x "$ACHOST_RUNTIME_CORE" ]; then
    printf 'achost-runtime-core not found: %s\n' "$ACHOST_RUNTIME_CORE" >&2
    exit 1
fi

export ACHOST ACHOST_BIN ACHOST_COMMON ACHOST_COMMON_BIN
exec "$ACHOST_RUNTIME_CORE" net-reconcile "$@"
