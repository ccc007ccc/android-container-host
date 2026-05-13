use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
struct RuntimeEnv {
    achost: PathBuf,
    bin: PathBuf,
    var: PathBuf,
    run: PathBuf,
    config: PathBuf,
    dockerd_pid: PathBuf,
    containerd_pid: PathBuf,
    dockerd_log: PathBuf,
    containerd_log: PathBuf,
    supervisor_log: PathBuf,
    docker_host: String,
    docker: PathBuf,
    common_bin: PathBuf,
    autostart_file: PathBuf,
    runtime_mode: String,
    cgroup_mode: String,
    use_chroot: String,
    chroot: PathBuf,
    native_root: PathBuf,
    dns_servers: String,
    bridge: String,
    return_rule_priority: String,
    source_rule_priority: String,
    base_present: bool,
}

#[derive(Debug)]
struct CommandResult {
    ok: bool,
    rc: i32,
    output: String,
}

#[derive(Serialize)]
struct ContainerItem {
    id: String,
    name: String,
    image: String,
    status: String,
    created: String,
}

#[derive(Serialize)]
struct ImageItem {
    repository: String,
    tag: String,
    id: String,
    size: String,
    created: String,
}

impl RuntimeEnv {
    fn from_env() -> Self {
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let achost = env_path("ACHOST").unwrap_or_else(|| {
            exe_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/data/adb/achost"))
        });
        let bin = env_path("ACHOST_BIN").unwrap_or_else(|| exe_dir.clone());
        let var = env_path("ACHOST_VAR").unwrap_or_else(|| achost.join("var"));
        let run = env_path("ACHOST_RUN").unwrap_or_else(|| var.join("run"));
        let log_dir = env_path("ACHOST_LOG_DIR").unwrap_or_else(|| var.join("log"));
        let config = env_path("ACHOST_CONFIG").unwrap_or_else(|| var.join("config"));
        let docker_host = env::var("DOCKER_HOST")
            .unwrap_or_else(|_| format!("unix://{}", run.join("docker.sock").display()));
        let common = env_path("ACHOST_COMMON").unwrap_or_else(|| achost.clone());
        let common_bin = env_path("ACHOST_COMMON_BIN").unwrap_or_else(|| common.join("bin"));
        let use_chroot = env::var("ACHOST_USE_CHROOT").unwrap_or_else(|_| "0".to_string());
        Self {
            dockerd_pid: env_path("ACHOST_DOCKERD_PID").unwrap_or_else(|| run.join("dockerd.pid")),
            containerd_pid: env_path("ACHOST_CONTAINERD_PID")
                .unwrap_or_else(|| run.join("containerd.pid")),
            dockerd_log: env_path("ACHOST_DOCKERD_LOG")
                .unwrap_or_else(|| log_dir.join("dockerd.log")),
            containerd_log: env_path("ACHOST_CONTAINERD_LOG")
                .unwrap_or_else(|| log_dir.join("containerd.log")),
            supervisor_log: env_path("ACHOST_SUPERVISOR_LOG")
                .unwrap_or_else(|| log_dir.join("achost-supervise.log")),
            docker: bin.join("docker"),
            autostart_file: config.join("docker.autostart"),
            runtime_mode: env::var("ACHOST_RUNTIME_MODE").unwrap_or_default(),
            cgroup_mode: env::var("ACHOST_CGROUP_MODE").unwrap_or_default(),
            chroot: env_path("ACHOST_CHROOT").unwrap_or_else(|| var.join("chroot")),
            native_root: env_path("ACHOST_NATIVE_ROOT").unwrap_or_else(|| var.join("native-root")),
            dns_servers: env::var("ACHOST_DNS_SERVERS").unwrap_or_default(),
            bridge: env::var("CONTAINER_BRIDGE")
                .or_else(|_| env::var("DOCKER_BRIDGE"))
                .unwrap_or_else(|_| "docker0".to_string()),
            return_rule_priority: valid_priority(
                env::var("ACHOST_CONTAINER_RETURN_RULE_PRIORITY")
                    .unwrap_or_else(|_| "11999".to_string()),
                "11999",
            ),
            source_rule_priority: valid_priority(
                env::var("ACHOST_CONTAINER_SOURCE_RULE_PRIORITY")
                    .unwrap_or_else(|_| "12000".to_string()),
                "12000",
            ),
            base_present: env::var("ACHOST_BASE_ENV_PRESENT")
                .map(|value| value == "1")
                .unwrap_or(false),
            achost,
            bin,
            var,
            run,
            config,
            docker_host,
            common_bin,
            use_chroot,
        }
    }

    fn docker_socket_path(&self) -> PathBuf {
        self.docker_host
            .strip_prefix("unix://")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.run.join("docker.sock"))
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn valid_priority(value: String, default: &str) -> String {
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        value
    } else {
        default.to_string()
    }
}

fn main() {
    let env = RuntimeEnv::from_env();
    let args: Vec<String> = env::args().skip(1).collect();
    let response = dispatch(&env, &args);
    println!(
        "{}",
        serde_json::to_string(&response)
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"json encode failed\"}".to_string())
    );
}

fn dispatch(env: &RuntimeEnv, args: &[String]) -> Value {
    match args.first().map(String::as_str) {
        Some("status") => status_json(env),
        Some("settings") => settings_json(env),
        Some("set-autostart") => set_autostart(env, args.get(1).map(String::as_str).unwrap_or("")),
        Some("check") => check_json(env),
        Some("start-docker") => run_and_report(
            "start-docker",
            &env.bin.join("achost-docker-runtime"),
            &["start".to_string()],
        ),
        Some("stop-docker") => run_and_report(
            "stop-docker",
            &env.bin.join("achost-docker-runtime"),
            &["stop".to_string()],
        ),
        Some("list-containers") => list_containers(env),
        Some("add-container") => add_container(
            env,
            args.get(1).map(String::as_str).unwrap_or(""),
            args.get(2).map(String::as_str).unwrap_or(""),
            args.get(3).map(String::as_str).unwrap_or(""),
            args.get(4).map(String::as_str).unwrap_or(""),
            args.get(5).map(String::as_str).unwrap_or(""),
            args.get(6).map(String::as_str).unwrap_or("bridge"),
        ),
        Some("delete-container") => docker_target_action(
            env,
            "delete-container",
            &["rm", "-f"],
            args.get(1).map(String::as_str).unwrap_or(""),
        ),
        Some("start-container") => docker_target_action(
            env,
            "start",
            &["start"],
            args.get(1).map(String::as_str).unwrap_or(""),
        ),
        Some("stop-container") => docker_target_action(
            env,
            "stop",
            &["stop"],
            args.get(1).map(String::as_str).unwrap_or(""),
        ),
        Some("restart-container") => docker_target_action(
            env,
            "restart",
            &["restart"],
            args.get(1).map(String::as_str).unwrap_or(""),
        ),
        Some("container-logs") => {
            container_logs(env, args.get(1).map(String::as_str).unwrap_or(""))
        }
        Some("inspect-container") => {
            inspect_container(env, args.get(1).map(String::as_str).unwrap_or(""))
        }
        Some("list-images") => list_images(env),
        Some("pull-image") => pull_image(env, args.get(1).map(String::as_str).unwrap_or("")),
        Some("remove-image") => remove_image(env, args.get(1).map(String::as_str).unwrap_or("")),
        Some("daemon-logs") => daemon_logs(env),
        _ => error_json("unsupported command"),
    }
}

fn error_json(message: &str) -> Value {
    json!({"ok": false, "error": message})
}

fn action_json(label: &str, result: CommandResult) -> Value {
    json!({"ok": result.ok, "action": label, "rc": result.rc, "output": result.output})
}

fn run_and_report(label: &str, program: &Path, args: &[String]) -> Value {
    action_json(label, run_program(program, args))
}

fn run_program(program: &Path, args: &[String]) -> CommandResult {
    match Command::new(program).args(args).output() {
        Ok(output) => {
            let rc = output.status.code().unwrap_or(1);
            let mut text = String::new();
            text.push_str(&String::from_utf8_lossy(&output.stdout));
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            CommandResult {
                ok: output.status.success(),
                rc,
                output: trim_trailing_newlines(text),
            }
        }
        Err(error) => CommandResult {
            ok: false,
            rc: 1,
            output: error.to_string(),
        },
    }
}

fn run_program_capture(program: &Path, args: &[&str]) -> CommandResult {
    let owned = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    run_program(program, &owned)
}

fn trim_trailing_newlines(mut value: String) -> String {
    while value.ends_with('\n') || value.ends_with('\r') {
        value.pop();
    }
    value
}

fn pid_value(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let pid_text = text.trim();
    if pid_text.is_empty() || !pid_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return String::new();
    }
    let Ok(pid) = pid_text.parse::<libc::pid_t>() else {
        return String::new();
    };
    if unsafe { libc::kill(pid, 0) } == 0 {
        pid_text.to_string()
    } else {
        String::new()
    }
}

fn socket_present(env: &RuntimeEnv) -> bool {
    is_socket(&env.docker_socket_path()) || is_socket(&env.run.join("docker.sock"))
}

fn is_socket(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

fn autostart_value(env: &RuntimeEnv) -> bool {
    fs::read_to_string(&env.autostart_file)
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

fn status_json(env: &RuntimeEnv) -> Value {
    let dockerd_pid = pid_value(&env.dockerd_pid);
    let containerd_pid = pid_value(&env.containerd_pid);
    let socket = socket_present(env);
    let running = !dockerd_pid.is_empty() && socket;
    let mut server_version = String::new();
    let mut cgroup_version = String::new();
    let mut storage_driver = String::new();
    let mut docker_error = None;
    let mut total = 0usize;
    let mut running_count = 0usize;
    let mut images = 0usize;

    if running && env.docker.exists() {
        let info = run_program_capture(
            &env.docker,
            &[
                "info",
                "--format",
                "{{.ServerVersion}}|{{.CgroupVersion}}|{{.Driver}}",
            ],
        );
        if info.ok && !info.output.is_empty() {
            let fields = info.output.split('|').collect::<Vec<_>>();
            server_version = fields.first().copied().unwrap_or_default().to_string();
            cgroup_version = fields.get(1).copied().unwrap_or_default().to_string();
            storage_driver = fields.get(2).copied().unwrap_or_default().to_string();
        } else if !info.output.is_empty() {
            docker_error = Some(info.output);
        }
        total = count_docker_lines(env, &["ps", "-aq"]);
        running_count = count_docker_lines(env, &["ps", "-q"]);
        images = count_unique_docker_lines(env, &["images", "-q"]);
    }

    let bridge_route = bridge_route_value(env);
    let bridge_subnet = bridge_subnet_value(&bridge_route);
    let return_policy_rule = if bridge_subnet.is_empty() {
        String::new()
    } else {
        ip_rule_value(
            &env.return_rule_priority,
            &format!("to {bridge_subnet} lookup main"),
        )
    };
    let source_policy_rule = if bridge_subnet.is_empty() {
        String::new()
    } else {
        ip_rule_value(
            &env.source_rule_priority,
            &format!("from {bridge_subnet} lookup"),
        )
    };
    let route_status = if bridge_route.is_empty() {
        "missing-bridge"
    } else if return_policy_rule.is_empty() {
        "missing-policy"
    } else {
        "ok"
    };
    let stopped = total.saturating_sub(running_count);
    let mut value = json!({
        "ok": true,
        "running": running,
        "status": if running { "running" } else { "stopped" },
        "socket": socket,
        "autostart": autostart_value(env),
        "base_present": env.base_present,
        "data_root": path_string(&env.var),
        "autostart_file": path_string(&env.autostart_file),
        "dockerd_pid": dockerd_pid,
        "containerd_pid": containerd_pid,
        "cgroup_version": cgroup_version,
        "configured_cgroup_mode": env.cgroup_mode,
        "cgroup_mount": cgroup_mount_value(),
        "runtime_mode": env.runtime_mode,
        "dns_servers": env.dns_servers,
        "resolv_conf": path_string(&runtime_resolv_conf_path(env)),
        "resolv_nameservers": resolv_conf_nameservers(&runtime_resolv_conf_path(env)),
        "bridge": env.bridge,
        "bridge_subnet": bridge_subnet,
        "bridge_route": bridge_route,
        "route_status": route_status,
        "return_policy_rule": return_policy_rule,
        "source_policy_rule": source_policy_rule,
        "uplink": uplink_value(env),
        "storage_driver": storage_driver,
        "server_version": server_version,
        "containers_total": total,
        "containers_running": running_count,
        "containers_stopped": stopped,
        "images": images,
    });
    if let Some(error) = docker_error {
        value["docker_error"] = json!(error);
    }
    value
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn count_docker_lines(env: &RuntimeEnv, args: &[&str]) -> usize {
    let result = run_program_capture(&env.docker, args);
    if result.ok {
        nonempty_lines(&result.output).count()
    } else {
        0
    }
}

fn count_unique_docker_lines(env: &RuntimeEnv, args: &[&str]) -> usize {
    let result = run_program_capture(&env.docker, args);
    if !result.ok {
        return 0;
    }
    nonempty_lines(&result.output).collect::<HashSet<_>>().len()
}

fn nonempty_lines(value: &str) -> impl Iterator<Item = &str> {
    value.lines().filter(|line| !line.trim().is_empty())
}

fn cgroup_mount_value() -> String {
    for path in ["/dev/memcg", "/sys/fs/cgroup/memory", "/sys/fs/cgroup"] {
        if Path::new(path).is_dir() {
            return path.to_string();
        }
    }
    String::new()
}

fn runtime_resolv_conf_path(env: &RuntimeEnv) -> PathBuf {
    if env.use_chroot == "1" {
        env.chroot.join("etc/resolv.conf")
    } else {
        env.native_root.join("etc/resolv.conf")
    }
}

fn resolv_conf_nameservers(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some("nameserver"), Some(server)) => Some(server.to_string()),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn bridge_route_value(env: &RuntimeEnv) -> String {
    let result = run_program_capture(
        Path::new("ip"),
        &["-4", "route", "show", "dev", &env.bridge, "scope", "link"],
    );
    if !result.ok {
        return String::new();
    }
    result
        .output
        .lines()
        .find(|line| {
            line.as_bytes()
                .first()
                .map(|byte| byte.is_ascii_digit())
                .unwrap_or(false)
                && line.contains('/')
        })
        .unwrap_or_default()
        .to_string()
}

fn bridge_subnet_value(route: &str) -> String {
    route
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn ip_rule_value(priority: &str, needle: &str) -> String {
    if needle.is_empty() {
        return String::new();
    }
    let result = run_program_capture(Path::new("ip"), &["rule", "show"]);
    if !result.ok {
        return String::new();
    }
    let prefix = format!("{priority}:");
    result
        .output
        .lines()
        .find(|line| {
            line.contains(needle) && line.split_whitespace().next() == Some(prefix.as_str())
        })
        .unwrap_or_default()
        .to_string()
}

fn uplink_value(env: &RuntimeEnv) -> String {
    let runtime_core = env.common_bin.join("achost-runtime-core");
    if runtime_core.exists() {
        let result = run_program_capture(&runtime_core, &["detect-uplink", "1.1.1.1"]);
        if result.ok {
            return result.output.trim().to_string();
        }
    }
    let result = run_program_capture(Path::new("ip"), &["route", "get", "1.1.1.1"]);
    if !result.ok {
        return String::new();
    }
    let parts = result.output.split_whitespace().collect::<Vec<_>>();
    parts
        .windows(2)
        .find_map(|pair| (pair[0] == "dev").then(|| pair[1].to_string()))
        .unwrap_or_default()
}

fn settings_json(env: &RuntimeEnv) -> Value {
    json!({
        "ok": true,
        "autostart": autostart_value(env),
        "autostart_file": path_string(&env.autostart_file),
        "data_root": path_string(&env.var),
        "module_root": path_string(&env.achost),
        "base_root": env::var("ACHOST_COMMON").unwrap_or_else(|_| path_string(&env.achost)),
        "dockerd_log": path_string(&env.dockerd_log),
        "containerd_log": path_string(&env.containerd_log),
        "supervisor_log": path_string(&env.supervisor_log),
    })
}

fn set_autostart(env: &RuntimeEnv, value: &str) -> Value {
    let enabled = match value {
        "on" | "1" | "true" => true,
        "off" | "0" | "false" => false,
        _ => return error_json("invalid autostart value"),
    };
    if let Err(error) = write_autostart(env, enabled) {
        return error_json(&format!("could not write autostart setting: {error}"));
    }
    json!({"ok": true, "autostart": enabled, "autostart_file": path_string(&env.autostart_file)})
}

fn write_autostart(env: &RuntimeEnv, enabled: bool) -> io::Result<()> {
    fs::create_dir_all(&env.config)?;
    fs::write(&env.autostart_file, if enabled { "1\n" } else { "0\n" })
}

fn check_json(env: &RuntimeEnv) -> Value {
    let mut rc = 0;
    let mut output = String::new();
    let validate = env.common_bin.join("achost-container-validate.sh");
    if validate.exists() {
        let result = run_program(&validate, &[]);
        if !result.ok {
            rc = result.rc;
        }
        output.push_str(&result.output);
    }
    if env.docker.exists() && socket_present(env) {
        let result = run_program_capture(&env.docker, &["info"]);
        if !result.ok {
            rc = result.rc;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&result.output);
    }
    json!({"ok": rc == 0, "rc": rc, "output": output})
}

fn list_containers(env: &RuntimeEnv) -> Value {
    if !env.docker.exists() {
        return error_json("docker binary not found");
    }
    if !socket_present(env) {
        return json!({"ok": true, "containers": []});
    }
    let result = run_program_capture(
        &env.docker,
        &[
            "ps",
            "-a",
            "--no-trunc",
            "--format",
            "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}|{{.CreatedAt}}",
        ],
    );
    let containers = if result.ok {
        result
            .output
            .lines()
            .filter_map(parse_container_line)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({"ok": true, "containers": containers})
}

fn parse_container_line(line: &str) -> Option<ContainerItem> {
    let fields = split_fields(line, 5);
    (!fields.first().copied().unwrap_or_default().is_empty()).then(|| ContainerItem {
        id: fields.first().copied().unwrap_or_default().to_string(),
        name: fields.get(1).copied().unwrap_or_default().to_string(),
        image: fields.get(2).copied().unwrap_or_default().to_string(),
        status: fields.get(3).copied().unwrap_or_default().to_string(),
        created: fields.get(4).copied().unwrap_or_default().to_string(),
    })
}

fn list_images(env: &RuntimeEnv) -> Value {
    if !env.docker.exists() {
        return error_json("docker binary not found");
    }
    if !socket_present(env) {
        return json!({"ok": true, "images": []});
    }
    let result = run_program_capture(
        &env.docker,
        &[
            "images",
            "--no-trunc",
            "--format",
            "{{.Repository}}|{{.Tag}}|{{.ID}}|{{.Size}}|{{.CreatedSince}}",
        ],
    );
    let images = if result.ok {
        result
            .output
            .lines()
            .filter_map(parse_image_line)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    json!({"ok": true, "images": images})
}

fn parse_image_line(line: &str) -> Option<ImageItem> {
    let fields = split_fields(line, 5);
    (!fields.get(2).copied().unwrap_or_default().is_empty()).then(|| ImageItem {
        repository: fields.first().copied().unwrap_or_default().to_string(),
        tag: fields.get(1).copied().unwrap_or_default().to_string(),
        id: fields.get(2).copied().unwrap_or_default().to_string(),
        size: fields.get(3).copied().unwrap_or_default().to_string(),
        created: fields.get(4).copied().unwrap_or_default().to_string(),
    })
}

fn split_fields(line: &str, count: usize) -> Vec<&str> {
    let mut fields = line.splitn(count, '|').collect::<Vec<_>>();
    fields.resize(count, "");
    fields
}

fn docker_target_action(env: &RuntimeEnv, label: &str, prefix: &[&str], target: &str) -> Value {
    if !valid_name(target) {
        return error_json("invalid container id or name");
    }
    let mut args = prefix
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    args.push(target.to_string());
    run_and_report(label, &env.docker, &args)
}

fn container_logs(env: &RuntimeEnv, target: &str) -> Value {
    if !valid_name(target) {
        return error_json("invalid container id or name");
    }
    run_and_report(
        "container-logs",
        &env.docker,
        &[
            "logs".to_string(),
            "--tail".to_string(),
            "200".to_string(),
            target.to_string(),
        ],
    )
}

fn inspect_container(env: &RuntimeEnv, target: &str) -> Value {
    if !valid_name(target) {
        return error_json("invalid container id or name");
    }
    run_and_report(
        "inspect-container",
        &env.docker,
        &["inspect".to_string(), target.to_string()],
    )
}

fn pull_image(env: &RuntimeEnv, image: &str) -> Value {
    if !valid_image(image) {
        return error_json("invalid image name");
    }
    run_and_report(
        "pull-image",
        &env.docker,
        &["pull".to_string(), image.to_string()],
    )
}

fn remove_image(env: &RuntimeEnv, image: &str) -> Value {
    if !valid_image(image) {
        return error_json("invalid image id or name");
    }
    run_and_report(
        "remove-image",
        &env.docker,
        &["rmi".to_string(), image.to_string()],
    )
}

fn add_container(
    env: &RuntimeEnv,
    name: &str,
    image: &str,
    ports: &str,
    envs: &str,
    mounts: &str,
    network: &str,
) -> Value {
    match build_add_container_args(env, name, image, ports, envs, mounts, network) {
        Ok(args) => run_and_report("add-container", &env.docker, &args),
        Err(message) => error_json(message),
    }
}

fn build_add_container_args(
    env: &RuntimeEnv,
    name: &str,
    image: &str,
    ports: &str,
    envs: &str,
    mounts: &str,
    network: &str,
) -> Result<Vec<String>, &'static str> {
    if !valid_name(name) {
        return Err("invalid container name");
    }
    if !valid_image(image) {
        return Err("invalid image name");
    }
    let network = if network.is_empty() {
        "bridge"
    } else {
        network
    };
    if !valid_network(network) {
        return Err("invalid network");
    }
    let ports = valid_csv(ports, valid_port_item).ok_or("invalid port mapping")?;
    let env_items = valid_csv(envs, valid_env_item).ok_or("invalid environment entry")?;
    let mount_items = valid_csv(mounts, valid_mount_item).ok_or("invalid bind mount")?;
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        name.to_string(),
    ];
    if !network.is_empty() {
        args.push("--network".to_string());
        args.push(network.to_string());
    }
    for item in ports {
        args.push("-p".to_string());
        args.push(item);
    }
    for item in env_items {
        args.push("-e".to_string());
        args.push(item);
    }
    for item in mount_items {
        args.push("-v".to_string());
        args.push(normalize_mount_item(env, &item));
    }
    args.push(image.to_string());
    Ok(args)
}

fn daemon_logs(env: &RuntimeEnv) -> Value {
    let mut output = String::new();
    for path in [&env.dockerd_log, &env.containerd_log, &env.supervisor_log] {
        if path.exists() {
            let chunk = tail_lines(path, 160);
            output.push_str(&format!(
                "\n== daemon-logs: {} ==\n{}",
                path.display(),
                chunk
            ));
        }
    }
    json!({"ok": true, "action": "daemon-logs", "rc": 0, "output": output})
}

fn tail_lines(path: &Path, count: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn valid_image(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b':' | b'@' | b'-')
        })
}

fn valid_network(value: &str) -> bool {
    value.is_empty() || matches!(value, "bridge" | "host" | "none") || valid_name(value)
}

fn valid_port_item(value: &str) -> bool {
    !value.is_empty()
        && value.contains(':')
        && value.bytes().all(|byte| {
            byte.is_ascii_digit()
                || byte.is_ascii_lowercase()
                || matches!(byte, b':' | b'/' | b'.' | b'-')
        })
}

fn valid_env_item(value: &str) -> bool {
    let Some(first) = value.as_bytes().first().copied() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && value.contains('=')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'=' | b'@' | b'.' | b',' | b':' | b'/' | b'+' | b'-'
                )
        })
}

fn valid_mount_item(value: &str) -> bool {
    let Some((left, right)) = value.split_once(':') else {
        return false;
    };
    !left.is_empty()
        && !right.is_empty()
        && left.starts_with('/')
        && right.starts_with('/')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'.' | b'/' | b':' | b'@' | b',' | b'+' | b'=' | b'-'
                )
        })
}

fn valid_csv(value: &str, validator: fn(&str) -> bool) -> Option<Vec<String>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    for item in value.split(',') {
        if !validator(item) {
            return None;
        }
        items.push(item.to_string());
    }
    Some(items)
}

fn normalize_mount_item(env: &RuntimeEnv, item: &str) -> String {
    let docker_socket = path_string(&env.docker_socket_path());
    if matches!(item, "/var/run/docker.sock" | "/run/docker.sock") {
        return docker_socket;
    }
    for prefix in ["/var/run/docker.sock:", "/run/docker.sock:"] {
        if let Some(rest) = item.strip_prefix(prefix) {
            return format!("{docker_socket}:{rest}");
        }
    }
    item.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_env() -> RuntimeEnv {
        RuntimeEnv {
            achost: PathBuf::from("/data/adb/modules/achost-docker/achost"),
            bin: PathBuf::from("/data/adb/modules/achost-docker/achost/bin"),
            var: PathBuf::from("/data/adb/achost"),
            run: PathBuf::from("/data/adb/achost/run"),
            config: PathBuf::from("/data/adb/achost/config"),
            dockerd_pid: PathBuf::from("/data/adb/achost/run/dockerd.pid"),
            containerd_pid: PathBuf::from("/data/adb/achost/run/containerd.pid"),
            dockerd_log: PathBuf::from("/data/adb/achost/log/dockerd.log"),
            containerd_log: PathBuf::from("/data/adb/achost/log/containerd.log"),
            supervisor_log: PathBuf::from("/data/adb/achost/log/achost-supervise.log"),
            docker_host: "unix:///data/adb/achost/run/docker.sock".to_string(),
            docker: PathBuf::from("/data/adb/modules/achost-docker/achost/bin/docker"),
            common_bin: PathBuf::from("/data/adb/modules/achost-base/achost/bin"),
            autostart_file: PathBuf::from("/data/adb/achost/config/docker.autostart"),
            runtime_mode: "native".to_string(),
            cgroup_mode: "v1".to_string(),
            use_chroot: "0".to_string(),
            chroot: PathBuf::from("/data/adb/achost/chroot"),
            native_root: PathBuf::from("/data/adb/achost/native-root"),
            dns_servers: "1.1.1.1 8.8.8.8".to_string(),
            bridge: "docker0".to_string(),
            return_rule_priority: "11999".to_string(),
            source_rule_priority: "12000".to_string(),
            base_present: true,
        }
    }

    #[test]
    fn validates_inputs_like_shell_api() {
        assert!(valid_name("portainer-1"));
        assert!(!valid_name("bad/name"));
        assert!(valid_image("6053537/portainer-ce:latest"));
        assert!(!valid_image("bad image"));
        assert!(valid_network("bridge"));
        assert!(valid_network("custom_net-1"));
        assert!(!valid_network("bad/net"));
        assert!(valid_port_item("127.0.0.1:8080:80/tcp"));
        assert!(!valid_port_item("8080"));
        assert!(valid_env_item("KEY=value:/tmp+1"));
        assert!(!valid_env_item("1KEY=value"));
        assert!(valid_mount_item("/data/www:/usr/share/nginx/html"));
        assert!(!valid_mount_item("relative:/container"));
    }

    #[test]
    fn rewrites_default_socket_mounts() {
        let env = test_env();
        assert_eq!(
            normalize_mount_item(&env, "/var/run/docker.sock:/var/run/docker.sock"),
            "/data/adb/achost/run/docker.sock:/var/run/docker.sock"
        );
        assert_eq!(
            normalize_mount_item(&env, "/run/docker.sock:/run/docker.sock"),
            "/data/adb/achost/run/docker.sock:/run/docker.sock"
        );
        assert_eq!(
            normalize_mount_item(&env, "/var/run/docker.sock"),
            "/data/adb/achost/run/docker.sock"
        );
        assert_eq!(normalize_mount_item(&env, "/data:/data"), "/data:/data");
    }

    #[test]
    fn builds_add_container_argv() {
        let env = test_env();
        let args = build_add_container_args(
            &env,
            "portainer",
            "6053537/portainer-ce",
            "9000:9000",
            "KEY=value",
            "/run/docker.sock:/run/docker.sock",
            "bridge",
        )
        .unwrap();
        assert_eq!(args[0], "run");
        assert!(args.contains(&"--name".to_string()));
        assert!(args.contains(&"portainer".to_string()));
        assert!(args.contains(&"/data/adb/achost/run/docker.sock:/run/docker.sock".to_string()));
    }

    #[test]
    fn parses_docker_rows() {
        let container =
            parse_container_line("abc|name|image|Up 1 second|2026-05-13 00:00:00 +0000 UTC")
                .unwrap();
        assert_eq!(container.id, "abc");
        assert_eq!(container.name, "name");
        let image = parse_image_line("repo|tag|sha256:abc|12MB|2 days ago").unwrap();
        assert_eq!(image.repository, "repo");
        assert_eq!(image.id, "sha256:abc");
    }

    #[test]
    fn response_for_unsupported_command_is_json_error() {
        let env = test_env();
        let value = dispatch(&env, &["bad".to_string()]);
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "unsupported command");
    }

    #[test]
    fn parses_network_helpers() {
        assert_eq!(
            bridge_subnet_value("172.31.0.0/16 proto kernel src 172.31.0.1"),
            "172.31.0.0/16"
        );
        assert_eq!(trim_trailing_newlines("hello\n\n".to_string()), "hello");
    }
}
