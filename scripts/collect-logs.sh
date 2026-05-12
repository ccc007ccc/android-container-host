#!/system/bin/sh
set -u

DEVICE="${DEVICE:-android}"
OUT_DIR="${OUT_DIR:-/data/local/tmp}"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
    ACHOST_BIN="$SCRIPT_DIR"
    . "$SCRIPT_DIR/achost-container-env.sh"
else
    ACHOST="${ACHOST:-/data/adb/achost}"
fi
ACHOST_NET_LOG="${ACHOST_NET_LOG:-/data/local/tmp/achost-network-watchdog.log}"
ACHOST_NET_PID="${ACHOST_NET_PID:-/data/local/tmp/achost-network-watchdog.pid}"
STAMP="$(date +%Y%m%d-%H%M%S 2>/dev/null || echo now)"
WORK_DIR="$OUT_DIR/achost-debug-$DEVICE-$STAMP"
ARCHIVE="$OUT_DIR/achost-debug-$DEVICE-$STAMP.tar.gz"

mkdir -p "$WORK_DIR"

section_file() {
    name="$1"
    shift
    {
        printf '## %s\n' "$name"
        "$@" 2>&1 || true
    } > "$WORK_DIR/$name.txt"
}

shell_file() {
    name="$1"
    shift
    {
        printf '## %s\n' "$name"
        sh -c "$*" 2>&1 || true
    } > "$WORK_DIR/$name.txt"
}

section_file uname uname -a
section_file proc_version cat /proc/version
shell_file proc_config 'zcat /proc/config.gz 2>/dev/null || cat /proc/config.gz 2>/dev/null'
section_file cmdline cat /proc/cmdline
section_file mount mount
section_file proc_mounts cat /proc/mounts
shell_file cgroup_mounts 'grep cgroup /proc/mounts 2>/dev/null'
section_file proc_cgroups cat /proc/cgroups
shell_file sys_fs_cgroup 'find /sys/fs/cgroup -maxdepth 4 2>/dev/null'
shell_file dev_cgroup_cpuset "find /dev -maxdepth 3 \( -name '*cgroup*' -o -name '*cpuset*' \) 2>/dev/null"
section_file ip_addr ip addr
section_file ip_route ip route
section_file ip_rule ip rule
shell_file iptables_filter 'iptables -S 2>/dev/null'
shell_file iptables_nat 'iptables -t nat -S 2>/dev/null'
shell_file iptables_mangle 'iptables -t mangle -S 2>/dev/null'
shell_file ip_forward 'cat /proc/sys/net/ipv4/ip_forward 2>/dev/null'
shell_file ipv6_forwarding 'cat /proc/sys/net/ipv6/conf/all/forwarding 2>/dev/null'
shell_file achost_network_watchdog "if [ -r '$ACHOST_NET_PID' ]; then pid=\$(cat '$ACHOST_NET_PID' 2>/dev/null); printf 'pid=%s\\n' \"\$pid\"; if kill -0 \"\$pid\" 2>/dev/null; then printf 'running=1\\n'; else printf 'running=0\\n'; fi; else printf 'pid file not found: %s\\n' '$ACHOST_NET_PID'; fi; { ps -A 2>/dev/null || ps 2>/dev/null; } | grep '[c]ontainer-network-watchdog' || true; printf '\\n## watchdog_log\\n'; if [ -r '$ACHOST_NET_LOG' ]; then tail -n 200 '$ACHOST_NET_LOG'; else printf 'watchdog log not found: %s\\n' '$ACHOST_NET_LOG'; fi"
shell_file achost_container_validate "if [ -x '$ACHOST/bin/achost-container-validate.sh' ]; then '$ACHOST/bin/achost-container-validate.sh'; else printf 'validation script not found: %s\\n' '$ACHOST/bin/achost-container-validate.sh'; fi"
shell_file achost_daemon_logs "for file in '$ACHOST/var/log/dockerd.log' '$ACHOST/var/log/containerd.log' '$ACHOST/var/log/achost-supervise.log'; do printf '## %s\\n' \"\$file\"; if [ -r \"\$file\" ]; then tail -n 200 \"\$file\"; else printf 'not found\\n'; fi; done"
shell_file docker_info 'docker info 2>/dev/null'
shell_file docker_version 'docker version 2>/dev/null'
shell_file docker_ps 'docker ps -a 2>/dev/null'
shell_file docker_bridge 'docker network inspect bridge 2>/dev/null'
shell_file lxc_checkconfig 'lxc-checkconfig 2>/dev/null'
shell_file lxc_ls 'lxc-ls -f 2>/dev/null'
shell_file dmesg 'dmesg 2>/dev/null'
shell_file logcat_all 'logcat -b all -d 2>/dev/null'
shell_file getprop 'getprop 2>/dev/null'
shell_file getenforce 'getenforce 2>/dev/null'
shell_file lmkd_logs "logcat -b all -d 2>/dev/null | grep -iE 'lmkd|lowmemory|kill.*dockerd|kill.*containerd|kill.*runc'"
shell_file oom_logs "dmesg 2>/dev/null | grep -iE 'oom|killed process|out of memory|lowmemory'"
shell_file avc_logs "{ dmesg 2>/dev/null; logcat -b all -d 2>/dev/null; } | grep -i avc"

if command -v tar >/dev/null 2>&1; then
    tar -czf "$ARCHIVE" -C "$OUT_DIR" "$(basename "$WORK_DIR")" 2>/dev/null || true
    if [ -f "$ARCHIVE" ]; then
        printf '%s\n' "$ARCHIVE"
        exit 0
    fi
fi

printf '%s\n' "$WORK_DIR"
