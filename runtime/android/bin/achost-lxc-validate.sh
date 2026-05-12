#!/system/bin/sh
set -u

if [ -z "${SCRIPT_DIR:-}" ]; then
    SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
fi
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="${ACHOST_BIN:-$SCRIPT_DIR}"
. "$SCRIPT_DIR/achost-container-env.sh"

lxc_binary_status() {
    name="$1"
    required="$2"
    for path in "$ACHOST_LXC_BIN/$name" "$ACHOST_BIN/$name"; do
        if [ -e "$path" ]; then
            if [ -x "$path" ]; then
                printf '%s=found path=%s executable=1 required=%s\n' "$name" "$path" "$required"
            else
                printf '%s=found path=%s executable=0 required=%s\n' "$name" "$path" "$required"
            fi
            return 0
        fi
    done
    if command -v "$name" >/dev/null 2>&1; then
        printf '%s=found path=%s executable=1 required=%s\n' "$name" "$(command -v "$name")" "$required"
    else
        printf '%s=missing required=%s\n' "$name" "$required"
    fi
}

printf 'ACHOST_LXC=%s\n' "$ACHOST_LXC"
printf 'ACHOST_LXC_BIN=%s\n' "$ACHOST_LXC_BIN"
for name in lxc-start lxc-stop lxc-attach lxc-create lxc-info lxc-ls lxc-checkconfig; do
    lxc_binary_status "$name" 0
done
printf 'lxc_default_conf=%s\n' "$ACHOST_ETC/lxc/default.conf"
[ -r "$ACHOST_ETC/lxc/default.conf" ] && printf 'lxc_default_conf_readable=1\n' || printf 'lxc_default_conf_readable=0\n'
