#!/system/bin/sh
set -u

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ACHOST="${ACHOST:-$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)}"
ACHOST_BIN="$SCRIPT_DIR"
BASE_ENV_PRESENT=0
if [ -r "$SCRIPT_DIR/achost-container-env.sh" ]; then
    . "$SCRIPT_DIR/achost-container-env.sh"
    BASE_ENV_PRESENT=1
elif [ -r "/data/adb/modules/achost-base/achost/bin/achost-container-env.sh" ]; then
    ACHOST_BASE="${ACHOST_BASE:-/data/adb/modules/achost-base/achost}"
    . "$ACHOST_BASE/bin/achost-container-env.sh"
    BASE_ENV_PRESENT=1
fi
ACHOST_VAR="${ACHOST_VAR:-$ACHOST/var}"
ACHOST_RUN="${ACHOST_RUN:-$ACHOST_VAR/run}"
ACHOST_LOG_DIR="${ACHOST_LOG_DIR:-$ACHOST_VAR/log}"
ACHOST_DOCKERD_PID="${ACHOST_DOCKERD_PID:-$ACHOST_RUN/dockerd.pid}"
ACHOST_CONTAINERD_PID="${ACHOST_CONTAINERD_PID:-$ACHOST_RUN/containerd.pid}"
ACHOST_DOCKERD_LOG="${ACHOST_DOCKERD_LOG:-$ACHOST_LOG_DIR/dockerd.log}"
ACHOST_CONTAINERD_LOG="${ACHOST_CONTAINERD_LOG:-$ACHOST_LOG_DIR/containerd.log}"
ACHOST_SUPERVISOR_LOG="${ACHOST_SUPERVISOR_LOG:-$ACHOST_LOG_DIR/achost-supervise.log}"
DOCKER_HOST="${DOCKER_HOST:-unix://$ACHOST_RUN/docker.sock}"
ACHOST_COMMON="${ACHOST_COMMON:-$ACHOST}"
ACHOST_COMMON_BIN="${ACHOST_COMMON_BIN:-$ACHOST_COMMON/bin}"
DOCKER="$ACHOST_BIN/docker"
FIELD_SEP="|"
ACHOST_CONFIG="${ACHOST_CONFIG:-$ACHOST_VAR/config}"
AUTOSTART_FILE="$ACHOST_CONFIG/docker.autostart"

json_escape() {
    printf '%s' "$1" | LC_ALL=C tr -d '\001-\010\013\014\016-\037' | sed 's/\\/\\\\/g; s/"/\\"/g' | tr '\n' ' '
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

valid_network() {
    case "$1" in
        ''|bridge|host|none) return 0 ;;
        *) valid_name "$1" ;;
    esac
}

valid_port_item() {
    case "$1" in
        ''|*[!0-9:/a-z.-]*) return 1 ;;
        *:*) return 0 ;;
        *) return 1 ;;
    esac
}

valid_env_item() {
    case "$1" in
        ''|[!A-Za-z_]*|*[!A-Za-z0-9_=@.,:/+-]*) return 1 ;;
        *=*) return 0 ;;
        *) return 1 ;;
    esac
}

valid_mount_item() {
    case "$1" in
        /*:/*) ;;
        *) return 1 ;;
    esac
    case "$1" in
        *[!A-Za-z0-9_./:@,+=-]*) return 1 ;;
        *) return 0 ;;
    esac
}

valid_csv() {
    values="$1"
    validator="$2"
    [ -n "$values" ] || return 0
    old_ifs="$IFS"
    IFS=','
    set -- $values
    IFS="$old_ifs"
    for item in "$@"; do
        "$validator" "$item" || return 1
    done
    return 0
}

socket_present() {
    [ -S "${DOCKER_HOST#unix://}" ] || [ -S "$ACHOST_RUN/docker.sock" ]
}

ensure_config_dir() {
    mkdir -p "$ACHOST_CONFIG" 2>/dev/null || true
}

autostart_value() {
    value="$(cat "$AUTOSTART_FILE" 2>/dev/null || printf '0')"
    if [ "$value" = "1" ]; then
        printf '1'
    else
        printf '0'
    fi
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
    autostart="$(autostart_value)"
    base_present=0
    [ "$BASE_ENV_PRESENT" = "1" ] && base_present=1

    if [ "$running" = "1" ] && [ -x "$DOCKER" ]; then
        info="$($DOCKER info --format '{{.ServerVersion}}|{{.CgroupVersion}}|{{.Driver}}' 2>&1)" || {
            docker_error="$info"
            info=""
        }
        if [ -n "$info" ]; then
            old_ifs="$IFS"
            IFS="$FIELD_SEP"
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
    printf ',"autostart":'
    json_bool "$autostart"
    printf ',"base_present":'
    json_bool "$base_present"
    printf ',"data_root":'
    json_string "${ACHOST_VAR:-}"
    printf ',"autostart_file":'
    json_string "$AUTOSTART_FILE"
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

settings_json() {
    printf '{"ok":true,"autostart":'
    json_bool "$(autostart_value)"
    printf ',"autostart_file":'
    json_string "$AUTOSTART_FILE"
    printf ',"data_root":'
    json_string "${ACHOST_VAR:-}"
    printf ',"module_root":'
    json_string "$ACHOST"
    printf ',"base_root":'
    json_string "${ACHOST_COMMON:-$ACHOST}"
    printf ',"dockerd_log":'
    json_string "${ACHOST_DOCKERD_LOG:-}"
    printf ',"containerd_log":'
    json_string "${ACHOST_CONTAINERD_LOG:-}"
    printf ',"supervisor_log":'
    json_string "${ACHOST_SUPERVISOR_LOG:-}"
    printf '}\n'
}

set_autostart() {
    case "$1" in
        on|1|true) value=1 ;;
        off|0|false) value=0 ;;
        *) json_error "invalid autostart value"; return 0 ;;
    esac
    ensure_config_dir
    printf '%s\n' "$value" > "$AUTOSTART_FILE" 2>/dev/null || { json_error "could not write autostart setting"; return 0; }
    printf '{"ok":true,"autostart":'
    json_bool "$value"
    printf ',"autostart_file":'
    json_string "$AUTOSTART_FILE"
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
    $DOCKER ps -a --no-trunc --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}|{{.CreatedAt}}' 2>/dev/null | while IFS="$FIELD_SEP" read -r id name image status created; do
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

list_images() {
    if [ ! -x "$DOCKER" ]; then
        json_error "docker binary not found"
        return 0
    fi
    if ! socket_present; then
        printf '{"ok":true,"images":[]}\n'
        return 0
    fi
    printf '{"ok":true,"images":['
    first=1
    $DOCKER images --no-trunc --format '{{.Repository}}|{{.Tag}}|{{.ID}}|{{.Size}}|{{.CreatedSince}}' 2>/dev/null | while IFS="$FIELD_SEP" read -r repository tag id size created; do
        [ -n "$id" ] || continue
        if [ "$first" = "1" ]; then first=0; else printf ','; fi
        printf '{"repository":'; json_string "$repository"
        printf ',"tag":'; json_string "$tag"
        printf ',"id":'; json_string "$id"
        printf ',"size":'; json_string "$size"
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

run_docker_target() {
    action="$1"
    target="$2"
    valid_name "$target" || { json_error "invalid container id or name"; return 0; }
    run_and_report "$action" "$DOCKER" "$action" "$target"
}

add_container() {
    name="$1"
    image="$2"
    ports="${3:-}"
    envs="${4:-}"
    mounts="${5:-}"
    network="${6:-bridge}"
    valid_name "$name" || { json_error "invalid container name"; return 0; }
    valid_image "$image" || { json_error "invalid image name"; return 0; }
    valid_network "$network" || { json_error "invalid network"; return 0; }
    valid_csv "$ports" valid_port_item || { json_error "invalid port mapping"; return 0; }
    valid_csv "$envs" valid_env_item || { json_error "invalid environment entry"; return 0; }
    valid_csv "$mounts" valid_mount_item || { json_error "invalid bind mount"; return 0; }

    set -- run -d --name "$name"
    [ -z "$network" ] || set -- "$@" --network "$network"
    old_ifs="$IFS"
    IFS=','
    for item in $ports; do
        [ -n "$item" ] && set -- "$@" -p "$item"
    done
    for item in $envs; do
        [ -n "$item" ] && set -- "$@" -e "$item"
    done
    for item in $mounts; do
        [ -n "$item" ] && set -- "$@" -v "$item"
    done
    IFS="$old_ifs"
    set -- "$@" "$image"
    run_and_report add-container "$DOCKER" "$@"
}

file_tail() {
    label="$1"
    shift
    output=""
    for file in "$@"; do
        [ -r "$file" ] || continue
        chunk="$(tail -n 160 "$file" 2>&1 || true)"
        output="$output
== $label: $file ==
$chunk"
    done
    printf '{"ok":true,"action":'
    json_string "$label"
    printf ',"output":'
    json_string "$output"
    printf '}\n'
}

case "${1:-}" in
    status)
        status_json
        ;;
    settings)
        settings_json
        ;;
    set-autostart)
        set_autostart "${2:-}"
        ;;
    check)
        output=""
        rc=0
        if [ -x "${ACHOST_COMMON_BIN:-$ACHOST_BIN}/achost-container-validate.sh" ]; then
            validate_output="$(${ACHOST_COMMON_BIN:-$ACHOST_BIN}/achost-container-validate.sh 2>&1)" || rc=$?
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
        add_container "${2:-}" "${3:-}" "${4:-}" "${5:-}" "${6:-}" "${7:-bridge}"
        ;;
    delete-container)
        target="${2:-}"
        valid_name "$target" || { json_error "invalid container id or name"; exit 0; }
        run_and_report delete-container "$DOCKER" rm -f "$target"
        ;;
    start-container|stop-container|restart-container)
        action="${1%-container}"
        run_docker_target "$action" "${2:-}"
        ;;
    container-logs)
        target="${2:-}"
        valid_name "$target" || { json_error "invalid container id or name"; exit 0; }
        run_and_report container-logs "$DOCKER" logs --tail 200 "$target"
        ;;
    inspect-container)
        target="${2:-}"
        valid_name "$target" || { json_error "invalid container id or name"; exit 0; }
        run_and_report inspect-container "$DOCKER" inspect "$target"
        ;;
    list-images)
        list_images
        ;;
    pull-image)
        image="${2:-}"
        valid_image "$image" || { json_error "invalid image name"; exit 0; }
        run_and_report pull-image "$DOCKER" pull "$image"
        ;;
    remove-image)
        image="${2:-}"
        valid_image "$image" || { json_error "invalid image id or name"; exit 0; }
        run_and_report remove-image "$DOCKER" rmi "$image"
        ;;
    daemon-logs)
        file_tail daemon-logs "${ACHOST_DOCKERD_LOG:-}" "${ACHOST_CONTAINERD_LOG:-}" "${ACHOST_SUPERVISOR_LOG:-}"
        ;;
    *)
        json_error "unsupported command"
        ;;
esac
