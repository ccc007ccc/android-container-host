#!/system/bin/sh
set -u

OOM_SCORE_ADJ="${OOM_SCORE_ADJ:--900}"
PROTECT_SHIMS="${PROTECT_SHIMS:-0}"
DRY_RUN="${ACHOST_DRY_RUN:-0}"
DAEMONS="dockerd containerd"
[ "$PROTECT_SHIMS" = "1" ] && DAEMONS="$DAEMONS containerd-shim containerd-shim-runc-v2"

log() {
    printf '%s\n' "$*"
}

write_oom_score_adj() {
    pid="$1"
    name="$2"
    path="/proc/$pid/oom_score_adj"
    score_path="/proc/$pid/oom_score"

    if [ ! -w "$path" ]; then
        log "skip $name pid=$pid: $path not writable"
        return 0
    fi

    old="$(cat "$path" 2>/dev/null || echo unknown)"
    score="$(cat "$score_path" 2>/dev/null || echo unknown)"
    if [ "$DRY_RUN" = "1" ]; then
        log "+ echo $OOM_SCORE_ADJ > $path (old=$old oom_score=$score name=$name)"
    else
        printf '%s\n' "$OOM_SCORE_ADJ" > "$path"
        new="$(cat "$path" 2>/dev/null || echo unknown)"
        log "protected $name pid=$pid oom_score_adj $old->$new oom_score=$score"
    fi
}

pids_for_name() {
    name="$1"
    if command -v pidof >/dev/null 2>&1; then
        pidof "$name" 2>/dev/null || true
        return 0
    fi
    for comm in /proc/[0-9]*/comm; do
        [ -r "$comm" ] || continue
        if [ "$(cat "$comm" 2>/dev/null)" = "$name" ]; then
            pid="${comm%/comm}"
            echo "${pid##*/}"
        fi
    done
}

log "oom_score_adj_target=$OOM_SCORE_ADJ"
log "protect_shims=$PROTECT_SHIMS"

for daemon in $DAEMONS; do
    pids="$(pids_for_name "$daemon")"
    if [ -z "$pids" ]; then
        log "not running: $daemon"
        continue
    fi
    for pid in $pids; do
        write_oom_score_adj "$pid" "$daemon"
    done
done

if command -v getprop >/dev/null 2>&1; then
    log "ro.lmk.use_psi=$(getprop ro.lmk.use_psi 2>/dev/null)"
    log "ro.lmk.debug=$(getprop ro.lmk.debug 2>/dev/null)"
fi

if [ -r /proc/pressure/memory ]; then
    log "memory pressure:"
    cat /proc/pressure/memory 2>/dev/null || true
fi
