#!/system/bin/sh
set -u

CONTAINER_BRIDGE="${CONTAINER_BRIDGE:-${DOCKER_BRIDGE:-docker0}}"
DOCKER_BRIDGE="$CONTAINER_BRIDGE"
DOCKER_SUBNET="${DOCKER_SUBNET:-}"
UPLINK="${UPLINK:-}"
TARGET="${TARGET:-1.1.1.1}"
IPTABLES="${IPTABLES:-}"
DRY_RUN="${ACHOST_DRY_RUN:-0}"
POLICY_RULES="${ACHOST_CONTAINER_POLICY_RULES:-1}"
RETURN_RULE_PRIORITY="${ACHOST_CONTAINER_RETURN_RULE_PRIORITY:-11999}"
SOURCE_RULE_PRIORITY="${ACHOST_CONTAINER_SOURCE_RULE_PRIORITY:-12000}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

case "$RETURN_RULE_PRIORITY" in
    ''|*[!0-9]*) RETURN_RULE_PRIORITY=11999 ;;
esac
case "$SOURCE_RULE_PRIORITY" in
    ''|*[!0-9]*) SOURCE_RULE_PRIORITY=12000 ;;
esac

log() {
    printf '%s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

run() {
    if [ "$DRY_RUN" = "1" ]; then
        printf '+ %s\n' "$*"
        return 0
    fi
    "$@"
}

have() {
    command -v "$1" >/dev/null 2>&1
}

pick_iptables() {
    if [ -n "$IPTABLES" ]; then
        echo "$IPTABLES"
        return 0
    fi
    for cmd in iptables iptables-legacy iptables-nft; do
        if have "$cmd"; then
            echo "$cmd"
            return 0
        fi
    done
    return 1
}

set_sysctl_value() {
    key="$1"
    value="$2"
    proc="/proc/sys/$(printf '%s' "$key" | tr . /)"

    if [ -w "$proc" ]; then
        if [ "$DRY_RUN" = "1" ]; then
            printf '+ echo %s > %s\n' "$value" "$proc"
        else
            printf '%s\n' "$value" > "$proc"
        fi
        return 0
    fi

    if have sysctl; then
        run sysctl -w "$key=$value"
        return $?
    fi

    log "warn: cannot set $key"
    return 0
}

require_bridge() {
    ip addr show "$CONTAINER_BRIDGE" >/dev/null 2>&1 || fail "$CONTAINER_BRIDGE not found"
}

resolve_uplink() {
    if [ -n "$UPLINK" ]; then
        echo "$UPLINK"
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
                echo "$2"
                exit 0
            fi
            shift
        done
    done
}

resolve_docker_subnet() {
    subnet="$(ip -4 route show dev "$CONTAINER_BRIDGE" scope link 2>/dev/null | awk '/^[0-9][0-9.]*\/[0-9][0-9]* / {print $1; exit}')"
    if [ -n "$subnet" ]; then
        echo "$subnet"
        return 0
    fi
    if [ -n "$DOCKER_SUBNET" ]; then
        echo "$DOCKER_SUBNET"
        return 0
    fi
    subnet="$(ip -4 addr show "$CONTAINER_BRIDGE" 2>/dev/null | awk '/ inet / {print $2; exit}')"
    [ -n "$subnet" ] || fail "cannot determine IPv4 subnet for $CONTAINER_BRIDGE; set DOCKER_SUBNET"
    echo "$subnet"
}

delete_existing_rules() {
    table="$1"
    chain="$2"
    shift 2
    ipt="$IPTABLES_BIN"

    [ "$DRY_RUN" = "1" ] && return 0
    if [ "$table" = "filter" ]; then
        while "$ipt" -D "$chain" "$@" >/dev/null 2>&1; do
            :
        done
        return 0
    fi

    while "$ipt" -t "$table" -D "$chain" "$@" >/dev/null 2>&1; do
        :
    done
}

ensure_rule() {
    table="$1"
    chain="$2"
    shift 2
    ipt="$IPTABLES_BIN"

    delete_existing_rules "$table" "$chain" "$@"
    if [ "$table" = "filter" ]; then
        run "$ipt" -I "$chain" "$@"
        return $?
    fi

    run "$ipt" -t "$table" -A "$chain" "$@"
}

count_ip_rules_at_priority() {
    count_rule_priority="$1"
    ip rule show | awk -v priority="$count_rule_priority:" '$1 == priority { count++ } END { print count + 0 }'
}

count_exact_ip_rules_at_priority() {
    exact_rule_priority="$1"
    exact_rule_needle="$2"
    ip rule show | grep -F "$exact_rule_needle" | awk -v priority="$exact_rule_priority:" '$1 == priority { count++ } END { print count + 0 }'
}

delete_ip_rules_at_priority() {
    delete_rule_priority="$1"
    if [ "$DRY_RUN" = "1" ]; then
        printf '+ ip rule del priority %s # until empty\n' "$delete_rule_priority"
        return 0
    fi
    while ip rule del priority "$delete_rule_priority" 2>/dev/null; do
        :
    done
}

ensure_ip_rule() {
    ensure_rule_priority="$1"
    ensure_rule_needle="$2"
    shift 2

    if [ "$DRY_RUN" != "1" ]; then
        ensure_rule_count="$(count_ip_rules_at_priority "$ensure_rule_priority")"
        ensure_rule_exact="$(count_exact_ip_rules_at_priority "$ensure_rule_priority" "$ensure_rule_needle")"
        if [ "$ensure_rule_count" = "1" ] && [ "$ensure_rule_exact" = "1" ]; then
            log "ok: ip rule priority $ensure_rule_priority $ensure_rule_needle"
            return 0
        fi
    fi

    delete_ip_rules_at_priority "$ensure_rule_priority"
    run ip rule add "$@" priority "$ensure_rule_priority"
}

cleanup_bridge_forward_rules() {
    [ "$DRY_RUN" = "1" ] && return 0
    "$IPTABLES_BIN" -S FORWARD 2>/dev/null | while IFS= read -r cleanup_rule; do
        set -- $cleanup_rule
        [ "${1:-}" = "-A" ] && [ "${2:-}" = "FORWARD" ] || continue
        cleanup_in=""
        cleanup_out=""
        cleanup_jump=""
        while [ "$#" -gt 0 ]; do
            case "$1" in
                -i)
                    shift
                    cleanup_in="${1:-}"
                    ;;
                -o)
                    shift
                    cleanup_out="${1:-}"
                    ;;
                -j)
                    shift
                    cleanup_jump="${1:-}"
                    ;;
            esac
            shift || break
        done
        [ "$cleanup_in" = "$CONTAINER_BRIDGE" ] || continue
        [ "$cleanup_jump" = "ACCEPT" ] || continue
        [ -n "$cleanup_out" ] || continue
        while "$IPTABLES_BIN" -D FORWARD -i "$CONTAINER_BRIDGE" -o "$cleanup_out" -j ACCEPT >/dev/null 2>&1; do
            :
        done
    done
}

ensure_policy_rules() {
    ensure_policy_subnet="$1"
    ensure_policy_uplink="$2"

    [ "$POLICY_RULES" = "1" ] || return 0
    ensure_ip_rule "$RETURN_RULE_PRIORITY" "to $ensure_policy_subnet lookup main" to "$ensure_policy_subnet" lookup main
    ensure_ip_rule "$SOURCE_RULE_PRIORITY" "from $ensure_policy_subnet lookup $ensure_policy_uplink" from "$ensure_policy_subnet" lookup "$ensure_policy_uplink"
}

have ip || fail "ip command not found"
require_bridge
IPTABLES_BIN="$(pick_iptables)" || fail "iptables command not found"
UPLINK_RESOLVED="$(resolve_uplink)"
[ -n "$UPLINK_RESOLVED" ] || fail "cannot detect uplink interface; set UPLINK"
DOCKER_SUBNET_RESOLVED="$(resolve_docker_subnet)"

log "container_bridge=$CONTAINER_BRIDGE"
log "docker_bridge=$CONTAINER_BRIDGE"
log "docker_subnet=$DOCKER_SUBNET_RESOLVED"
log "uplink=$UPLINK_RESOLVED"
log "iptables=$IPTABLES_BIN"
log "policy_rules=$POLICY_RULES return_rule_priority=$RETURN_RULE_PRIORITY source_rule_priority=$SOURCE_RULE_PRIORITY"

set_sysctl_value net.ipv4.ip_forward 1
set_sysctl_value net.ipv6.conf.all.forwarding 1
ensure_policy_rules "$DOCKER_SUBNET_RESOLVED" "$UPLINK_RESOLVED"
cleanup_bridge_forward_rules

ensure_rule filter FORWARD -i "$CONTAINER_BRIDGE" -o "$UPLINK_RESOLVED" -j ACCEPT
ensure_rule filter FORWARD -o "$CONTAINER_BRIDGE" -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
ensure_rule nat POSTROUTING -s "$DOCKER_SUBNET_RESOLVED" ! -o "$CONTAINER_BRIDGE" -j MASQUERADE

log "container NAT reconciliation complete"
