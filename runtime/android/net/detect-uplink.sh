#!/system/bin/sh
set -u

TARGET="${1:-1.1.1.1}"

if ! command -v ip >/dev/null 2>&1; then
    echo "ip command not found" >&2
    exit 2
fi

link_is_usable() {
    dev="$1"
    link="$(ip link show "$dev" 2>/dev/null || true)"
    [ -n "$link" ] || return 1
    case "$link" in
        *NO-CARRIER*|*"state DOWN"*) return 1 ;;
    esac
    return 0
}

route="$(ip route get "$TARGET" 2>/dev/null || true)"
if [ -z "$route" ]; then
    echo "failed to resolve uplink for $TARGET" >&2
    exit 1
fi

set -- $route
while [ "$#" -gt 0 ]; do
    if [ "$1" = "dev" ] && [ "$#" -ge 2 ]; then
        if link_is_usable "$2"; then
            echo "$2"
            exit 0
        fi
        echo "route dev is not usable: $2" >&2
        exit 1
    fi
    shift
done

echo "no dev field in route: $route" >&2
exit 1
