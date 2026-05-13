use std::env;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const FILTER_FORWARD_CHAINS: &[&str] = &[
    "DOCKER-FORWARD",
    "DOCKER-BRIDGE",
    "DOCKER-CT",
    "DOCKER-INTERNAL",
    "DOCKER-ISOLATION",
    "DOCKER-ISOLATION-STAGE-1",
    "DOCKER-ISOLATION-STAGE-2",
    "DOCKER",
];

const FILTER_DELETE_CHAINS: &[&str] = &[
    "DOCKER-INTERNAL",
    "DOCKER-CT",
    "DOCKER-BRIDGE",
    "DOCKER-FORWARD",
    "DOCKER",
    "DOCKER-ISOLATION-STAGE-2",
    "DOCKER-ISOLATION-STAGE-1",
    "DOCKER-ISOLATION",
];

#[derive(Debug)]
struct DockerRuntimeConfig {
    achost: PathBuf,
    achost_bin: PathBuf,
    common_bin: PathBuf,
    runtime_mode: String,
    use_chroot: bool,
    cgroup_mode: String,
    chroot: PathBuf,
    chroot_launch_mode: String,
    native_root: PathBuf,
    run: PathBuf,
    docker_root: PathBuf,
    docker_exec_root: PathBuf,
    containerd_root: PathBuf,
    containerd_state: PathBuf,
    docker_config: PathBuf,
    dockerd_config: PathBuf,
    containerd_config: PathBuf,
    docker_host: String,
    docker_socket: Option<PathBuf>,
    compat_host: Option<String>,
    compat_socket: Option<PathBuf>,
    containerd_address: PathBuf,
    dns_servers: Vec<String>,
    supervise: PathBuf,
    use_supervisor: bool,
    supervisor_pid: PathBuf,
    supervisor_socket: PathBuf,
    supervisor_log: PathBuf,
    dockerd_pid: PathBuf,
    dockerd_launch_pid: PathBuf,
    dockerd_log: PathBuf,
    containerd_pid: PathBuf,
    containerd_log: PathBuf,
    external_containerd: bool,
    container_bridge: String,
}

impl DockerRuntimeConfig {
    fn from_env() -> Self {
        let achost = env_path("ACHOST").unwrap_or_else(|| PathBuf::from("/data/adb/achost"));
        let achost_var = env_path("ACHOST_VAR").unwrap_or_else(|| achost.join("var"));
        let achost_bin = env_path("ACHOST_BIN").unwrap_or_else(|| achost.join("bin"));
        let common_bin = env_path("ACHOST_COMMON_BIN").unwrap_or_else(|| achost_bin.clone());
        let run = env_path("ACHOST_RUN").unwrap_or_else(|| achost_var.join("run"));
        let log_dir = env_path("ACHOST_LOG_DIR").unwrap_or_else(|| achost_var.join("log"));
        let runtime_mode = env::var("ACHOST_RUNTIME_MODE").unwrap_or_else(|_| "native".to_string());
        let use_chroot = match env::var("ACHOST_USE_CHROOT") {
            Ok(value) => value == "1",
            Err(_) => runtime_mode == "chroot",
        };
        let docker_host = match env::var("DOCKER_HOST") {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => {
                format!("unix://{}", run.join("docker.sock").display())
            }
            Err(env::VarError::NotUnicode(_)) => String::new(),
        };
        let docker_socket = unix_socket_path(&docker_host);
        let compat_host = env::var("ACHOST_DOCKER_COMPAT_HOST")
            .unwrap_or_else(|_| "unix:///var/run/docker.sock".to_string());
        let compat_host =
            (!matches!(compat_host.as_str(), "" | "0" | "none")).then_some(compat_host);
        let compat_socket = compat_host.as_deref().and_then(unix_socket_path);
        let container_bridge = env::var("CONTAINER_BRIDGE")
            .or_else(|_| env::var("DOCKER_BRIDGE"))
            .unwrap_or_else(|_| "docker0".to_string());
        Self {
            achost: achost.clone(),
            achost_bin,
            common_bin: common_bin.clone(),
            runtime_mode,
            use_chroot,
            cgroup_mode: env::var("ACHOST_CGROUP_MODE").unwrap_or_else(|_| "v1".to_string()),
            chroot: env_path("ACHOST_CHROOT").unwrap_or_else(|| achost_var.join("chroot")),
            chroot_launch_mode: env::var("ACHOST_CHROOT_LAUNCH_MODE")
                .unwrap_or_else(|_| "pivot".to_string()),
            native_root: env_path("ACHOST_NATIVE_ROOT")
                .unwrap_or_else(|| achost_var.join("native-root")),
            run: run.clone(),
            docker_root: env_path("ACHOST_DOCKER_ROOT")
                .unwrap_or_else(|| achost_var.join("docker")),
            docker_exec_root: env_path("ACHOST_DOCKER_EXEC_ROOT")
                .unwrap_or_else(|| run.join("docker-exec")),
            containerd_root: env_path("ACHOST_CONTAINERD_ROOT")
                .unwrap_or_else(|| achost_var.join("containerd/root")),
            containerd_state: env_path("ACHOST_CONTAINERD_STATE")
                .unwrap_or_else(|| achost_var.join("containerd/state")),
            docker_config: env_path("DOCKER_CONFIG").unwrap_or_else(|| achost.join("etc/docker")),
            dockerd_config: env_path("ACHOST_DOCKERD_CONFIG")
                .unwrap_or_else(|| run.join("dockerd-daemon.json")),
            containerd_config: env_path("ACHOST_CONTAINERD_CONFIG")
                .unwrap_or_else(|| achost.join("etc/containerd/config.toml")),
            docker_host,
            docker_socket,
            compat_host,
            compat_socket,
            containerd_address: env_path("CONTAINERD_ADDRESS")
                .unwrap_or_else(|| run.join("containerd.sock")),
            dns_servers: env::var("ACHOST_DNS_SERVERS")
                .unwrap_or_else(|_| "1.1.1.1 8.8.8.8".to_string())
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            supervise: env_path("ACHOST_SUPERVISE")
                .unwrap_or_else(|| common_bin.join("achost-supervise")),
            use_supervisor: env::var("ACHOST_USE_SUPERVISOR").map_or(true, |value| value != "0"),
            supervisor_pid: env_path("ACHOST_SUPERVISOR_PID")
                .unwrap_or_else(|| run.join("achost-supervise.pid")),
            supervisor_socket: env_path("ACHOST_SUPERVISOR_SOCKET")
                .unwrap_or_else(|| run.join("achost-supervise.sock")),
            supervisor_log: env_path("ACHOST_SUPERVISOR_LOG")
                .unwrap_or_else(|| log_dir.join("achost-supervise.log")),
            dockerd_pid: env_path("ACHOST_DOCKERD_PID").unwrap_or_else(|| run.join("dockerd.pid")),
            dockerd_launch_pid: env_path("ACHOST_DOCKERD_LAUNCH_PID")
                .unwrap_or_else(|| run.join("dockerd-launch.pid")),
            dockerd_log: env_path("ACHOST_DOCKERD_LOG")
                .unwrap_or_else(|| log_dir.join("dockerd.log")),
            containerd_pid: env_path("ACHOST_CONTAINERD_PID")
                .unwrap_or_else(|| run.join("containerd.pid")),
            containerd_log: env_path("ACHOST_CONTAINERD_LOG")
                .unwrap_or_else(|| log_dir.join("containerd.log")),
            external_containerd: env::var("ACHOST_EXTERNAL_CONTAINERD")
                .map_or(true, |value| value == "1"),
            container_bridge,
        }
    }
}

#[derive(Debug)]
struct DockerStopConfig {
    use_chroot: bool,
    chroot: PathBuf,
    dockerd_pid: PathBuf,
    dockerd_launch_pid: PathBuf,
    containerd_pid: PathBuf,
    supervisor_pid: PathBuf,
    supervisor_socket: PathBuf,
    docker_socket: Option<PathBuf>,
    compat_socket: Option<PathBuf>,
    containerd_address: PathBuf,
}

impl DockerStopConfig {
    fn from_env() -> Self {
        let achost = env_path("ACHOST").unwrap_or_else(|| PathBuf::from("/data/adb/achost"));
        let achost_var = env_path("ACHOST_VAR").unwrap_or_else(|| achost.join("var"));
        let run = env_path("ACHOST_RUN").unwrap_or_else(|| achost_var.join("run"));
        Self {
            use_chroot: env::var("ACHOST_USE_CHROOT").is_ok_and(|value| value == "1"),
            chroot: env_path("ACHOST_CHROOT").unwrap_or_else(|| achost_var.join("chroot")),
            dockerd_pid: env_path("ACHOST_DOCKERD_PID").unwrap_or_else(|| run.join("dockerd.pid")),
            dockerd_launch_pid: env_path("ACHOST_DOCKERD_LAUNCH_PID")
                .unwrap_or_else(|| run.join("dockerd-launch.pid")),
            containerd_pid: env_path("ACHOST_CONTAINERD_PID")
                .unwrap_or_else(|| run.join("containerd.pid")),
            supervisor_pid: env_path("ACHOST_SUPERVISOR_PID")
                .unwrap_or_else(|| run.join("achost-supervise.pid")),
            supervisor_socket: env_path("ACHOST_SUPERVISOR_SOCKET")
                .unwrap_or_else(|| run.join("achost-supervise.sock")),
            docker_socket: match env::var("DOCKER_HOST") {
                Ok(value) => unix_socket_path(&value),
                Err(env::VarError::NotPresent) => Some(run.join("docker.sock")),
                Err(env::VarError::NotUnicode(_)) => None,
            },
            compat_socket: env::var("ACHOST_DOCKER_COMPAT_HOST")
                .ok()
                .and_then(|value| unix_socket_path(&value)),
            containerd_address: env_path("CONTAINERD_ADDRESS")
                .unwrap_or_else(|| run.join("containerd.sock")),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("cleanup-stale-iptables") => {
            cleanup_stale_iptables();
            0
        }
        Some("prepare-native-root") => run_prepare_native_root(),
        Some("native-preflight") => run_native_preflight(),
        Some("prepare-compat-socket") => run_prepare_compat_socket(),
        Some("prepare-cgroups") => {
            run_prepare_cgroups(args.iter().skip(2).any(|arg| arg == "--print-memory"))
        }
        Some("write-configs") => run_write_configs(),
        Some("namespace-diagnostics") => run_namespace_diagnostics(),
        Some("start") => run_start(&args[2..]),
        Some("stop") => run_stop(),
        Some(command) => {
            eprintln!("unsupported command: {command}");
            2
        }
        None => {
            eprintln!("usage: achost-docker-runtime <cleanup-stale-iptables|prepare-native-root|native-preflight|prepare-compat-socket|prepare-cgroups|write-configs|namespace-diagnostics|start|stop>");
            2
        }
    };
    std::process::exit(code);
}

fn run_prepare_native_root() -> i32 {
    let config = DockerRuntimeConfig::from_env();
    if let Err(error) = prepare_native_root(&config) {
        eprintln!("prepare native root failed: {error}");
        return 1;
    }
    0
}

fn run_native_preflight() -> i32 {
    let config = DockerRuntimeConfig::from_env();
    native_preflight(&config);
    0
}

fn run_prepare_compat_socket() -> i32 {
    let config = DockerRuntimeConfig::from_env();
    let Some(host) = prepare_compat_socket(&config) else {
        return 0;
    };
    println!("{host}");
    0
}

fn run_prepare_cgroups(print_memory: bool) -> i32 {
    let memory_mount = prepare_cgroups();
    if print_memory {
        let Some(path) = memory_mount else {
            return 1;
        };
        println!("{}", path.display());
    }
    0
}

fn run_write_configs() -> i32 {
    let config = DockerRuntimeConfig::from_env();
    if let Err(error) = write_configs(&config) {
        eprintln!("write Docker runtime configs failed: {error}");
        return 1;
    }
    0
}

fn run_namespace_diagnostics() -> i32 {
    let config = DockerRuntimeConfig::from_env();
    namespace_diagnostics(&config);
    0
}

fn run_start(extra_dockerd_args: &[String]) -> i32 {
    let config = DockerRuntimeConfig::from_env();
    if let Err(error) = start_runtime(&config, extra_dockerd_args) {
        eprintln!("{error}");
        return 1;
    }
    0
}

fn write_configs(config: &DockerRuntimeConfig) -> std::io::Result<()> {
    write_dockerd_config(config)?;
    write_containerd_config(config)
}

fn write_dockerd_config(config: &DockerRuntimeConfig) -> std::io::Result<()> {
    let template = config.docker_config.join("daemon.json");
    let raw = fs::read_to_string(&template)?;
    let rendered = raw.replace("@ACHOST_PREFIX@", &path_string(&config.achost));
    if let Some(parent) = config.dockerd_config.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&config.dockerd_config, rendered)
}

fn write_containerd_config(config: &DockerRuntimeConfig) -> std::io::Result<()> {
    if let Some(parent) = config.containerd_config.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &config.containerd_config,
        format!(
            "version = 3\nroot = '{}'\nstate = '{}'\ntemp = '{}/containerd-tmp'\ndisabled_plugins = ['io.containerd.grpc.v1.cri', 'io.containerd.cri.v1.images', 'io.containerd.cri.v1.runtime']\nrequired_plugins = []\noom_score = 0\nimports = []\n\n[grpc]\n  address = '{}'\n  tcp_address = ''\n  uid = 0\n  gid = 0\n\n[debug]\n  address = ''\n  uid = 0\n  gid = 0\n  level = 'debug'\n\n[metrics]\n  address = ''\n  grpc_histogram = false\n\n[plugins.'io.containerd.cri.v1.runtime']\n  enable_cdi = false\n  cdi_spec_dirs = []\n\n[plugins.'io.containerd.nri.v1.nri']\n  disable = true\n  socket_path = '{}/nri.sock'\n",
            path_string(&config.containerd_root),
            path_string(&config.containerd_state),
            path_string(&config.run),
            path_string(&config.containerd_address),
            path_string(&config.run),
        ),
    )
}

fn start_runtime(
    config: &DockerRuntimeConfig,
    extra_dockerd_args: &[String],
) -> Result<(), String> {
    require_start_executables(config)?;
    create_start_dirs(config)?;
    println!("runtime_mode={}", config.runtime_mode);
    println!("use_chroot={}", if config.use_chroot { "1" } else { "0" });
    println!("cgroup_mode={}", config.cgroup_mode);
    println!("chroot_launch_mode={}", config.chroot_launch_mode);

    if config.use_supervisor && !is_executable(&config.supervise) {
        eprintln!("warning: achost-supervise missing; daemon descendants may be reparented to Android init");
    }
    if config.use_chroot {
        return Err("Docker chroot startup is not supported by achost-docker-runtime start; use native runtime".to_string());
    }

    prepare_native_root(config).map_err(|error| format!("prepare native root failed: {error}"))?;
    prepare_cgroups();
    if !ensure_supervisor_server(config) {
        eprintln!("warning: native supervisor server not ready; private /run unavailable");
    }
    native_preflight(config);
    run_protect_daemons(config);
    spawn_network_watchdog(config);
    write_configs(config)
        .map_err(|error| format!("write Docker runtime configs failed: {error}"))?;

    if config.external_containerd {
        if pid_running_file(&config.containerd_pid) {
            if let Some(pid) = read_trimmed(&config.containerd_pid) {
                println!("containerd already running pid={pid}");
            }
        } else {
            remove_file_quiet(&config.containerd_pid);
            remove_file_quiet(&config.containerd_address);
            start_containerd_daemon(config);
            if wait_for_socket(&config.containerd_address) {
                if let Some(pid) = read_trimmed(&config.containerd_pid) {
                    println!("containerd started pid={pid}");
                }
            } else {
                eprintln!(
                    "containerd socket not ready: {}",
                    config.containerd_address.display()
                );
            }
        }
    } else {
        remove_file_quiet(&config.containerd_pid);
        remove_file_quiet(&config.containerd_address);
        println!("external containerd disabled; dockerd will manage containerd");
    }

    let compat_host = prepare_compat_socket(config);
    if pid_running_file(&config.dockerd_pid) {
        if let Some(pid) = read_trimmed(&config.dockerd_pid) {
            println!("dockerd already running pid={pid}");
        }
    } else {
        remove_file_quiet(&config.dockerd_pid);
        remove_file_quiet(&config.dockerd_launch_pid);
        if let Some(path) = config.docker_socket.as_deref() {
            remove_file_quiet(path);
        }
        if let Some(path) = config.compat_socket.as_deref() {
            remove_file_quiet(path);
        }
        cleanup_stale_iptables();
        start_dockerd(config, compat_host.as_deref(), extra_dockerd_args);
        if let Some(socket) = config.docker_socket.as_deref() {
            if wait_for_socket(socket) {
                if let Some(pid) = dockerd_pid_for_display(config) {
                    println!("dockerd started pid={pid}");
                }
                if supervisor_server_running(config) {
                    if let Some(pid) = read_trimmed(&config.supervisor_pid) {
                        println!("supervisor_pid={pid}");
                    }
                }
            } else {
                eprintln!("dockerd socket not ready: {}", socket.display());
            }
        }
    }

    namespace_diagnostics(config);
    if reconcile_network_once(config) {
        println!("network reconciled bridge={}", config.container_bridge);
    } else {
        eprintln!(
            "warning: network reconciliation pending for bridge={}",
            config.container_bridge
        );
    }
    println!("DOCKER_HOST={}", config.docker_host);
    if let Some(host) = compat_host.as_deref() {
        println!("DOCKER_COMPAT_HOST={host}");
    }
    println!("dockerd_log={}", config.dockerd_log.display());
    println!("containerd_log={}", config.containerd_log.display());
    Ok(())
}

fn require_start_executables(config: &DockerRuntimeConfig) -> Result<(), String> {
    for name in [
        "docker",
        "dockerd",
        "containerd",
        "containerd-shim-runc-v2",
        "ctr",
        "runc",
    ] {
        let path = config.achost_bin.join(name);
        if !is_executable(&path) {
            return Err(format!("missing executable: {}", path.display()));
        }
    }
    Ok(())
}

fn create_start_dirs(config: &DockerRuntimeConfig) -> Result<(), String> {
    for path in [
        &config.docker_root,
        &config.docker_exec_root,
        &config.containerd_root,
        &config.containerd_state,
        &config.run,
        &config.native_root,
    ] {
        fs::create_dir_all(path)
            .map_err(|error| format!("create {} failed: {error}", path.display()))?;
    }
    for log in [
        &config.supervisor_log,
        &config.dockerd_log,
        &config.containerd_log,
    ] {
        if let Some(parent) = log.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create log directory failed: {error}"))?;
        }
    }
    fs::create_dir_all(config.docker_config.join("cli-plugins"))
        .map_err(|error| format!("create Docker config directory failed: {error}"))?;
    if let Some(parent) = config.containerd_config.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create containerd config directory failed: {error}"))?;
    }
    Ok(())
}

fn prepare_cgroups() -> Option<PathBuf> {
    setup_devices_cgroup();
    ensure_host_memory_cgroup()
}

fn setup_devices_cgroup() {
    if !cgroup_controller_available("devices") || has_cgroup_mount("devices") {
        return;
    }
    let path = Path::new("/dev/achost-cgroup/devices");
    if fs::create_dir_all(path).is_err() || !mount_cgroup("devices", path) {
        eprintln!("warning: unable to mount devices cgroup");
    }
}

fn ensure_host_memory_cgroup() -> Option<PathBuf> {
    if let Some(path) = cgroup_v1_mount_point("memory", Some(Path::new("/dev/memcg"))) {
        return Some(path);
    }
    if !cgroup_controller_available("memory") {
        eprintln!("warning: memory cgroup controller unavailable");
        return None;
    }
    if let Ok(controllers) = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers") {
        if controllers.split_whitespace().any(|item| item == "memory") {
            eprintln!(
                "warning: memory still exposed in cgroup2; confirm cgroup_no_v2=memory is active"
            );
        }
    }
    let path = Path::new("/dev/memcg");
    if fs::create_dir_all(path).is_err() {
        eprintln!("warning: unable to create /dev/memcg");
        return None;
    }
    if !mount_cgroup("memory", path) {
        eprintln!("warning: unable to mount memory cgroup at /dev/memcg");
        return None;
    }
    make_mount_private(path);
    Some(path.to_path_buf())
}

fn mount_cgroup(controller: &str, target: &Path) -> bool {
    mount_fs("none", target, "cgroup", controller)
}

fn pid_running_file(pid_file: &Path) -> bool {
    pid_from_file(pid_file).is_some_and(pid_alive)
}

fn dockerd_pid_for_display(config: &DockerRuntimeConfig) -> Option<String> {
    pid_running_file(&config.dockerd_pid).then(|| read_trimmed(&config.dockerd_pid))?
}

fn supervisor_server_running(config: &DockerRuntimeConfig) -> bool {
    pid_running_file(&config.supervisor_pid) && is_socket(&config.supervisor_socket)
}

fn ensure_supervisor_server(config: &DockerRuntimeConfig) -> bool {
    if !config.use_supervisor || !is_executable(&config.supervise) {
        return false;
    }
    if supervisor_server_running(config) {
        return true;
    }
    remove_file_quiet(&config.supervisor_pid);
    remove_file_quiet(&config.supervisor_socket);
    let Ok((stdout, stderr)) = log_stdio(&config.supervisor_log) else {
        eprintln!(
            "warning: unable to open supervisor log: {}",
            config.supervisor_log.display()
        );
        return false;
    };
    let mut command = Command::new(&config.supervise);
    command
        .arg("--server")
        .arg("--socket")
        .arg(&config.supervisor_socket)
        .arg("--pid-file")
        .arg(&config.supervisor_pid)
        .arg("--native-root")
        .arg(&config.native_root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    if command.spawn().is_err() {
        return false;
    }
    for _ in 0..10 {
        if supervisor_server_running(config) {
            return true;
        }
        thread::sleep(Duration::from_secs(1));
    }
    false
}

fn log_stdio(path: &Path) -> std::io::Result<(Stdio, Stdio)> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let stderr = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    Ok((stdout.into(), stderr.into()))
}

fn start_containerd_daemon(config: &DockerRuntimeConfig) -> bool {
    let args = vec![
        path_string(&config.achost_bin.join("containerd")),
        "--config".to_string(),
        path_string(&config.containerd_config),
        "--log-level".to_string(),
        "debug".to_string(),
    ];
    start_daemon_command(
        config,
        "containerd",
        &config.containerd_pid,
        &config.containerd_log,
        &args,
    )
}

fn start_dockerd(
    config: &DockerRuntimeConfig,
    compat_host: Option<&str>,
    extra_dockerd_args: &[String],
) -> bool {
    let mut args = vec![
        path_string(&config.achost_bin.join("dockerd")),
        "--config-file".to_string(),
        path_string(&config.dockerd_config),
        "--data-root".to_string(),
        path_string(&config.docker_root),
        "--exec-root".to_string(),
        path_string(&config.docker_exec_root),
        "--pidfile".to_string(),
        path_string(&config.dockerd_pid),
        "--host".to_string(),
        config.docker_host.clone(),
    ];
    if let Some(host) = compat_host {
        args.push("--host".to_string());
        args.push(host.to_string());
    }
    if config.external_containerd {
        args.push("--containerd".to_string());
        args.push(path_string(&config.containerd_address));
    }
    args.extend(extra_dockerd_args.iter().cloned());
    start_daemon_command(
        config,
        "dockerd",
        &config.dockerd_launch_pid,
        &config.dockerd_log,
        &args,
    )
}

fn start_daemon_command(
    config: &DockerRuntimeConfig,
    name: &str,
    pid_file: &Path,
    log_file: &Path,
    argv: &[String],
) -> bool {
    if !ensure_supervisor_server(config) {
        eprintln!("error: achost-supervise server unavailable for {name}");
        return false;
    }
    let status = Command::new(&config.supervise)
        .arg("--client")
        .arg("--socket")
        .arg(&config.supervisor_socket)
        .arg("--name")
        .arg(name)
        .arg("--pid-file")
        .arg(pid_file)
        .arg("--")
        .arg(&config.supervise)
        .arg("--launch")
        .arg("--log-file")
        .arg(log_file)
        .arg("--")
        .args(argv)
        .status();
    if status.is_ok_and(|status| status.success()) {
        true
    } else {
        eprintln!("error: achost-supervise client failed for {name}");
        false
    }
}

fn wait_for_socket(path: &Path) -> bool {
    for _ in 0..30 {
        if is_socket(path) {
            return true;
        }
        thread::sleep(Duration::from_secs(1));
    }
    false
}

fn is_socket(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

fn reconcile_network_once(config: &DockerRuntimeConfig) -> bool {
    if !wait_for_bridge(&config.container_bridge) {
        return false;
    }
    for helper in [
        config.common_bin.join("container-nat-manager.sh"),
        config.achost_bin.join("container-nat-manager.sh"),
    ] {
        if is_executable(&helper) {
            return command_success_path(&helper, &[]);
        }
    }
    true
}

fn wait_for_bridge(bridge: &str) -> bool {
    for _ in 0..30 {
        if Command::new("ip")
            .arg("addr")
            .arg("show")
            .arg(bridge)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return true;
        }
        thread::sleep(Duration::from_secs(1));
    }
    false
}

fn run_protect_daemons(config: &DockerRuntimeConfig) {
    let helper = config.common_bin.join("protect-container-daemons.sh");
    if is_executable(&helper) {
        command_success_path(&helper, &[]);
    }
}

fn spawn_network_watchdog(config: &DockerRuntimeConfig) {
    let helper = config.common_bin.join("container-network-watchdog.sh");
    if !is_executable(&helper) {
        return;
    }
    Command::new(helper)
        .env(
            "ACHOST_NET_LOG",
            env::var("ACHOST_NET_LOG")
                .unwrap_or_else(|_| "/data/local/tmp/achost-network-watchdog.log".to_string()),
        )
        .env(
            "ACHOST_NET_PID",
            env::var("ACHOST_NET_PID")
                .unwrap_or_else(|_| "/data/local/tmp/achost-network-watchdog.pid".to_string()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
}

fn command_success_path(path: &Path, args: &[&str]) -> bool {
    Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn prepare_native_root(config: &DockerRuntimeConfig) -> std::io::Result<()> {
    fs::create_dir_all(&config.native_root)?;
    fs::create_dir_all(config.native_root.join("etc"))?;
    fs::create_dir_all(config.native_root.join("run"))?;
    fs::create_dir_all(config.native_root.join("tmp"))?;
    fs::create_dir_all(config.native_root.join("var"))?;
    symlink_replace(Path::new("/run"), &config.native_root.join("var/run"));
    write_native_resolv_conf(config)?;
    setup_native_ca_certs(config);
    Ok(())
}

fn write_native_resolv_conf(config: &DockerRuntimeConfig) -> std::io::Result<()> {
    let etc = config.native_root.join("etc");
    fs::create_dir_all(&etc)?;
    let resolv_conf = config
        .dns_servers
        .iter()
        .map(|server| format!("nameserver {server}\n"))
        .collect::<String>();
    fs::write(etc.join("resolv.conf"), resolv_conf)?;
    fs::write(etc.join("hosts"), "127.0.0.1 localhost\n::1 localhost\n")
}

fn setup_native_ca_certs(config: &DockerRuntimeConfig) {
    let system_certs = Path::new("/system/etc/security/cacerts");
    if !system_certs.is_dir() {
        return;
    }
    let ssl = config.native_root.join("etc/ssl");
    fs::create_dir_all(&ssl).ok();
    symlink_replace(system_certs, &ssl.join("certs"));
}

fn prepare_compat_socket(config: &DockerRuntimeConfig) -> Option<String> {
    let host = config.compat_host.as_deref()?;
    let compat_socket = config.compat_socket.as_deref()?;
    if config.docker_socket.as_deref() == Some(compat_socket) {
        return None;
    }

    if let Some(root) = compat_root(config) {
        if path_starts_with(compat_socket, Path::new("/var/run")) {
            fs::create_dir_all(root.join("run")).ok();
            fs::create_dir_all(root.join("var")).ok();
            let var_run = root.join("var/run");
            remove_file_quiet(&var_run);
            symlink(Path::new("/run"), &var_run).ok();
            if let Ok(suffix) = compat_socket.strip_prefix("/var/run") {
                remove_file_quiet(
                    &root
                        .join("run")
                        .join(suffix.strip_prefix("/").unwrap_or(suffix)),
                );
            }
        } else if compat_socket.is_absolute() {
            if let Some(parent) = compat_socket.parent() {
                fs::create_dir_all(root.join(strip_absolute(parent))).ok();
            }
            remove_file_quiet(&root.join(strip_absolute(compat_socket)));
        }
        return Some(host.to_string());
    }

    if let Some(parent) = compat_socket.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!(
                "warning: unable to create Docker compatibility socket dir: {}: {error}",
                parent.display()
            );
            return None;
        }
    }
    remove_file_quiet(compat_socket);
    Some(host.to_string())
}

fn compat_root(config: &DockerRuntimeConfig) -> Option<&Path> {
    if config.use_chroot {
        Some(&config.chroot)
    } else if config.runtime_mode == "native" {
        Some(&config.native_root)
    } else {
        None
    }
}

fn native_preflight(config: &DockerRuntimeConfig) {
    println!("native_path_run={}", config.run.display());
    println!("native_path_native_root={}", config.native_root.display());
    println!("native_path_docker_root={}", config.docker_root.display());
    println!(
        "native_path_containerd_root={}",
        config.containerd_root.display()
    );
    println!(
        "native_path_containerd_state={}",
        config.containerd_state.display()
    );
    print_path_state(Path::new("/run"));
    print_path_state(Path::new("/var/run"));
    print_path_state(Path::new("/sys/fs/cgroup"));
    if mount_is_present(Path::new("/run")) {
        println!("global_run_mount=present");
    } else {
        println!("global_run_mount=absent");
    }

    if let Some(pid) = pid_from_file(&config.supervisor_pid).filter(|pid| pid_alive(*pid)) {
        println!("supervisor_pid={pid}");
        let root = PathBuf::from(format!("/proc/{pid}/root"));
        print_path_state(&root.join("run"));
        print_path_state(&root.join("var/run"));
        print_path_state(&root.join("sys/fs/cgroup"));
        print_path_state(&root.join("sys/fs/cgroup/memory/memory.limit_in_bytes"));
        print_path_state(&root.join("sys/fs/cgroup/cpuset/cpuset.cpus"));
        if let Some(socket) = config.docker_socket.as_deref() {
            print_path_state(&root.join(strip_absolute(socket)));
        }
        print_path_state(&root.join(strip_absolute(&config.containerd_address)));
        let var_run = root.join("var/run");
        if var_run.exists() {
            println!(
                "native_var_run_target={}",
                fs::read_link(&var_run)
                    .map(|path| path_string(&path))
                    .unwrap_or_default()
            );
        }
        let ns = PathBuf::from(format!("/proc/{pid}/ns/mnt"));
        if ns.exists() {
            println!(
                "supervisor_mnt_ns={}",
                fs::read_link(ns)
                    .map(|path| path_string(&path))
                    .unwrap_or_default()
            );
        }
    } else {
        println!("supervisor=not-running");
    }

    print_cgroup_diagnostics();
}

fn namespace_diagnostics(config: &DockerRuntimeConfig) {
    if config.runtime_mode != "native" {
        return;
    }
    let Some(supervisor_pid) = pid_from_file(&config.supervisor_pid).filter(|pid| pid_alive(*pid))
    else {
        return;
    };
    let supervisor_ns = mount_namespace(supervisor_pid).unwrap_or_default();
    for (name, pid_file) in [
        ("containerd", &config.containerd_pid),
        ("dockerd", &config.dockerd_pid),
        ("dockerd_launch", &config.dockerd_launch_pid),
    ] {
        let Some(pid) = pid_from_file(pid_file) else {
            continue;
        };
        let daemon_ns = mount_namespace(pid).unwrap_or_default();
        if !supervisor_ns.is_empty() && daemon_ns == supervisor_ns {
            println!("{name}_mnt_ns={daemon_ns} match=1");
        } else {
            println!("{name}_mnt_ns={daemon_ns} match=0 supervisor={supervisor_ns}");
        }
    }
}

fn print_cgroup_diagnostics() {
    if has_cgroup_mount("devices") {
        println!("devices_cgroup=mounted");
    } else if cgroup_controller_available("devices") {
        println!("devices_cgroup=available-not-mounted");
    } else {
        println!("devices_cgroup=unavailable");
    }

    if let Some(path) = cgroup_v1_mount_point("memory", Some(Path::new("/dev/memcg"))) {
        println!("memory_cgroup=mounted path={}", path.display());
    } else if cgroup_controller_available("memory") {
        println!("memory_cgroup=available-not-mounted");
    } else {
        println!("memory_cgroup=unavailable");
    }

    print_path_state(Path::new("/dev/memcg"));
    print_path_state(Path::new("/dev/memcg/memory.limit_in_bytes"));

    if let Ok(controllers) = fs::read_to_string("/sys/fs/cgroup/cgroup.controllers") {
        let controllers = controllers.trim_end();
        println!("cgroup2_controllers={controllers}");
        if controllers
            .split_whitespace()
            .any(|value| value == "memory")
        {
            println!("cgroup2_memory=present");
        } else {
            println!("cgroup2_memory=absent");
        }
    }

    for mount in read_mount_records() {
        if mount.fs_type == "cgroup" || mount.fs_type == "cgroup2" {
            println!(
                "cgroup_mount={}:{}:{}",
                mount.destination, mount.fs_type, mount.options
            );
        }
    }

    if read_mount_records()
        .into_iter()
        .any(|mount| mount.destination == "/sys/fs/cgroup" && mount.fs_type == "cgroup2")
    {
        cgroup2_diagnostics(Path::new("/sys/fs/cgroup"));
    }
}

fn cgroup2_diagnostics(prefix: &Path) {
    println!("cgroup2_path={}", prefix.display());
    for file in [
        "cgroup.controllers",
        "cgroup.subtree_control",
        "cgroup.type",
        "memory.current",
        "memory.max",
        "memory.swap.current",
        "memory.swap.max",
        "memory.oom.group",
    ] {
        let path = prefix.join(file);
        match fs::read_to_string(&path) {
            Ok(value) => println!("cgroup2_{file}={}", value.trim_end()),
            Err(_) => println!("cgroup2_{file}=missing"),
        }
    }
}

fn has_cgroup_mount(controller: &str) -> bool {
    cgroup_v1_mount_point(controller, None).is_some()
}

fn cgroup_controller_available(controller: &str) -> bool {
    let Ok(cgroups) = fs::read_to_string("/proc/cgroups") else {
        return false;
    };
    cgroups.lines().any(|line| {
        let mut parts = line.split_whitespace();
        matches!(
            (parts.next(), parts.next(), parts.next(), parts.next()),
            (Some(name), Some(_), Some(_), Some("1")) if name == controller
        )
    })
}

fn cgroup_v1_mount_point(controller: &str, preferred: Option<&Path>) -> Option<PathBuf> {
    let mounts = read_mount_records();
    if let Some(preferred) = preferred {
        let preferred = path_string(preferred);
        if let Some(mount) = mounts.iter().find(|mount| {
            mount.destination == preferred
                && mount.fs_type == "cgroup"
                && mount.options.split(',').any(|option| option == controller)
        }) {
            return Some(PathBuf::from(&mount.destination));
        }
    }
    mounts
        .into_iter()
        .find(|mount| {
            mount.fs_type == "cgroup" && mount.options.split(',').any(|option| option == controller)
        })
        .map(|mount| PathBuf::from(mount.destination))
}

fn print_path_state(path: &Path) {
    if path.exists() {
        if writable(path) {
            println!("{}=present,writable", path.display());
        } else {
            println!("{}=present,not-writable", path.display());
        }
    } else {
        println!("{}=missing", path.display());
    }
}

fn writable(path: &Path) -> bool {
    let Some(c_path) = c_path(path) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

fn symlink_replace(src: &Path, dst: &Path) {
    remove_path_quiet(dst);
    symlink(src, dst).ok();
}

fn strip_absolute(path: &Path) -> &Path {
    path.strip_prefix("/").unwrap_or(path)
}

fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    path == prefix || path.starts_with(prefix)
}

fn mount_namespace(pid: u32) -> Option<String> {
    fs::read_link(format!("/proc/{pid}/ns/mnt"))
        .ok()
        .map(|path| path_string(&path))
}

fn pid_from_file(pid_file: &Path) -> Option<u32> {
    parse_pid(read_trimmed(pid_file)?.as_str())
}

fn run_stop() -> i32 {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("achost-docker-stop requires root");
        return 1;
    }

    let config = DockerStopConfig::from_env();
    stop_pid_file("dockerd", &config.dockerd_pid);
    stop_pid_file("dockerd-launch", &config.dockerd_launch_pid);
    stop_pid_file("containerd", &config.containerd_pid);
    stop_named_processes("dockerd");
    stop_named_processes("containerd");
    stop_pid_file("achost-supervise", &config.supervisor_pid);
    remove_file_quiet(&config.supervisor_socket);
    unmount_chroot(&config);
    unmount_devices_cgroup();
    if let Some(path) = config.docker_socket.as_deref() {
        remove_file_quiet(path);
    }
    if let Some(path) = config.compat_socket.as_deref() {
        remove_file_quiet(path);
    }
    remove_file_quiet(&config.containerd_address);
    0
}

fn cleanup_stale_iptables() {
    let Some(iptables) = pick_iptables() else {
        return;
    };

    for chain in FILTER_FORWARD_CHAINS {
        remove_iptables_rule(&iptables, "filter", "FORWARD", &["-j", chain]);
    }
    remove_iptables_rule(
        &iptables,
        "nat",
        "PREROUTING",
        &["-m", "addrtype", "--dst-type", "LOCAL", "-j", "DOCKER"],
    );
    remove_iptables_rule(
        &iptables,
        "nat",
        "OUTPUT",
        &[
            "-m",
            "addrtype",
            "--dst-type",
            "LOCAL",
            "!",
            "--dst",
            "127.0.0.0/8",
            "-j",
            "DOCKER",
        ],
    );
    remove_iptables_rule(
        &iptables,
        "nat",
        "OUTPUT",
        &["-m", "addrtype", "--dst-type", "LOCAL", "-j", "DOCKER"],
    );

    for chain in FILTER_DELETE_CHAINS {
        command_success_null(&iptables, &["-F", chain]);
        command_success_null(&iptables, &["-X", chain]);
    }
    command_success_null(&iptables, &["-t", "nat", "-F", "DOCKER"]);
    command_success_null(&iptables, &["-t", "nat", "-X", "DOCKER"]);
}

fn pick_iptables() -> Option<String> {
    ["iptables", "/system/bin/iptables"]
        .into_iter()
        .find(|command| have_command(command))
        .map(str::to_string)
}

fn remove_iptables_rule(iptables: &str, table: &str, chain: &str, args: &[&str]) {
    let mut check = Vec::new();
    if table != "filter" {
        check.extend(["-t", table]);
    }
    check.extend(["-C", chain]);
    check.extend(args.iter().copied());

    let mut delete = Vec::new();
    if table != "filter" {
        delete.extend(["-t", table]);
    }
    delete.extend(["-D", chain]);
    delete.extend(args.iter().copied());

    while command_success_null(iptables, &check) {
        if !command_success_null(iptables, &delete) {
            break;
        }
    }
}

fn stop_pid_file(name: &str, pid_file: &Path) {
    if !pid_file.is_file() {
        println!("{name} pid file missing: {}", pid_file.display());
        return;
    }

    let raw_pid = fs::read_to_string(pid_file).unwrap_or_default();
    let Some(pid) = parse_pid(raw_pid.trim()) else {
        println!("{name} pid invalid: {}", raw_pid.trim());
        remove_file_quiet(pid_file);
        return;
    };

    if !pid_alive(pid) {
        println!("{name} not running pid={pid}");
        remove_file_quiet(pid_file);
        return;
    }

    signal_pid(pid, libc::SIGTERM);
    for _ in 0..10 {
        if !pid_alive(pid) {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    if pid_alive(pid) {
        signal_pid(pid, libc::SIGKILL);
    }
    remove_file_quiet(pid_file);
    println!("{name} stopped pid={pid}");
}

fn stop_named_processes(name: &str) {
    let pids = pids_for_name(name);
    if pids.is_empty() {
        return;
    }
    for pid in &pids {
        signal_pid(*pid, libc::SIGTERM);
    }
    thread::sleep(Duration::from_secs(1));
    for pid in &pids {
        if pid_alive(*pid) {
            signal_pid(*pid, libc::SIGKILL);
        }
    }
    let joined = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    println!("{name} stopped leftover pids={joined}");
}

fn pids_for_name(name: &str) -> Vec<u32> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut pids = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid) = file_name.to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        if read_trimmed(&entry.path().join("comm")).as_deref() == Some(name) {
            pids.push(pid);
        }
    }
    pids.sort_unstable();
    pids
}

fn unmount_chroot(config: &DockerStopConfig) {
    if !config.use_chroot || !Path::new("/proc/mounts").is_file() {
        return;
    }
    make_mount_private(&config.chroot);

    for _ in 0..8 {
        let mounts = chroot_mounts(&config.chroot);
        if mounts.is_empty() {
            break;
        }
        for mount in mounts {
            unmount_path(&mount);
        }
    }

    for _ in 0..4 {
        if !mount_is_present(&config.chroot) {
            break;
        }
        if !unmount_path(&config.chroot) {
            break;
        }
    }
}

fn unmount_devices_cgroup() {
    let path = Path::new("/dev/achost-cgroup/devices");
    if mount_is_present(path) {
        unmount_path(path);
    }
    fs::remove_dir(path).ok();
    fs::remove_dir("/dev/achost-cgroup").ok();
}

fn chroot_mounts(chroot: &Path) -> Vec<PathBuf> {
    let chroot_string = path_string(chroot);
    let prefix = format!("{}/", chroot_string.trim_end_matches('/'));
    let mut mounts: Vec<PathBuf> = read_mount_destinations()
        .into_iter()
        .filter(|mount| mount.starts_with(&prefix))
        .map(PathBuf::from)
        .collect();
    sort_mounts_deepest_first(&mut mounts);
    mounts
}

#[derive(Debug)]
struct MountRecord {
    destination: String,
    fs_type: String,
    options: String,
}

fn read_mount_destinations() -> Vec<String> {
    read_mount_records()
        .into_iter()
        .map(|mount| mount.destination)
        .collect()
}

fn read_mount_records() -> Vec<MountRecord> {
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    mounts
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let _source = parts.next()?;
            let destination = decode_mount_field(parts.next()?);
            let fs_type = parts.next()?.to_string();
            let options = parts.next()?.to_string();
            Some(MountRecord {
                destination,
                fs_type,
                options,
            })
        })
        .collect()
}

fn mount_is_present(path: &Path) -> bool {
    let needle = path_string(path);
    read_mount_destinations()
        .into_iter()
        .any(|mount| mount == needle)
}

fn sort_mounts_deepest_first(mounts: &mut [PathBuf]) {
    mounts.sort_by(|left, right| {
        right
            .components()
            .count()
            .cmp(&left.components().count())
            .then_with(|| path_string(right).len().cmp(&path_string(left).len()))
            .then_with(|| path_string(right).cmp(&path_string(left)))
    });
}

fn make_mount_private(path: &Path) {
    let _ = mount_private(path, libc::MS_PRIVATE | libc::MS_REC)
        || mount_private(path, libc::MS_PRIVATE);
}

fn mount_private(path: &Path, flags: libc::c_ulong) -> bool {
    let Some(c_path) = c_path(path) else {
        return false;
    };
    unsafe {
        libc::mount(
            std::ptr::null::<libc::c_char>(),
            c_path.as_ptr(),
            std::ptr::null::<libc::c_char>(),
            flags,
            std::ptr::null::<libc::c_void>(),
        ) == 0
    }
}

fn mount_fs(source: &str, target: &Path, fs_type: &str, data: &str) -> bool {
    let (Ok(c_source), Some(c_target), Ok(c_fs_type), Ok(c_data)) = (
        CString::new(source),
        c_path(target),
        CString::new(fs_type),
        CString::new(data),
    ) else {
        return false;
    };
    unsafe {
        libc::mount(
            c_source.as_ptr(),
            c_target.as_ptr(),
            c_fs_type.as_ptr(),
            0,
            c_data.as_ptr().cast::<libc::c_void>(),
        ) == 0
    }
}

fn unmount_path(path: &Path) -> bool {
    umount(path, 0) || umount(path, libc::MNT_DETACH)
}

fn umount(path: &Path, flags: libc::c_int) -> bool {
    let Some(c_path) = c_path(path) else {
        return false;
    };
    unsafe { libc::umount2(c_path.as_ptr(), flags) == 0 }
}

fn signal_pid(pid: u32, signal: libc::c_int) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, signal) == 0 }
}

fn pid_alive(pid: u32) -> bool {
    (unsafe { libc::kill(pid as libc::pid_t, 0) == 0 })
        || PathBuf::from(format!("/proc/{pid}")).exists()
}

fn parse_pid(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let pid = value.parse().ok()?;
    (pid != 0).then_some(pid)
}

fn unix_socket_path(value: &str) -> Option<PathBuf> {
    value
        .strip_prefix("unix://")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn decode_mount_field(value: &str) -> String {
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn command_success_null(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn have_command(command: &str) -> bool {
    if command.contains('/') {
        return is_executable(Path::new(command));
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| is_executable(&path.join(command))))
        .unwrap_or(false)
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
        && path
            .metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn c_path(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

fn path_string(path: &Path) -> String {
    path.as_os_str().to_string_lossy().into_owned()
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn remove_file_quiet(path: &Path) {
    fs::remove_file(path).ok();
}

fn remove_path_quiet(path: &Path) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).ok();
    } else {
        fs::remove_file(path).ok();
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pid_values() {
        assert_eq!(parse_pid("123"), Some(123));
        assert_eq!(parse_pid("001"), Some(1));
        assert_eq!(parse_pid(""), None);
        assert_eq!(parse_pid("0"), None);
        assert_eq!(parse_pid("12x"), None);
        assert_eq!(parse_pid("12\n"), None);
    }

    #[test]
    fn parses_unix_socket_paths() {
        assert_eq!(
            unix_socket_path("unix:///data/adb/achost/run/docker.sock"),
            Some(PathBuf::from("/data/adb/achost/run/docker.sock"))
        );
        assert_eq!(unix_socket_path("tcp://127.0.0.1:2375"), None);
        assert_eq!(unix_socket_path("unix://"), None);
    }

    #[test]
    fn filters_chroot_mounts_under_prefix_only() {
        let chroot = Path::new("/data/adb/achost/chroot");
        let prefix = format!("{}/", path_string(chroot).trim_end_matches('/'));
        assert!("/data/adb/achost/chroot/proc".starts_with(&prefix));
        assert!(!"/data/adb/achost/chroot-other/proc".starts_with(&prefix));
        assert!(!"/data/adb/achost/chroot".starts_with(&prefix));
    }

    #[test]
    fn sorts_mounts_deepest_first() {
        let mut mounts = vec![
            PathBuf::from("/a/b"),
            PathBuf::from("/a/b/c/d"),
            PathBuf::from("/a/b/c"),
        ];
        sort_mounts_deepest_first(&mut mounts);
        assert_eq!(
            mounts,
            vec![
                PathBuf::from("/a/b/c/d"),
                PathBuf::from("/a/b/c"),
                PathBuf::from("/a/b"),
            ]
        );
    }

    #[test]
    fn decodes_mount_fields() {
        assert_eq!(decode_mount_field("/a\\040b/c"), "/a b/c");
    }
}
