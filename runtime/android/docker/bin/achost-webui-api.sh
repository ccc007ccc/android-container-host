#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
fi
DOCKER="$ACHOST_BIN/docker"
TAB="$(printf '\t')"

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s//\\r/g; s/	/\\t/g' | tr '
' ' '
}

json_string() {
    printf '"%s"' "$(json_escape "$1")"
}

json_bool() {
    if [ "$1" = "1" ]; then
        printf 'true'
    else
        printf 'false'
    fi
}

json_error() {
    printf '{"ok":false,"error":'
    json_string "$1"
    printf '}\n'
}

pid_value() {
    file="$1"
    [ -r "$file" ] || return 0
    pid="$(cat "$file" 2>/dev/null || true)"
    case "$pid" in
        ''|*[!0-9]*) return 0 ;;
    esac
    if kill -0 "$pid" 2>/dev/null; then
        printf '%s' "$pid"
    fi
}

count_lines() {
    wc -l | tr -d ' '
}

valid_name() {
    case "$1" in
        ''|*[!A-Za-z0-9_.-]*) return 1 ;;
        *) return 0 ;;
    esac
}

valid_image() {
    case "$1" in
        ''|*[!A-Za-z0-9_./:@-]*) return 1 ;;
        *) return 0 ;;
    esac
}

socket_present() {
    [ -S "${DOCKER_HOST#unix://}" ] || [ -S "$ACHOST_RUN/docker.sock" ]
}

status_json() {
    dockerd_pid="$(pid_value "$ACHOST_DOCKERD_PID")"
    containerd_pid="$(pid_value "$ACHOST_CONTAINERD_PID")"
    socket=0
    socket_present && socket=1
    running=0
    [ -n "$dockerd_pid" ] && [ "$socket" = "1" ] && running=1
    server_version=""
    cgroup_version=""
    storage_driver=""
    total=0
    running_count=0
    images=0
    docker_error=""

    if [ "$running" = "1" ] && [ -x "$DOCKER" ]; then
        info="$($DOCKER info --format '{{.ServerVersion}}	{{.CgroupVersion}}	{{.Driver}}' 2>&1)" || {
            docker_error="$info"
            info=""
        }
        if [ -n "$info" ]; then
            old_ifs="$IFS"
            IFS="	"
            set -- $info
            IFS="$old_ifs"
            server_version="${1:-}"
            cgroup_version="${2:-}"
            storage_driver="${3:-}"
        fi
        total="$($DOCKER ps -aq 2>/dev/null | count_lines)"
        running_count="$($DOCKER ps -q 2>/dev/null | count_lines)"
        images="$($DOCKER images -q 2>/dev/null | sort -u | count_lines)"
    fi
    stopped=$((total - running_count))

    printf '{"ok":true,"running":'
    json_bool "$running"
    printf ',"status":'
    if [ "$running" = "1" ]; then json_string "running"; else json_string "stopped"; fi
    printf ',"socket":'
    json_bool "$socket"
    printf ',"dockerd_pid":'
    json_string "$dockerd_pid"
    printf ',"containerd_pid":'
    json_string "$containerd_pid"
    printf ',"cgroup_version":'
    json_string "$cgroup_version"
    printf ',"storage_driver":'
    json_string "$storage_driver"
    printf ',"server_version":'
    json_string "$server_version"
    printf ',"containers_total":%s,"containers_running":%s,"containers_stopped":%s,"images":%s' "$total" "$running_count" "$stopped" "$images"
    if [ -n "$docker_error" ]; then
        printf ',"docker_error":'
        json_string "$docker_error"
    fi
    printf '}\n'
}

list_containers() {
    if [ ! -x "$DOCKER" ]; then
        json_error "docker binary not found"
        return 0
    fi
    if ! socket_present; then
        printf '{"ok":true,"containers":[]}\n'
        return 0
    fi
    printf '{"ok":true,"containers":['
    first=1
    $DOCKER ps -a --no-trunc --format '{{.ID}}	{{.Names}}	{{.Image}}	{{.Status}}	{{.CreatedAt}}' 2>/dev/null | while IFS="	" read -r id name image status created; do
        [ -n "$id" ] || continue
        if [ "$first" = "1" ]; then first=0; else printf ','; fi
        printf '{"id":'; json_string "$id"
        printf ',"name":'; json_string "$name"
        printf ',"image":'; json_string "$image"
        printf ',"status":'; json_string "$status"
        printf ',"created":'; json_string "$created"
        printf '}'
    done
    printf ']}\n'
}

run_and_report() {
    label="$1"
    shift
    output="$("$@" 2>&1)"
    rc=$?
    printf '{"ok":'
    if [ "$rc" -eq 0 ]; then printf 'true'; else printf 'false'; fi
    printf ',"action":'
    json_string "$label"
    printf ',"rc":%s,"output":' "$rc"
    json_string "$output"
    printf '}\n'
}

case "${1:-}" in
    status)
        status_json
        ;;
    check)
        output=""
        rc=0
        if [ -x "$ACHOST_BIN/achost-container-validate.sh" ]; then
            validate_output="$($ACHOST_BIN/achost-container-validate.sh 2>&1)" || rc=$?
            output="$output$validate_output"
        fi
        if [ -x "$DOCKER" ] && socket_present; then
            info_output="$($DOCKER info 2>&1)" || rc=$?
            output="$output
$info_output"
        fi
        printf '{"ok":'
        if [ "$rc" -eq 0 ]; then printf 'true'; else printf 'false'; fi
        printf ',"rc":%s,"output":' "$rc"
        json_string "$output"
        printf '}\n'
        ;;
    start-docker)
        run_and_report start-docker "$ACHOST_BIN/achost-docker-start.sh"
        ;;
    stop-docker)
        run_and_report stop-docker "$ACHOST_BIN/achost-docker-stop.sh"
        ;;
    list-containers)
        list_containers
        ;;
    add-container)
        name="${2:-}"
        image="${3:-}"
        valid_name "$name" || { json_error "invalid container name"; exit 0; }
        valid_image "$image" || { json_error "invalid image name"; exit 0; }
        run_and_report add-container "$DOCKER" run -d --name "$name" --network bridge "$image"
        ;;
    delete-container)
        target="${2:-}"
        valid_name "$target" || { json_error "invalid container id or name"; exit 0; }
        run_and_report delete-container "$DOCKER" rm -f "$target"
        ;;
    *)
        json_error "unsupported command"
        ;;
esac
