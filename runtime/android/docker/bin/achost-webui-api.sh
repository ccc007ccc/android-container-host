#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
ACHOST_BASE_ENV_PRESENT=0

if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
    ACHOST_BASE_ENV_PRESENT=1
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
    ACHOST_BASE_ENV_PRESENT=1
fi

export ACHOST ACHOST_BIN ACHOST_BASE_ENV_PRESENT
exec "$SCRIPT_DIR/achost-webui-api" "$@"
