#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
ACHOST_BIN="$SCRIPT_DIR"
ACHOST_ETC="$ACHOST/etc"
ACHOST_RUNTIME_CONF="$ACHOST_ETC/achost-runtime.conf"
ACHOST_BASE_ENV_PRESENT=0

if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
    ACHOST_BASE_ENV_PRESENT=1
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
    ACHOST_BASE_ENV_PRESENT=1
fi

export ACHOST ACHOST_BIN ACHOST_ETC ACHOST_RUNTIME_CONF ACHOST_BASE_ENV_PRESENT
export ACHOST_MODULE_TARGET ACHOST_COMMON ACHOST_COMMON_BIN ACHOST_VAR ACHOST_CONFIG ACHOST_RUN ACHOST_LOG_DIR
export ACHOST_LXC_RUNTIME ACHOST_LXC ACHOST_LXC_BIN ACHOST_LXC_ETC ACHOST_LXC_VAR ACHOST_LXC_RUN ACHOST_LXC_LOG ACHOST_LXC_ROOTFS ACHOST_LXC_CONTAINERS LXC_BRIDGE LXC_SUBNET
exec "$SCRIPT_DIR/achost-webui-api" "$@"
