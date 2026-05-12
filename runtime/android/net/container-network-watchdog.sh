#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
CONTAINER_BRIDGE="${CONTAINER_BRIDGE:-${DOCKER_BRIDGE:-docker0}}"
DOCKER_BRIDGE="$CONTAINER_BRIDGE"
DOCKER_SUBNET="${DOCKER_SUBNET:-}"
UPLINK="${UPLINK:-}"
TARGET="${TARGET:-1.1.1.1}"
IPTABLES="${IPTABLES:-}"
WATCH_INTERVAL="${ACHOST_NET_WATCH_INTERVAL:-5}"
REPAIR_INTERVAL="${ACHOST_NET_REPAIR_INTERVAL:-30}"
LOG_FILE="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}"
PID_FILE="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
NAT_MANAGER="${ACHOST_NAT_MANAGER:-$SCRIPT_DIR/container-nat-manager.sh}"
DRY_RUN="${ACHOST_DRY_RUN:-0}"

case "$WATCH_INTERVAL" in
    ''|*[!0-9]*) WATCH_INTERVAL=5 ;;
esac
case "$REPAIR_INTERVAL" in
    ''|*[!0-9]*) REPAIR_INTERVAL=30 ;;
esac

LOG_DIR="${LOG_FILE%/*}"
if [ "$LOG_DIR" != "$LOG_FILE" ]; then
    mkdir -p "$LOG_DIR" 2>/dev/null || true
fi
PID_DIR="${PID_FILE%/*}"
if [ "$PID_DIR" != "$PID_FILE" ]; then
    mkdir -p "$PID_DIR" 2>/dev/null || true
fi

timestamp() {
    date '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date 2>/dev/null || printf 'now'
}

log_msg() {
    line="$(timestamp) $*"
    if ! printf '%s\n' "$line" >> "$LOG_FILE" 2>/dev/null; then
        printf '%s\n' "$line"
    fi
}

have() {
    command -v "$1" >/dev/null 2>&1
}

bridge_exists() {
    ip addr show "$CONTAINER_BRIDGE" >/dev/null 2>&1
}

bridge_subnet() {
    if [ -n "$DOCKER_SUBNET" ]; then
        printf '%s\n' "$DOCKER_SUBNET"
        return 0
    fi
    subnet="$(ip -4 route show dev "$CONTAINER_BRIDGE" scope link 2>/dev/null | awk '/^[0-9][0-9.]*\/[0-9][0-9]* / {print $1; exit}')"
    if [ -n "$subnet" ]; then
        printf '%s\n' "$subnet"
        return 0
    fi
    ip -4 addr show "$CONTAINER_BRIDGE" 2>/dev/null | awk '/ inet / {print $2; exit}'
}

resolve_uplink() {
    if [ -n "$UPLINK" ]; then
        printf '%s\n' "$UPLINK"
        return 0
    fi
    if [ -x "$SCRIPT_DIR/detect-uplink.sh" ]; then
        "$SCRIPT_DIR/detect-uplink.sh" "$TARGET"
        return $?
    fi
    ip route get "$TARGET" 2>/dev/null | while read -r token rest; do
        set -- $token $rest
        while [ "$#" -gt 0 ]; do
            if [ "$1" = "dev" ] && [ "$#" -ge 2 ]; then
                printf '%s\n' "$2"
                exit 0
            fi
            shift
        done
    done
}

if [ -r "$PID_FILE" ]; then
    OLD_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
    case "$OLD_PID" in
        ''|*[!0-9]*) ;;
        *)
            if kill -0 "$OLD_PID" 2>/dev/null; then
                log_msg "watchdog already running pid=$OLD_PID"
                exit 0
            fi
            ;;
    esac
fi

printf '%s\n' "$$" > "$PID_FILE" 2>/dev/null || true
trap '' HUP
trap 'rm -f "$PID_FILE" 2>/dev/null || true' EXIT INT TERM

if [ ! -x "$NAT_MANAGER" ]; then
    log_msg "error: NAT manager not executable: $NAT_MANAGER"
    exit 1
fi

log_msg "watchdog starting pid=$$ bridge=$CONTAINER_BRIDGE target=$TARGET interval=$WATCH_INTERVAL repair_interval=$REPAIR_INTERVAL"
LAST_STATE=""
LAST_WAIT=""
CYCLES="$REPAIR_INTERVAL"

while :; do
    if ! have ip; then
        if [ "$LAST_WAIT" != "missing-ip" ]; then
            log_msg "waiting: ip command not found"
            LAST_WAIT="missing-ip"
        fi
        sleep "$WATCH_INTERVAL"
        continue
    fi

    if ! bridge_exists; then
        if [ "$LAST_WAIT" != "missing-bridge" ]; then
            log_msg "waiting: $CONTAINER_BRIDGE not found"
            LAST_WAIT="missing-bridge"
        fi
        sleep "$WATCH_INTERVAL"
        continue
    fi

    SUBNET_RESOLVED="$(bridge_subnet 2>/dev/null || true)"
    if [ -z "$SUBNET_RESOLVED" ]; then
        if [ "$LAST_WAIT" != "missing-subnet" ]; then
            log_msg "waiting: cannot determine IPv4 subnet for $CONTAINER_BRIDGE"
            LAST_WAIT="missing-subnet"
        fi
        sleep "$WATCH_INTERVAL"
        continue
    fi

    UPLINK_RESOLVED="$(resolve_uplink 2>/dev/null || true)"
    if [ -z "$UPLINK_RESOLVED" ]; then
        if [ "$LAST_WAIT" != "missing-uplink" ]; then
            log_msg "waiting: cannot determine uplink for target $TARGET"
            LAST_WAIT="missing-uplink"
        fi
        sleep "$WATCH_INTERVAL"
        continue
    fi

    LAST_WAIT=""
    STATE="$CONTAINER_BRIDGE|$SUBNET_RESOLVED|$UPLINK_RESOLVED|$IPTABLES"
    if [ "$STATE" != "$LAST_STATE" ]; then
        log_msg "state: bridge=$CONTAINER_BRIDGE subnet=$SUBNET_RESOLVED uplink=$UPLINK_RESOLVED iptables=${IPTABLES:-auto}"
        LAST_STATE="$STATE"
        CYCLES="$REPAIR_INTERVAL"
    fi

    if [ "$CYCLES" -ge "$REPAIR_INTERVAL" ]; then
        if CONTAINER_BRIDGE="$CONTAINER_BRIDGE" DOCKER_BRIDGE="$CONTAINER_BRIDGE" DOCKER_SUBNET="$SUBNET_RESOLVED" UPLINK="$UPLINK_RESOLVED" TARGET="$TARGET" IPTABLES="$IPTABLES" ACHOST_DRY_RUN="$DRY_RUN" "$NAT_MANAGER" >> "$LOG_FILE" 2>&1; then
            log_msg "reconcile ok: bridge=$CONTAINER_BRIDGE uplink=$UPLINK_RESOLVED"
        else
            rc=$?
            log_msg "reconcile failed: bridge=$CONTAINER_BRIDGE uplink=$UPLINK_RESOLVED exit=$rc"
        fi
        CYCLES=0
    fi

    sleep "$WATCH_INTERVAL"
    CYCLES=$((CYCLES + WATCH_INTERVAL))
done
