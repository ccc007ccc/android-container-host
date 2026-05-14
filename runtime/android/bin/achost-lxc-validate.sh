#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="${ACHOST_BIN:-$SCRIPT_DIR}"

if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
else
    printf 'ACHost env not found\n' >&2
    exit 1
fi

ACHOST_LXC_MODULE="${ACHOST_LXC_MODULE:-$ACHOST}"
ACHOST_LXC="${ACHOST_LXC:-$ACHOST/lxc}"
ACHOST_LXC_BIN="${ACHOST_LXC_BIN:-$ACHOST_LXC/bin}"
ACHOST_LXC_ETC="${ACHOST_LXC_ETC:-$ACHOST/etc/lxc}"
ACHOST_LXC_RUNTIME="${ACHOST_LXC_RUNTIME:-$SCRIPT_DIR/achost-lxc-runtime}"

if [ ! -x "$ACHOST_LXC_RUNTIME" ]; then
    printf 'missing executable: %s\n' "$ACHOST_LXC_RUNTIME" >&2
    exit 1
fi

exec "$ACHOST_LXC_RUNTIME" validate-assets "$@"
