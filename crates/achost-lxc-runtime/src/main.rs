use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256, Sha512};
use std::env;
use std::ffi::{CString, OsStr};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::net::Ipv4Addr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REQUIRED_BINARIES: &[&str] = &[
    "lxc-start",
    "lxc-stop",
    "lxc-attach",
    "lxc-info",
    "lxc-ls",
    "lxc-destroy",
    "lxc-execute",
    "lxc-checkconfig",
];

const CONDITIONAL_BINARIES: &[&str] = &["lxc-create", "lxc-copy", "lxc-console"];
const METADATA_FILE: &str = "achost-container.json";
const GUEST_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const SHA512_CRYPT_ROUNDS: usize = 5000;
const CRYPT_BASE64: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Debug, Clone)]
struct LxcConfig {
    achost: PathBuf,
    achost_bin: PathBuf,
    common_bin: PathBuf,
    lxc_module: PathBuf,
    lxc_root: PathBuf,
    lxc_bin: PathBuf,
    lxc_etc: PathBuf,
    lxc_var: PathBuf,
    lxc_run: PathBuf,
    lxc_log: PathBuf,
    lxc_rootfs: PathBuf,
    lxc_containers: PathBuf,
    native_root: PathBuf,
    supervise: PathBuf,
    bridge: String,
    subnet: String,
}

impl LxcConfig {
    fn from_env() -> Self {
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("/data/adb/achost/bin"));
        let module_root = exe_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/data/adb/achost"));
        let achost = env_path("ACHOST").unwrap_or(module_root);
        let achost_bin = env_path("ACHOST_BIN").unwrap_or_else(|| exe_dir.clone());
        let common_root = env_path("ACHOST_COMMON")
            .or_else(|| env_path("ACHOST_BASE"))
            .unwrap_or_else(|| {
                let split_base = PathBuf::from("/data/adb/modules/achost-base/achost");
                if split_base.exists() {
                    split_base
                } else {
                    achost.clone()
                }
            });
        let common_bin = env_path("ACHOST_COMMON_BIN").unwrap_or_else(|| common_root.join("bin"));
        let achost_var =
            env_path("ACHOST_VAR").unwrap_or_else(|| PathBuf::from("/data/adb/achost"));
        let lxc_module = env_path("ACHOST_LXC_MODULE").unwrap_or_else(|| achost.clone());
        let lxc_root = env_path("ACHOST_LXC").unwrap_or_else(|| {
            let split_root = lxc_module.join("lxc");
            if split_root.exists() {
                split_root
            } else {
                achost.join("lxc")
            }
        });
        let lxc_var = env_path("ACHOST_LXC_VAR").unwrap_or_else(|| achost_var.join("lxc"));
        let lxc_run = env_path("ACHOST_LXC_RUN").unwrap_or_else(|| achost_var.join("run/lxc"));
        let lxc_log = env_path("ACHOST_LXC_LOG").unwrap_or_else(|| achost_var.join("log/lxc"));
        let native_root =
            env_path("ACHOST_NATIVE_ROOT").unwrap_or_else(|| achost_var.join("native-root"));
        let supervise =
            env_path("ACHOST_SUPERVISE").unwrap_or_else(|| common_bin.join("achost-supervise"));
        Self {
            achost: achost.clone(),
            achost_bin,
            common_bin,
            lxc_module,
            lxc_bin: env_path("ACHOST_LXC_BIN").unwrap_or_else(|| lxc_root.join("bin")),
            lxc_etc: env_path("ACHOST_LXC_ETC").unwrap_or_else(|| achost.join("etc/lxc")),
            lxc_root,
            lxc_var: lxc_var.clone(),
            lxc_run,
            lxc_log,
            lxc_rootfs: env_path("ACHOST_LXC_ROOTFS").unwrap_or_else(|| lxc_var.join("rootfs")),
            lxc_containers: env_path("ACHOST_LXC_CONTAINERS")
                .unwrap_or_else(|| lxc_var.join("containers")),
            native_root,
            supervise,
            bridge: env_nonempty("LXC_BRIDGE").unwrap_or_else(|| "lxcbr0".to_string()),
            subnet: env_nonempty("LXC_SUBNET").unwrap_or_else(|| "172.32.0.0/16".to_string()),
        }
    }

    fn container_dir(&self, name: &str) -> PathBuf {
        self.lxc_containers.join(name)
    }

    fn container_config(&self, name: &str) -> PathBuf {
        self.container_dir(name).join("config")
    }

    fn container_rootfs(&self, name: &str) -> PathBuf {
        self.container_dir(name).join("rootfs")
    }

    fn container_metadata(&self, name: &str) -> PathBuf {
        self.container_dir(name).join(METADATA_FILE)
    }

    fn container_log(&self, name: &str) -> PathBuf {
        self.lxc_log.join(format!("{name}.log"))
    }
}

#[derive(Debug)]
struct BinaryReport {
    name: String,
    required: bool,
    path: Option<PathBuf>,
    executable: bool,
    elf: ElfInfo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ElfInfo {
    is_elf: bool,
    arch: Option<String>,
    interpreter: Option<String>,
}

impl ElfInfo {
    fn none() -> Self {
        Self {
            is_elf: false,
            arch: None,
            interpreter: None,
        }
    }
}

#[derive(Debug)]
struct CommandResult {
    ok: bool,
    output: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ContainerMetadata {
    name: String,
    distro: String,
    release: String,
    arch: String,
    rootfs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rootfs_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    init_cmd: Option<String>,
    created_at_unix: u64,
}

#[derive(Clone, Debug, Serialize)]
struct ContainerStatus {
    name: String,
    state: String,
    pid: Option<String>,
    distro: String,
    release: String,
    arch: String,
    rootfs: String,
    config: String,
    log: String,
    metadata: String,
    autostart: bool,
}

#[derive(Clone, Debug, Serialize)]
struct GuestSystemStatus {
    name: String,
    state: String,
    distro: String,
    release: String,
    arch: String,
    hostname: String,
    uptime: String,
    pretty_name: String,
    addresses: Vec<String>,
    message: String,
}

#[derive(Debug)]
struct ImportArgs {
    name: String,
    rootfs_asset: PathBuf,
    rootfs_sha256: Option<String>,
    distro: String,
    release: String,
    arch: String,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = dispatch(&args);
    std::process::exit(code);
}

fn dispatch(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("validate-host") => run_validate_host(),
        Some("validate-assets") => run_validate_assets(),
        Some("write-configs") => run_write_configs(),
        Some("prepare-bridge") => run_prepare_bridge(),
        Some("import-rootfs") => run_import_rootfs(&args[1..]),
        Some("list") => run_list(&args[1..]),
        Some("status") => run_status(&args[1..]),
        Some("start") => run_start(&args[1..]),
        Some("stop") => run_stop(&args[1..]),
        Some("destroy") => run_destroy(&args[1..]),
        Some("set-autostart") => run_set_autostart(&args[1..]),
        Some("autostart") => run_autostart(),
        Some("system-status") => run_system_status(&args[1..]),
        Some("set-password") => run_set_password(&args[1..]),
        Some("generate-password") => run_generate_password(&args[1..]),
        Some("exec") => run_exec(&args[1..]),
        Some("logs") => run_logs(&args[1..]),
        Some("smoke") => run_smoke(&args[1..]),
        Some(command) => {
            eprintln!("unsupported command: {command}");
            usage();
            2
        }
        None => {
            usage();
            2
        }
    }
}

fn usage() {
    eprintln!(
        "usage: achost-lxc-runtime <validate-host|validate-assets|write-configs|prepare-bridge|import-rootfs|list|status|start|stop|destroy|set-autostart|autostart|system-status|set-password|generate-password|exec|logs|smoke>"
    );
}

fn run_validate_host() -> i32 {
    let config = LxcConfig::from_env();
    let mut failures = 0;

    section("lxc paths");
    print_config_paths(&config);

    section("namespaces");
    for name in ["mnt", "uts", "ipc", "pid", "net"] {
        if !path_check(
            &format!("namespace_{name}"),
            &PathBuf::from(format!("/proc/self/ns/{name}")),
            true,
        ) {
            failures += 1;
        }
    }
    path_check("namespace_user", Path::new("/proc/self/ns/user"), false);

    section("cgroups");
    if !path_check("proc_cgroups", Path::new("/proc/cgroups"), true) {
        failures += 1;
    }
    let mounts =
        parse_cgroup_mounts(&read_to_string(Path::new("/proc/mounts")).unwrap_or_default());
    println!("cgroup_mounts={} required=1", mounts.len());
    for mount in &mounts {
        println!("cgroup_mount path={} fstype={}", mount.path, mount.fstype);
    }
    if mounts.is_empty() {
        failures += 1;
    }

    section("devices");
    if !path_check("devpts", Path::new("/dev/pts"), true) {
        failures += 1;
    }

    section("network");
    match pick_command(&["ip", "/system/bin/ip", "/system/xbin/ip"]) {
        Some(path) => println!("ip=found path={}", path.display()),
        None => println!("ip=missing required=0"),
    }
    match pick_command(&["iptables", "/system/bin/iptables", "/system/xbin/iptables"]) {
        Some(path) => println!("iptables=found path={}", path.display()),
        None => println!("iptables=missing required=0"),
    }
    let bridge_path = PathBuf::from("/sys/class/net").join(&config.bridge);
    let bridge_present = bridge_path.exists();
    println!(
        "lxc_bridge={} present={} subnet={} path={}",
        config.bridge,
        bit(bridge_present),
        config.subnet,
        bridge_path.display()
    );

    section("selinux and pressure");
    print_selinux_status();
    path_check("psi_memory", Path::new("/proc/pressure/memory"), false);
    path_check(
        "memcg_pressure",
        Path::new("/dev/memcg/memory.pressure_level"),
        false,
    );

    section("lxc configs");
    for name in ["android-common.conf", "default.conf"] {
        if !path_check(
            &format!("lxc_config_{name}"),
            &config.lxc_etc.join(name),
            true,
        ) {
            failures += 1;
        }
    }
    path_check(
        "lxc_config_unprivileged.conf",
        &config.lxc_etc.join("unprivileged.conf"),
        false,
    );

    if failures == 0 {
        0
    } else {
        eprintln!("validate-host failures: {failures}");
        1
    }
}

fn run_validate_assets() -> i32 {
    let config = LxcConfig::from_env();
    let mut failures = 0;

    section("lxc paths");
    print_config_paths(&config);

    section("lxc required binaries");
    let mut checkconfig: Option<PathBuf> = None;
    for name in REQUIRED_BINARIES {
        let report = binary_report(&config, name, true);
        if report.name == "lxc-checkconfig" && report.executable {
            checkconfig = report.path.clone();
        }
        if report.required && (report.path.is_none() || !report.executable) {
            failures += 1;
        }
        print_binary_report(&report);
    }

    section("lxc conditional binaries");
    for name in CONDITIONAL_BINARIES {
        print_binary_report(&binary_report(&config, name, false));
    }

    section("lxc checkconfig result");
    match checkconfig {
        Some(path) => run_checkconfig(&path),
        None => println!("lxc_checkconfig=skipped reason=missing"),
    }

    if failures == 0 {
        0
    } else {
        eprintln!("validate-assets failures: {failures}");
        1
    }
}

fn run_write_configs() -> i32 {
    let config = LxcConfig::from_env();
    match write_lxc_configs(&config) {
        Ok(paths) => {
            for path in paths {
                println!("wrote_config={}", path.display());
            }
            0
        }
        Err(error) => {
            eprintln!("write-configs failed: {error}");
            1
        }
    }
}

fn run_prepare_bridge() -> i32 {
    let config = LxcConfig::from_env();
    let core = env_path("ACHOST_RUNTIME_CORE")
        .unwrap_or_else(|| config.common_bin.join("achost-runtime-core"));
    if !core.exists() || !is_executable(&core) {
        eprintln!("achost-runtime-core not executable: {}", core.display());
        return 1;
    }
    let path = format!(
        "{}:{}",
        config.common_bin.display(),
        env::var("PATH").unwrap_or_default()
    );
    match Command::new(&core)
        .arg("bridge-reconcile")
        .arg("--bridge")
        .arg(&config.bridge)
        .arg("--subnet")
        .arg(&config.subnet)
        .arg("--owner")
        .arg("lxc")
        .env("CONTAINER_BRIDGE", &config.bridge)
        .env("CONTAINER_SUBNET", &config.subnet)
        .env("LXC_BRIDGE", &config.bridge)
        .env("LXC_SUBNET", &config.subnet)
        .env("PATH", path)
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("prepare-bridge failed: {error}");
            1
        }
    }
}

fn run_import_rootfs(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    match parse_import_args(args).and_then(|parsed| import_rootfs(&config, &parsed)) {
        Ok(metadata) => {
            println!("imported_container={}", metadata.name);
            println!("rootfs={}", metadata.rootfs);
            if let Some(sha256) = &metadata.rootfs_sha256 {
                println!("sha256={sha256}");
            }
            println!(
                "distro={} release={} arch={}",
                metadata.distro, metadata.release, metadata.arch
            );
            0
        }
        Err(error) => {
            eprintln!("import-rootfs failed: {error}");
            1
        }
    }
}

fn run_list(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let json_output = has_flag(args, "--json");
    match list_containers(&config) {
        Ok(containers) => {
            if json_output {
                print_json(&json!({"ok": true, "containers": containers}));
            } else if containers.is_empty() {
                println!("containers=0");
            } else {
                for item in containers {
                    println!(
                        "name={} state={} pid={} distro={} release={} arch={} rootfs={}",
                        item.name,
                        item.state,
                        item.pid.as_deref().unwrap_or(""),
                        item.distro,
                        item.release,
                        item.arch,
                        item.rootfs
                    );
                }
            }
            0
        }
        Err(error) => {
            if json_output {
                print_json(&json!({"ok": false, "error": error}));
            } else {
                eprintln!("list failed: {error}");
            }
            1
        }
    }
}

fn run_status(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let json_output = has_flag(args, "--json");
    let Some(name) = first_non_flag(args) else {
        eprintln!("status requires container name");
        return 2;
    };
    match validate_container_name(name).and_then(|()| container_status(&config, name)) {
        Ok(status) => {
            if json_output {
                print_json(&json!({"ok": true, "container": status}));
            } else {
                println!("name={}", status.name);
                println!("state={}", status.state);
                println!("pid={}", status.pid.as_deref().unwrap_or(""));
                println!("distro={}", status.distro);
                println!("release={}", status.release);
                println!("arch={}", status.arch);
                println!("rootfs={}", status.rootfs);
                println!("config={}", status.config);
                println!("log={}", status.log);
            }
            0
        }
        Err(error) => {
            if json_output {
                print_json(&json!({"ok": false, "error": error}));
            } else {
                eprintln!("status failed: {error}");
            }
            1
        }
    }
}

fn run_start(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("start requires container name");
        return 2;
    };
    match start_container(&config, name) {
        Ok(()) => {
            println!("started_container={name}");
            0
        }
        Err(error) => {
            eprintln!("start failed: {error}");
            1
        }
    }
}

fn run_stop(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let force = has_flag(args, "--force") || has_flag(args, "-k");
    let Some(name) = first_non_flag(args) else {
        eprintln!("stop requires container name");
        return 2;
    };
    match stop_container(&config, name, force) {
        Ok(()) => {
            if force {
                println!("force_stopped_container={name}");
            } else {
                println!("stopped_container={name}");
            }
            0
        }
        Err(error) => {
            eprintln!("stop failed: {error}");
            1
        }
    }
}

fn run_destroy(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("destroy requires container name");
        return 2;
    };
    match destroy_container(&config, name) {
        Ok(()) => {
            println!("destroyed_container={name}");
            0
        }
        Err(error) => {
            eprintln!("destroy failed: {error}");
            1
        }
    }
}

fn run_set_autostart(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("set-autostart requires container name");
        return 2;
    };
    let Some(value) = args.get(1).map(String::as_str) else {
        eprintln!("set-autostart requires on|off");
        return 2;
    };
    let enabled = match parse_toggle(value) {
        Some(enabled) => enabled,
        None => {
            eprintln!("set-autostart value must be on|off");
            return 2;
        }
    };
    match set_container_autostart(&config, name, enabled) {
        Ok(()) => {
            println!("container={name} autostart={}", bit(enabled));
            0
        }
        Err(error) => {
            eprintln!("set-autostart failed: {error}");
            1
        }
    }
}

fn run_autostart() -> i32 {
    let config = LxcConfig::from_env();
    match autostart_containers(&config) {
        Ok(failures) => {
            if failures == 0 {
                0
            } else {
                1
            }
        }
        Err(error) => {
            eprintln!("autostart failed: {error}");
            1
        }
    }
}

fn run_system_status(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let json_output = has_flag(args, "--json");
    let Some(name) = first_non_flag(args) else {
        eprintln!("system-status requires container name");
        return 2;
    };
    match guest_system_status(&config, name) {
        Ok(status) => {
            if json_output {
                print_json(&json!({"ok": true, "status": status}));
            } else {
                println!("name={}", status.name);
                println!("state={}", status.state);
                println!("distro={}", status.distro);
                println!("release={}", status.release);
                println!("arch={}", status.arch);
                println!("hostname={}", status.hostname);
                println!("uptime={}", status.uptime);
                println!("pretty_name={}", status.pretty_name);
                println!("addresses={}", status.addresses.join(","));
                println!("message={}", status.message);
            }
            0
        }
        Err(error) => {
            if json_output {
                print_json(&json!({"ok": false, "error": error}));
            } else {
                eprintln!("system-status failed: {error}");
            }
            1
        }
    }
}

fn run_set_password(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let json_output = has_flag(args, "--json");
    let Some(name) = first_non_flag(args) else {
        eprintln!("set-password requires container name");
        return 2;
    };
    let Some(user) = parse_option_value(args, "--user") else {
        eprintln!("set-password requires --user");
        return 2;
    };
    if !has_flag(args, "--stdin") {
        eprintln!("set-password requires --stdin");
        return 2;
    }
    match read_password_from_stdin()
        .and_then(|password| set_container_password(&config, name, user, &password))
    {
        Ok(()) => {
            if json_output {
                print_json(&json!({"ok": true, "container": name, "user": user}));
            } else {
                println!("container={name} user={user} password_updated=1");
            }
            0
        }
        Err(error) => {
            if json_output {
                print_json(&json!({"ok": false, "error": error}));
            } else {
                eprintln!("set-password failed: {error}");
            }
            1
        }
    }
}

fn run_generate_password(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let json_output = has_flag(args, "--json");
    let Some(name) = first_non_flag(args) else {
        eprintln!("generate-password requires container name");
        return 2;
    };
    let Some(user) = parse_option_value(args, "--user") else {
        eprintln!("generate-password requires --user");
        return 2;
    };
    match generate_password().and_then(|password| {
        set_container_password(&config, name, user, &password)?;
        Ok(password)
    }) {
        Ok(password) => {
            if json_output {
                print_json(
                    &json!({"ok": true, "container": name, "user": user, "password": password}),
                );
            } else {
                println!("container={name} user={user} password={password}");
            }
            0
        }
        Err(error) => {
            if json_output {
                print_json(&json!({"ok": false, "error": error}));
            } else {
                eprintln!("generate-password failed: {error}");
            }
            1
        }
    }
}

fn run_exec(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("exec requires container name");
        return 2;
    };
    let command = if args.get(1).map(String::as_str) == Some("--") {
        &args[2..]
    } else {
        &args[1..]
    };
    if command.is_empty() {
        eprintln!("exec requires command after container name");
        return 2;
    }
    match exec_container(&config, name, command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("exec failed: {error}");
            1
        }
    }
}

fn run_logs(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("logs requires container name");
        return 2;
    };
    let lines = parse_lines_arg(&args[1..]).unwrap_or(200);
    match read_container_logs(&config, name, lines) {
        Ok(text) => {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
            0
        }
        Err(error) => {
            eprintln!("logs failed: {error}");
            1
        }
    }
}

fn run_smoke(args: &[String]) -> i32 {
    let config = LxcConfig::from_env();
    let Some(name) = args.first().map(String::as_str) else {
        eprintln!("smoke requires container name");
        return 2;
    };
    match smoke_container(&config, name) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("smoke failed: {error}");
            1
        }
    }
}

fn write_lxc_configs(config: &LxcConfig) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(&config.lxc_etc)
        .map_err(|error| format!("create {}: {error}", config.lxc_etc.display()))?;
    fs::create_dir_all(&config.lxc_containers)
        .map_err(|error| format!("create {}: {error}", config.lxc_containers.display()))?;
    fs::create_dir_all(&config.lxc_log)
        .map_err(|error| format!("create {}: {error}", config.lxc_log.display()))?;
    fs::create_dir_all(&config.lxc_run)
        .map_err(|error| format!("create {}: {error}", config.lxc_run.display()))?;
    let common = config.lxc_etc.join("android-common.conf");
    let default = config.lxc_etc.join("default.conf");
    let unprivileged = config.lxc_etc.join("unprivileged.conf");
    fs::write(&common, android_common_config())
        .map_err(|error| format!("write {}: {error}", common.display()))?;
    fs::write(&default, default_config(config))
        .map_err(|error| format!("write {}: {error}", default.display()))?;
    fs::write(&unprivileged, unprivileged_config(config))
        .map_err(|error| format!("write {}: {error}", unprivileged.display()))?;
    Ok(vec![common, default, unprivileged])
}

fn android_common_config() -> &'static str {
    "lxc.mount.auto = proc:mixed sys:ro cgroup:mixed\nlxc.pty.max = 1024\nlxc.tty.max = 4\nlxc.cap.drop =\nlxc.apparmor.profile = unconfined\nlxc.selinux.context = unconfined_u:unconfined_r:unconfined_t:s0\n"
}

fn default_config(config: &LxcConfig) -> String {
    format!(
        "lxc.include = {}/android-common.conf\nlxc.net.0.type = veth\nlxc.net.0.link = {}\nlxc.net.0.flags = up\nlxc.net.0.name = eth0\nlxc.start.auto = 0\n",
        config.lxc_etc.display(),
        config.bridge
    )
}

fn unprivileged_config(config: &LxcConfig) -> String {
    format!(
        "# ACHost unprivileged LXC is not supported yet.\nlxc.include = {}/default.conf\nlxc.start.auto = 0\n",
        config.lxc_etc.display()
    )
}

fn parse_import_args(args: &[String]) -> Result<ImportArgs, String> {
    let mut name: Option<String> = None;
    let mut rootfs_asset: Option<PathBuf> = None;
    let mut rootfs_sha256: Option<String> = None;
    let mut distro = "unknown".to_string();
    let mut release = "unknown".to_string();
    let mut arch = "unknown".to_string();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--name" => name = Some(value.clone()),
            "--rootfs-asset" => rootfs_asset = Some(PathBuf::from(value)),
            "--sha256" => rootfs_sha256 = Some(normalize_sha256(value)?),
            "--distro" => distro = value.clone(),
            "--release" => release = value.clone(),
            "--arch" => arch = value.clone(),
            _ => return Err(format!("unsupported import-rootfs argument: {flag}")),
        }
        index += 2;
    }
    let name = name.ok_or_else(|| "--name is required".to_string())?;
    validate_container_name(&name)?;
    for label in [&distro, &release, &arch] {
        validate_label(label)?;
    }
    let rootfs_asset = rootfs_asset.ok_or_else(|| "--rootfs-asset is required".to_string())?;
    if !rootfs_asset.is_file() {
        return Err(format!(
            "rootfs asset not found: {}",
            rootfs_asset.display()
        ));
    }
    Ok(ImportArgs {
        name,
        rootfs_asset,
        rootfs_sha256,
        distro,
        release,
        arch,
    })
}

fn import_rootfs(config: &LxcConfig, args: &ImportArgs) -> Result<ContainerMetadata, String> {
    ensure_runtime_dirs(config)?;
    let rootfs_sha256 = sha256_file(&args.rootfs_asset)?;
    if let Some(expected) = &args.rootfs_sha256 {
        if expected != &rootfs_sha256 {
            return Err(format!(
                "rootfs sha256 mismatch: expected {expected} actual {rootfs_sha256}"
            ));
        }
    }
    let container_dir = config.container_dir(&args.name);
    if container_dir.exists() {
        return Err(format!("container already exists: {}", args.name));
    }
    let temp_dir = config
        .lxc_containers
        .join(format!(".{}.tmp.{}", args.name, std::process::id()));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|error| format!("remove stale {}: {error}", temp_dir.display()))?;
    }
    fs::create_dir_all(&temp_dir)
        .map_err(|error| format!("create {}: {error}", temp_dir.display()))?;
    let rootfs = temp_dir.join("rootfs");
    fs::create_dir_all(&rootfs).map_err(|error| format!("create {}: {error}", rootfs.display()))?;

    let result = (|| {
        extract_rootfs_asset(&args.rootfs_asset, &rootfs)?;
        let metadata = ContainerMetadata {
            name: args.name.clone(),
            distro: args.distro.clone(),
            release: args.release.clone(),
            arch: args.arch.clone(),
            rootfs: config.container_rootfs(&args.name).display().to_string(),
            rootfs_sha256: Some(rootfs_sha256.clone()),
            init_cmd: default_init_cmd(&rootfs),
            created_at_unix: unix_now(),
        };
        ensure_guest_network_config(&rootfs, &container_network_settings(config, &args.name))?;
        let config_text = container_config_text(config, &metadata);
        fs::write(temp_dir.join("config"), config_text)
            .map_err(|error| format!("write container config: {error}"))?;
        let metadata_text = serde_json::to_string_pretty(&metadata)
            .map_err(|error| format!("encode metadata: {error}"))?;
        fs::write(temp_dir.join(METADATA_FILE), metadata_text + "\n")
            .map_err(|error| format!("write metadata: {error}"))?;
        fs::rename(&temp_dir, &container_dir).map_err(|error| {
            format!(
                "install container {} -> {}: {error}",
                temp_dir.display(),
                container_dir.display()
            )
        })?;
        Ok(metadata)
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    result
}

fn container_config_text(config: &LxcConfig, metadata: &ContainerMetadata) -> String {
    let network = container_network_settings(config, &metadata.name);
    let mut text = format!(
        "lxc.include = {}/default.conf\nlxc.rootfs.path = dir:{}\nlxc.uts.name = {}\nlxc.log.file = {}\nlxc.log.level = INFO\nlxc.net.0.ipv4.address = {}/{}\nlxc.net.0.ipv4.gateway = {}\n",
        config.lxc_etc.display(),
        metadata.rootfs,
        metadata.name,
        config.container_log(&metadata.name).display(),
        network.address,
        network.prefix,
        network.gateway
    );
    if let Some(init_cmd) = &metadata.init_cmd {
        text.push_str("lxc.init.cmd = ");
        text.push_str(init_cmd);
        text.push('\n');
    }
    text
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ContainerNetworkSettings {
    address: Ipv4Addr,
    gateway: Ipv4Addr,
    prefix: u8,
    dns: Vec<Ipv4Addr>,
}

fn container_network_settings(config: &LxcConfig, name: &str) -> ContainerNetworkSettings {
    let (base, prefix) =
        parse_ipv4_cidr(&config.subnet).unwrap_or((Ipv4Addr::new(172, 32, 0, 0), 16));
    let mut octets = base.octets();
    octets[3] = 1;
    let gateway = Ipv4Addr::from(octets);
    let hash = stable_name_hash(name);
    let mut address = octets;
    address[2] = ((hash / 253) % 253 + 1) as u8;
    address[3] = (hash % 253 + 2) as u8;
    if Ipv4Addr::from(address) == gateway {
        address[3] = 2;
    }
    ContainerNetworkSettings {
        address: Ipv4Addr::from(address),
        gateway,
        prefix,
        dns: vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)],
    }
}

fn parse_ipv4_cidr(value: &str) -> Option<(Ipv4Addr, u8)> {
    let (addr, prefix) = value.split_once('/')?;
    let addr = addr.parse().ok()?;
    let prefix = prefix.parse().ok()?;
    if prefix <= 32 {
        Some((addr, prefix))
    } else {
        None
    }
}

fn stable_name_hash(value: &str) -> u32 {
    let mut hash = 2166136261u32;
    for byte in value.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

fn default_init_cmd(rootfs: &Path) -> Option<String> {
    if rootfs.join("sbin/init").exists() {
        None
    } else if rootfs.join("bin/sleep").exists() {
        Some("/bin/sleep 3600".to_string())
    } else {
        None
    }
}

fn ensure_runtime_dirs(config: &LxcConfig) -> Result<(), String> {
    for path in [
        &config.lxc_containers,
        &config.lxc_log,
        &config.lxc_run,
        &config.lxc_rootfs,
    ] {
        fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    }
    Ok(())
}

fn extract_rootfs_asset(asset: &Path, rootfs: &Path) -> Result<usize, String> {
    let file = File::open(asset).map_err(|error| format!("open {}: {error}", asset.display()))?;
    if is_gzip_asset(asset) {
        let decoder = GzDecoder::new(file);
        extract_tar_stream(decoder, rootfs)
    } else {
        extract_tar_stream(file, rootfs)
    }
}

fn is_gzip_asset(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| matches!(ext, "gz" | "tgz"))
        || path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.ends_with(".tar.gz") || name.ends_with(".tgz"))
}

fn extract_tar_stream<R: Read>(reader: R, rootfs: &Path) -> Result<usize, String> {
    let mut archive = tar::Archive::new(reader);
    let mut count = 0;
    let entries = archive
        .entries()
        .map_err(|error| format!("read tar entries: {error}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| format!("read tar entry: {error}"))?;
        let entry_type = entry.header().entry_type();
        let entry_path = entry
            .path()
            .map_err(|error| format!("read tar path: {error}"))?;
        let Some(relative) = safe_relative_path(&entry_path) else {
            return Err(format!("unsafe rootfs path: {}", entry_path.display()));
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = rootfs.join(&relative);
        if has_symlink_ancestor(rootfs, &relative)? || path_is_symlink(&target) {
            return Err(format!(
                "refusing symlink escape path: {}",
                relative.display()
            ));
        }
        if entry_type.is_file() || entry_type.is_dir() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            entry
                .unpack(&target)
                .map_err(|error| format!("unpack {}: {error}", relative.display()))?;
            count += 1;
        } else if entry_type.is_symlink() {
            let link = entry
                .link_name()
                .map_err(|error| format!("read symlink target: {error}"))?
                .ok_or_else(|| format!("symlink without target: {}", relative.display()))?;
            if normalize_link_target(relative.parent().unwrap_or_else(|| Path::new("")), &link)
                .is_none()
            {
                return Err(format!(
                    "unsafe symlink target: {} -> {}",
                    relative.display(),
                    link.display()
                ));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            symlink(&link, &target)
                .map_err(|error| format!("symlink {}: {error}", relative.display()))?;
            count += 1;
        } else if entry_type.is_hard_link() {
            let link = entry
                .link_name()
                .map_err(|error| format!("read hardlink target: {error}"))?
                .ok_or_else(|| format!("hardlink without target: {}", relative.display()))?;
            let Some(link_relative) = normalize_link_target(Path::new(""), &link) else {
                return Err(format!(
                    "unsafe hardlink target: {} -> {}",
                    relative.display(),
                    link.display()
                ));
            };
            if has_symlink_ancestor(rootfs, &link_relative)? {
                return Err(format!(
                    "hardlink target crosses symlink: {}",
                    link.display()
                ));
            }
            let link_target = rootfs.join(&link_relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::hard_link(&link_target, &target).map_err(|error| {
                format!(
                    "hardlink {} -> {}: {error}",
                    relative.display(),
                    link_relative.display()
                )
            })?;
            count += 1;
        } else {
            return Err(format!(
                "unsupported rootfs entry type at {}",
                relative.display()
            ));
        }
    }
    Ok(count)
}

fn safe_relative_path(path: &Path) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => relative.push(value),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(relative)
}

fn normalize_link_target(parent: &Path, target: &Path) -> Option<PathBuf> {
    let candidate = if target.is_absolute() {
        target.strip_prefix(Path::new("/")).ok()?.to_path_buf()
    } else {
        parent.join(target)
    };
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(normalized)
}

fn has_symlink_ancestor(root: &Path, relative: &Path) -> Result<bool, String> {
    let mut current = root.to_path_buf();
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    for component in parent.components() {
        if let Component::Normal(value) = component {
            current.push(value);
            if path_is_symlink(&current) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn path_is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn list_containers(config: &LxcConfig) -> Result<Vec<ContainerStatus>, String> {
    if !config.lxc_containers.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&config.lxc_containers)
        .map_err(|error| format!("read {}: {error}", config.lxc_containers.display()))?
    {
        let entry = entry.map_err(|error| format!("read container entry: {error}"))?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || validate_container_name(&name).is_err() {
            continue;
        }
        if entry.path().join("config").exists() || entry.path().join(METADATA_FILE).exists() {
            names.push(name);
        }
    }
    names.sort();
    let mut containers = Vec::new();
    for name in names {
        containers.push(container_status(config, &name)?);
    }
    Ok(containers)
}

fn container_status(config: &LxcConfig, name: &str) -> Result<ContainerStatus, String> {
    validate_container_name(name)?;
    let container_dir = config.container_dir(name);
    if !container_dir.exists() {
        return Err(format!("container not found: {name}"));
    }
    let metadata = read_container_metadata(config, name)
        .unwrap_or_else(|| fallback_container_metadata(config, name));
    let info = lxc_info(config, name).unwrap_or_else(|_| LxcInfo {
        state: "UNKNOWN".to_string(),
        pid: None,
    });
    Ok(ContainerStatus {
        name: name.to_string(),
        state: info.state,
        pid: info.pid,
        distro: metadata.distro,
        release: metadata.release,
        arch: metadata.arch,
        rootfs: metadata.rootfs,
        config: config.container_config(name).display().to_string(),
        log: config.container_log(name).display().to_string(),
        metadata: config.container_metadata(name).display().to_string(),
        autostart: container_autostart(config, name),
    })
}

fn read_container_metadata(config: &LxcConfig, name: &str) -> Option<ContainerMetadata> {
    let text = fs::read_to_string(config.container_metadata(name)).ok()?;
    serde_json::from_str(&text).ok()
}

fn fallback_container_metadata(config: &LxcConfig, name: &str) -> ContainerMetadata {
    let rootfs = config.container_rootfs(name);
    let (distro, release) =
        infer_os_release(&rootfs).unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
    ContainerMetadata {
        name: name.to_string(),
        distro,
        release,
        arch: host_arch_label(),
        rootfs: rootfs.display().to_string(),
        rootfs_sha256: None,
        init_cmd: default_init_cmd(&rootfs),
        created_at_unix: 0,
    }
}

fn infer_os_release(rootfs: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(rootfs.join("etc/os-release")).ok()?;
    let mut distro = String::new();
    let mut release = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = unquote_os_release_value(value.trim());
        match key {
            "ID" => distro = value,
            "NAME" if distro.is_empty() => distro = value,
            "VERSION_ID" => release = value,
            "VERSION_CODENAME" if release.is_empty() => release = value,
            _ => {}
        }
    }
    if distro.is_empty() && release.is_empty() {
        None
    } else {
        if distro.is_empty() {
            distro = "unknown".to_string();
        }
        if release.is_empty() {
            release = "unknown".to_string();
        }
        Some((distro, release))
    }
}

fn unquote_os_release_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

fn host_arch_label() -> String {
    match env::consts::ARCH {
        "aarch64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn parse_toggle(value: &str) -> Option<bool> {
    match value {
        "on" | "1" | "true" => Some(true),
        "off" | "0" | "false" => Some(false),
        _ => None,
    }
}

fn container_autostart(config: &LxcConfig, name: &str) -> bool {
    fs::read_to_string(config.container_config(name))
        .map(|text| lxc_start_auto_value(&text))
        .unwrap_or(false)
}

fn lxc_start_auto_value(text: &str) -> bool {
    let mut value = false;
    for line in text.lines() {
        if let Some(enabled) = lxc_start_auto_line(line) {
            value = enabled;
        }
    }
    value
}

fn lxc_start_auto_line(line: &str) -> Option<bool> {
    let body = line.split('#').next().unwrap_or(line).trim();
    let (key, value) = body.split_once('=')?;
    if key.trim() == "lxc.start.auto" {
        parse_toggle(value.trim())
    } else {
        None
    }
}

fn set_container_autostart(config: &LxcConfig, name: &str, enabled: bool) -> Result<(), String> {
    validate_existing_container(config, name)?;
    let path = config.container_config(name);
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let updated = rewrite_lxc_start_auto(&text, enabled);
    fs::write(&path, updated).map_err(|error| format!("write {}: {error}", path.display()))
}

fn rewrite_lxc_start_auto(text: &str, enabled: bool) -> String {
    let mut lines = text
        .lines()
        .filter(|line| {
            let body = line.split('#').next().unwrap_or(line).trim();
            body.split_once('=')
                .map(|(key, _)| key.trim() != "lxc.start.auto")
                .unwrap_or(true)
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines.push(format!("lxc.start.auto = {}", bit(enabled)));
    let mut updated = lines.join("\n");
    updated.push('\n');
    updated
}

fn autostart_containers(config: &LxcConfig) -> Result<usize, String> {
    ensure_runtime_dirs(config)?;
    let containers = list_containers(config)?;
    let mut selected = 0usize;
    let mut failures = 0usize;
    for container in containers.iter().filter(|container| container.autostart) {
        selected += 1;
        if container.state == "RUNNING" {
            println!(
                "autostart_container={} status=already-running",
                container.name
            );
            continue;
        }
        match start_container(config, &container.name) {
            Ok(()) => println!("autostart_container={} status=started", container.name),
            Err(error) => {
                failures += 1;
                println!(
                    "autostart_container={} status=failed error={}",
                    container.name, error
                );
            }
        }
    }
    println!("autostart_selected={selected} failures={failures}");
    Ok(failures)
}

fn guest_system_status(config: &LxcConfig, name: &str) -> Result<GuestSystemStatus, String> {
    let status = container_status(config, name)?;
    if status.state != "RUNNING" {
        return Ok(GuestSystemStatus {
            name: status.name,
            state: status.state,
            distro: status.distro,
            release: status.release,
            arch: status.arch,
            hostname: String::new(),
            uptime: String::new(),
            pretty_name: String::new(),
            addresses: Vec::new(),
            message: "container is not running".to_string(),
        });
    }
    let result = run_guest_shell_capture(config, name, &guest_status_script())?;
    let mut guest = parse_guest_system_status(&status, &result.output);
    if !result.ok {
        guest.message = if result.output.is_empty() {
            "system status command failed".to_string()
        } else {
            result.output
        };
    }
    Ok(guest)
}

fn guest_status_script() -> String {
    String::from(
        r#"hostname_value="$(hostname 2>/dev/null || true)"
printf 'hostname=%s\n' "$hostname_value"
if [ -r /proc/uptime ]; then
    IFS=' ' read -r up _ < /proc/uptime
    printf 'uptime=%s\n' "$up"
fi
if [ -r /etc/os-release ]; then
    while IFS='=' read -r key value || [ -n "$key" ]; do
        case "$key" in
            PRETTY_NAME|NAME|VERSION_ID)
                value="${value%\"}"
                value="${value#\"}"
                printf '%s=%s\n' "$key" "$value"
                ;;
        esac
    done < /etc/os-release
fi
if command -v ip >/dev/null 2>&1; then
    ip -o addr show scope global 2>/dev/null | while read -r _ _ _ _ addr _; do
        [ -n "$addr" ] && printf 'addr=%s\n' "$addr"
    done
fi
"#,
    )
}

fn parse_guest_system_status(status: &ContainerStatus, output: &str) -> GuestSystemStatus {
    let mut hostname = String::new();
    let mut uptime = String::new();
    let mut pretty_name = String::new();
    let mut addresses = Vec::new();
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "hostname" => hostname = value.to_string(),
            "uptime" => uptime = value.to_string(),
            "PRETTY_NAME" => pretty_name = value.to_string(),
            "NAME" if pretty_name.is_empty() => pretty_name = value.to_string(),
            "addr" if !value.is_empty() => addresses.push(value.to_string()),
            _ => {}
        }
    }
    GuestSystemStatus {
        name: status.name.clone(),
        state: status.state.clone(),
        distro: status.distro.clone(),
        release: status.release.clone(),
        arch: status.arch.clone(),
        hostname,
        uptime,
        pretty_name,
        addresses,
        message: String::new(),
    }
}

fn set_container_password(
    config: &LxcConfig,
    name: &str,
    user: &str,
    password: &str,
) -> Result<(), String> {
    validate_existing_container(config, name)?;
    validate_linux_user(user)?;
    validate_password(password)?;
    let shadow_path = config.container_rootfs(name).join("etc/shadow");
    ensure_child_path(&config.container_rootfs(name), &shadow_path)?;
    let salt = generate_shadow_salt()?;
    let hash = sha512_crypt(password, &salt, SHA512_CRYPT_ROUNDS);
    update_shadow_password(&shadow_path, user, &hash)
}

fn read_password_from_stdin() -> Result<String, String> {
    let mut password = String::new();
    std::io::stdin()
        .read_to_string(&mut password)
        .map_err(|error| format!("read password from stdin: {error}"))?;
    while password.ends_with('\n') || password.ends_with('\r') {
        password.pop();
    }
    validate_password(&password)?;
    Ok(password)
}

fn validate_linux_user(user: &str) -> Result<(), String> {
    if user.is_empty() || user.len() > 64 {
        return Err("invalid Linux user".to_string());
    }
    let mut chars = user.chars();
    let Some(first) = chars.next() else {
        return Err("invalid Linux user".to_string());
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err("invalid Linux user".to_string());
    }
    if chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')) {
        Ok(())
    } else {
        Err("invalid Linux user".to_string())
    }
}

fn validate_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("empty password".to_string());
    }
    if password.bytes().any(|byte| {
        byte == b':' || byte == 0 || byte == b'\n' || byte == b'\r' || byte.is_ascii_control()
    }) {
        return Err("password contains unsupported characters".to_string());
    }
    Ok(())
}

fn generate_password() -> Result<String, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789_-.!@#%+=";
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| format!("generate random password: {error}"))?;
    Ok(bytes
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect())
}

fn generate_shadow_salt() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| format!("generate shadow salt: {error}"))?;
    Ok(bytes
        .iter()
        .map(|byte| CRYPT_BASE64[*byte as usize % CRYPT_BASE64.len()] as char)
        .collect())
}

fn update_shadow_password(path: &Path, user: &str, hash: &str) -> Result<(), String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut found = false;
    let mut updated = String::new();
    for line in text.lines() {
        if let Some(rest) = line
            .strip_prefix(user)
            .and_then(|rest| rest.strip_prefix(':'))
        {
            let mut fields: Vec<&str> = rest.split(':').collect();
            if fields.is_empty() {
                return Err("invalid /etc/shadow entry".to_string());
            }
            fields[0] = hash;
            if fields.len() > 1 {
                let days = unix_now() / 86_400;
                let days_text = days.to_string();
                updated.push_str(user);
                updated.push(':');
                updated.push_str(&fields[0..1].join(":"));
                updated.push(':');
                updated.push_str(&days_text);
                if fields.len() > 2 {
                    updated.push(':');
                    updated.push_str(&fields[2..].join(":"));
                }
                updated.push('\n');
            } else {
                updated.push_str(user);
                updated.push(':');
                updated.push_str(hash);
                updated.push('\n');
            }
            found = true;
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    if !found {
        return Err(format!("user {user} not found in /etc/shadow"));
    }
    let temp = path.with_extension("shadow.achost-tmp");
    fs::write(&temp, updated).map_err(|error| format!("write {}: {error}", temp.display()))?;
    fs::set_permissions(&temp, metadata.permissions())
        .map_err(|error| format!("chmod {}: {error}", temp.display()))?;
    chown_path(&temp, metadata.uid(), metadata.gid())?;
    fs::rename(&temp, path).map_err(|error| format!("rename {}: {error}", temp.display()))?;
    Ok(())
}

fn chown_path(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("path contains NUL: {}", path.display()))?;
    let rc = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!(
            "chown {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ))
    }
}

fn sha512_crypt(password: &str, salt: &str, rounds: usize) -> String {
    let password = password.as_bytes();
    let salt = salt.as_bytes();
    let mut ctx = Sha512::new();
    ctx.update(password);
    ctx.update(salt);
    let mut alt_ctx = Sha512::new();
    alt_ctx.update(password);
    alt_ctx.update(salt);
    alt_ctx.update(password);
    let alt_result: [u8; 64] = alt_ctx.finalize().into();
    update_repeated(&mut ctx, &alt_result, password.len());
    let mut count = password.len();
    while count > 0 {
        if count & 1 == 1 {
            ctx.update(alt_result);
        } else {
            ctx.update(password);
        }
        count >>= 1;
    }
    let mut digest: [u8; 64] = ctx.finalize().into();
    let mut p_ctx = Sha512::new();
    for _ in 0..password.len() {
        p_ctx.update(password);
    }
    let p_digest: [u8; 64] = p_ctx.finalize().into();
    let p_bytes = repeat_digest_to_len(&p_digest, password.len());
    let mut s_ctx = Sha512::new();
    for _ in 0..(16 + digest[0] as usize) {
        s_ctx.update(salt);
    }
    let s_digest: [u8; 64] = s_ctx.finalize().into();
    let s_bytes = repeat_digest_to_len(&s_digest, salt.len());
    for round in 0..rounds {
        let mut round_ctx = Sha512::new();
        if round % 2 == 0 {
            round_ctx.update(digest);
        } else {
            round_ctx.update(&p_bytes);
        }
        if round % 3 != 0 {
            round_ctx.update(&s_bytes);
        }
        if round % 7 != 0 {
            round_ctx.update(&p_bytes);
        }
        if round % 2 == 0 {
            round_ctx.update(&p_bytes);
        } else {
            round_ctx.update(digest);
        }
        digest = round_ctx.finalize().into();
    }
    format!(
        "$6${}${}",
        String::from_utf8_lossy(salt),
        sha512_crypt_encode(&digest)
    )
}

fn update_repeated(ctx: &mut Sha512, bytes: &[u8], len: usize) {
    let mut remaining = len;
    while remaining > bytes.len() {
        ctx.update(bytes);
        remaining -= bytes.len();
    }
    if remaining > 0 {
        ctx.update(&bytes[..remaining]);
    }
}

fn repeat_digest_to_len(digest: &[u8; 64], len: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(len);
    while output.len() < len {
        let remaining = len - output.len();
        output.extend_from_slice(&digest[..remaining.min(digest.len())]);
    }
    output
}

fn sha512_crypt_encode(digest: &[u8; 64]) -> String {
    let mut output = String::with_capacity(86);
    crypt_b64(&mut output, digest[0], digest[21], digest[42], 4);
    crypt_b64(&mut output, digest[22], digest[43], digest[1], 4);
    crypt_b64(&mut output, digest[44], digest[2], digest[23], 4);
    crypt_b64(&mut output, digest[3], digest[24], digest[45], 4);
    crypt_b64(&mut output, digest[25], digest[46], digest[4], 4);
    crypt_b64(&mut output, digest[47], digest[5], digest[26], 4);
    crypt_b64(&mut output, digest[6], digest[27], digest[48], 4);
    crypt_b64(&mut output, digest[28], digest[49], digest[7], 4);
    crypt_b64(&mut output, digest[50], digest[8], digest[29], 4);
    crypt_b64(&mut output, digest[9], digest[30], digest[51], 4);
    crypt_b64(&mut output, digest[31], digest[52], digest[10], 4);
    crypt_b64(&mut output, digest[53], digest[11], digest[32], 4);
    crypt_b64(&mut output, digest[12], digest[33], digest[54], 4);
    crypt_b64(&mut output, digest[34], digest[55], digest[13], 4);
    crypt_b64(&mut output, digest[56], digest[14], digest[35], 4);
    crypt_b64(&mut output, digest[15], digest[36], digest[57], 4);
    crypt_b64(&mut output, digest[37], digest[58], digest[16], 4);
    crypt_b64(&mut output, digest[59], digest[17], digest[38], 4);
    crypt_b64(&mut output, digest[18], digest[39], digest[60], 4);
    crypt_b64(&mut output, digest[40], digest[61], digest[19], 4);
    crypt_b64(&mut output, digest[62], digest[20], digest[41], 4);
    crypt_b64(&mut output, 0, 0, digest[63], 2);
    output
}

fn crypt_b64(output: &mut String, b2: u8, b1: u8, b0: u8, count: usize) {
    let mut value = ((b2 as u32) << 16) | ((b1 as u32) << 8) | b0 as u32;
    for _ in 0..count {
        output.push(CRYPT_BASE64[(value & 0x3f) as usize] as char);
        value >>= 6;
    }
}

#[derive(Debug)]
struct LxcInfo {
    state: String,
    pid: Option<String>,
}

fn lxc_info(config: &LxcConfig, name: &str) -> Result<LxcInfo, String> {
    let result = run_lxc_capture(
        config,
        "lxc-info",
        &[
            "-P",
            path_str(&config.lxc_containers)?,
            "-n",
            name,
            "-s",
            "-p",
        ],
    )?;
    if !result.ok {
        return Err(result.output);
    }
    let mut state = "UNKNOWN".to_string();
    let mut pid = None;
    for line in result.output.lines() {
        if let Some(value) = line.strip_prefix("State:") {
            state = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("PID:") {
            let value = value.trim();
            if !value.is_empty() && value != "-1" {
                pid = Some(value.to_string());
            }
        }
    }
    Ok(LxcInfo { state, pid })
}

fn wait_for_lxc_state(
    config: &LxcConfig,
    name: &str,
    expected: &str,
    timeout_secs: u64,
) -> Result<LxcInfo, String> {
    let mut last = "UNKNOWN".to_string();
    for _ in 0..timeout_secs {
        match lxc_info(config, name) {
            Ok(info) if info.state == expected => return Ok(info),
            Ok(info) => last = format!("{} pid={}", info.state, info.pid.unwrap_or_default()),
            Err(error) => last = error,
        }
        thread::sleep(Duration::from_secs(1));
    }
    Err(format!(
        "container {name} did not reach {expected}; last_status={last}"
    ))
}

fn lxc_state_label(config: &LxcConfig, name: &str) -> String {
    match lxc_info(config, name) {
        Ok(info) => format!("{} pid={}", info.state, info.pid.unwrap_or_default()),
        Err(error) => format!("UNKNOWN error={error}"),
    }
}

fn ensure_container_network(config: &LxcConfig, name: &str) -> Result<(), String> {
    let settings = container_network_settings(config, name);
    let path = config.container_config(name);
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let text = upsert_lxc_config_value(
        &text,
        "lxc.net.0.ipv4.address",
        &format!("{}/{}", settings.address, settings.prefix),
    );
    let text = upsert_lxc_config_value(
        &text,
        "lxc.net.0.ipv4.gateway",
        &settings.gateway.to_string(),
    );
    fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))?;
    ensure_guest_network_config(&config.container_rootfs(name), &settings)
}

fn upsert_lxc_config_value(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key} ");
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim_start().starts_with(&prefix))
        .collect();
    let mut output = String::new();
    for line in lines.drain(..) {
        output.push_str(line);
        output.push('\n');
    }
    output.push_str(key);
    output.push_str(" = ");
    output.push_str(value);
    output.push('\n');
    output
}

fn ensure_guest_network_config(
    rootfs: &Path,
    settings: &ContainerNetworkSettings,
) -> Result<(), String> {
    write_guest_resolv_conf(rootfs, settings)?;
    write_guest_netplan(rootfs, settings)?;
    write_guest_systemd_network(rootfs, settings)?;
    Ok(())
}

fn write_guest_resolv_conf(
    rootfs: &Path,
    settings: &ContainerNetworkSettings,
) -> Result<(), String> {
    let path = rootfs.join("etc/resolv.conf");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    if fs::symlink_metadata(&path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        fs::remove_file(&path).map_err(|error| format!("remove {}: {error}", path.display()))?;
    }
    let mut text = String::new();
    for dns in &settings.dns {
        text.push_str("nameserver ");
        text.push_str(&dns.to_string());
        text.push('\n');
    }
    fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn write_guest_netplan(rootfs: &Path, settings: &ContainerNetworkSettings) -> Result<(), String> {
    let netplan = rootfs.join("etc/netplan");
    if !netplan.exists() {
        return Ok(());
    }
    fs::create_dir_all(&netplan)
        .map_err(|error| format!("create {}: {error}", netplan.display()))?;
    let path = if netplan.join("10-lxc.yaml").exists() {
        netplan.join("10-lxc.yaml")
    } else {
        netplan.join("10-achost-lxc.yaml")
    };
    let dns = settings
        .dns
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "network:\n  version: 2\n  renderer: networkd\n  ethernets:\n    eth0:\n      dhcp4: false\n      addresses: [{}/{}]\n      routes:\n        - to: default\n          via: {}\n      nameservers:\n        addresses: [{}]\n",
        settings.address, settings.prefix, settings.gateway, dns
    );
    fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn write_guest_systemd_network(
    rootfs: &Path,
    settings: &ContainerNetworkSettings,
) -> Result<(), String> {
    if !rootfs.join("etc/systemd").exists() {
        return Ok(());
    }
    let dir = rootfs.join("etc/systemd/network");
    fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    let mut text = format!(
        "[Match]\nName=eth0\n\n[Network]\nAddress={}/{}\nGateway={}\n",
        settings.address, settings.prefix, settings.gateway
    );
    for dns in &settings.dns {
        text.push_str("DNS=");
        text.push_str(&dns.to_string());
        text.push('\n');
    }
    let path = dir.join("10-achost-lxc.network");
    fs::write(&path, text).map_err(|error| format!("write {}: {error}", path.display()))
}

fn prepare_container_log(config: &LxcConfig, name: &str) -> Result<PathBuf, String> {
    let log = config.container_log(name);
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    File::create(&log).map_err(|error| format!("truncate {}: {error}", log.display()))?;
    Ok(log)
}

fn start_container(config: &LxcConfig, name: &str) -> Result<(), String> {
    validate_existing_container(config, name)?;
    ensure_container_network(config, name)?;
    if lxc_info(config, name).is_ok_and(|info| info.state == "RUNNING") {
        println!("container_state=RUNNING");
        return Ok(());
    }
    if run_prepare_bridge() != 0 {
        return Err("prepare-bridge failed".to_string());
    }
    let log = prepare_container_log(config, name)?;
    let result = run_lxc_capture(
        config,
        "lxc-start",
        &[
            "-P",
            path_str(&config.lxc_containers)?,
            "-n",
            name,
            "-d",
            "-o",
            path_str(&log)?,
            "-l",
            "INFO",
        ],
    )?;
    if !result.ok {
        return Err(format!(
            "{}; log={}; try: achost-lxc-runtime logs {}",
            result.output,
            log.display(),
            name
        ));
    }
    let info = wait_for_lxc_state(config, name, "RUNNING", 20).map_err(|error| {
        format!(
            "{error}; log={}; try: achost-lxc-runtime logs {}",
            log.display(),
            name
        )
    })?;
    println!(
        "container_state={} pid={}",
        info.state,
        info.pid.unwrap_or_default()
    );
    Ok(())
}

fn stop_container(config: &LxcConfig, name: &str, force: bool) -> Result<(), String> {
    validate_existing_container(config, name)?;
    if lxc_info(config, name).is_ok_and(|info| info.state == "STOPPED") {
        println!("container_state=STOPPED");
        return Ok(());
    }
    let containers = path_str(&config.lxc_containers)?;
    let args = if force {
        vec!["-P", containers, "-n", name, "-k"]
    } else {
        vec!["-P", containers, "-n", name, "-t", "5"]
    };
    let result = run_lxc_capture(config, "lxc-stop", &args)?;
    let log = config.container_log(name);
    if !result.ok {
        if lxc_info(config, name).is_ok_and(|info| info.state == "STOPPED") {
            println!("container_state=STOPPED");
            return Ok(());
        }
        return Err(format!(
            "{}; state={}; log={}",
            result.output,
            lxc_state_label(config, name),
            log.display()
        ));
    }
    let info = wait_for_lxc_state(config, name, "STOPPED", if force { 10 } else { 15 })
        .map_err(|error| format!("{error}; log={}", log.display()))?;
    println!("container_state={}", info.state);
    Ok(())
}

fn destroy_container(config: &LxcConfig, name: &str) -> Result<(), String> {
    validate_existing_container(config, name)?;
    if container_status(config, name)
        .map(|status| status.state == "RUNNING")
        .unwrap_or(false)
    {
        let _ = stop_container(config, name, true);
    }
    if find_lxc_binary(config, "lxc-destroy").is_ok() {
        let _ = run_lxc_capture(
            config,
            "lxc-destroy",
            &["-P", path_str(&config.lxc_containers)?, "-n", name],
        );
    }
    let dir = config.container_dir(name);
    ensure_child_path(&config.lxc_containers, &dir)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| format!("remove {}: {error}", dir.display()))?;
    }
    Ok(())
}

fn exec_container(config: &LxcConfig, name: &str, command: &[String]) -> Result<i32, String> {
    validate_existing_container(config, name)?;
    let mut args = vec![
        "-P".to_string(),
        path_string(&config.lxc_containers),
        "-n".to_string(),
        name.to_string(),
        "--".to_string(),
    ];
    args.extend(command.iter().cloned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut cmd = lxc_command(config, "lxc-attach", &arg_refs)?;
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = cmd
        .status()
        .map_err(|error| format!("run lxc-attach: {error}"))?;
    Ok(status.code().unwrap_or(1))
}

fn run_guest_shell_capture(
    config: &LxcConfig,
    name: &str,
    script: &str,
) -> Result<CommandResult, String> {
    let wrapped = guest_shell_script(script);
    run_guest_capture(config, name, &["/bin/sh", "-c", &wrapped])
}

fn guest_shell_script(script: &str) -> String {
    format!("PATH={GUEST_PATH}; export PATH; {script}")
}

fn run_guest_capture(
    config: &LxcConfig,
    name: &str,
    command: &[&str],
) -> Result<CommandResult, String> {
    validate_existing_container(config, name)?;
    let mut args = vec![
        "-P".to_string(),
        path_string(&config.lxc_containers),
        "-n".to_string(),
        name.to_string(),
        "--clear-env".to_string(),
        "--".to_string(),
    ];
    args.extend(command.iter().map(|item| item.to_string()));
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = lxc_command(config, "lxc-attach", &arg_refs)?
        .output()
        .map_err(|error| format!("run lxc-attach: {error}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(CommandResult {
        ok: output.status.success(),
        output: trim_trailing_newlines(text),
    })
}

fn read_container_logs(config: &LxcConfig, name: &str, lines: usize) -> Result<String, String> {
    validate_existing_container(config, name)?;
    let path = config.container_log(name);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(format!("no log file: {}\n", path.display()));
        }
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    Ok(last_lines(&text, lines))
}

fn smoke_container(config: &LxcConfig, name: &str) -> Result<(), String> {
    validate_existing_container(config, name)?;
    let status = container_status(config, name)?;
    println!("name={} state={}", status.name, status.state);
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("PATH={GUEST_PATH}; export PATH; cat /etc/os-release 2>/dev/null || true; uname -a 2>/dev/null || true"),
    ];
    let code = exec_container(config, name, &command)?;
    if code == 0 {
        Ok(())
    } else {
        Err(format!("smoke command exit={code}"))
    }
}

fn validate_existing_container(config: &LxcConfig, name: &str) -> Result<(), String> {
    validate_container_name(name)?;
    let dir = config.container_dir(name);
    if !dir.exists() {
        return Err(format!("container not found: {name}"));
    }
    ensure_child_path(&config.lxc_containers, &dir)?;
    Ok(())
}

fn validate_container_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.contains("..") {
        return Err("invalid container name".to_string());
    }
    if name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(format!("invalid container name: {name}"))
    }
}

fn validate_label(label: &str) -> Result<(), String> {
    if !label.is_empty()
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(format!("invalid label: {label}"))
    }
}

fn normalize_sha256(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err("invalid sha256".to_string())
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn ensure_child_path(root: &Path, child: &Path) -> Result<(), String> {
    let relative = child
        .strip_prefix(root)
        .map_err(|_| format!("path escaped root: {}", child.display()))?;
    if safe_relative_path(relative).is_none() {
        return Err(format!("unsafe child path: {}", child.display()));
    }
    Ok(())
}

fn find_lxc_binary(config: &LxcConfig, name: &str) -> Result<PathBuf, String> {
    candidate_binary_paths(config, name)
        .into_iter()
        .find(|path| path.exists() && is_executable(path))
        .ok_or_else(|| format!("{name} not executable"))
}

fn run_lxc_capture(config: &LxcConfig, name: &str, args: &[&str]) -> Result<CommandResult, String> {
    let output = lxc_command(config, name, args)?
        .output()
        .map_err(|error| format!("run {name}: {error}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(CommandResult {
        ok: output.status.success(),
        output: trim_trailing_newlines(text),
    })
}

fn lxc_command(config: &LxcConfig, name: &str, args: &[&str]) -> Result<Command, String> {
    let program = find_lxc_binary(config, name)?;
    let mut command = if config.supervise.exists() && is_executable(&config.supervise) {
        let mut command = Command::new(&config.supervise);
        command
            .arg("--launch")
            .arg("--native-root")
            .arg(&config.native_root)
            .arg("--close-range-enosys")
            .arg("--")
            .arg(program);
        command
    } else {
        Command::new(program)
    };
    apply_lxc_env(config, &mut command);
    command.args(args);
    Ok(command)
}

fn apply_lxc_env(config: &LxcConfig, command: &mut Command) {
    let path = format!(
        "{}:{}:{}",
        config.lxc_bin.display(),
        config.achost_bin.display(),
        env::var("PATH").unwrap_or_default()
    );
    let ld_library_path = format!(
        "{}:{}:{}:{}",
        config.lxc_root.join("lib").display(),
        config.lxc_root.join("lib64").display(),
        config.achost.join("lib").display(),
        env::var("LD_LIBRARY_PATH").unwrap_or_default()
    );
    command
        .env("PATH", path)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .env("ACHOST_LXC", &config.lxc_root)
        .env("ACHOST_LXC_BIN", &config.lxc_bin)
        .env("ACHOST_LXC_ETC", &config.lxc_etc)
        .env("ACHOST_LXC_RUN", &config.lxc_run)
        .env("ACHOST_LXC_LOG", &config.lxc_log)
        .env("ACHOST_LXC_ROOTFS", &config.lxc_rootfs)
        .env("ACHOST_LXC_CONTAINERS", &config.lxc_containers)
        .env("ACHOST_NATIVE_ROOT", &config.native_root)
        .env("ACHOST_SUPERVISE", &config.supervise)
        .env("LXC_BRIDGE", &config.bridge)
        .env("LXC_SUBNET", &config.subnet);
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("non-utf8 path: {}", path.display()))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn section(title: &str) {
    println!("\n## {title}");
}

fn print_config_paths(config: &LxcConfig) {
    println!("ACHOST={}", config.achost.display());
    println!("ACHOST_BIN={}", config.achost_bin.display());
    println!("ACHOST_COMMON_BIN={}", config.common_bin.display());
    println!("ACHOST_LXC_MODULE={}", config.lxc_module.display());
    println!("ACHOST_LXC={}", config.lxc_root.display());
    println!("ACHOST_LXC_BIN={}", config.lxc_bin.display());
    println!("ACHOST_LXC_ETC={}", config.lxc_etc.display());
    println!("ACHOST_LXC_VAR={}", config.lxc_var.display());
    println!("ACHOST_LXC_RUN={}", config.lxc_run.display());
    println!("ACHOST_LXC_LOG={}", config.lxc_log.display());
    println!("ACHOST_LXC_ROOTFS={}", config.lxc_rootfs.display());
    println!("ACHOST_LXC_CONTAINERS={}", config.lxc_containers.display());
    println!("ACHOST_NATIVE_ROOT={}", config.native_root.display());
    println!("ACHOST_SUPERVISE={}", config.supervise.display());
    println!("LXC_BRIDGE={}", config.bridge);
    println!("LXC_SUBNET={}", config.subnet);
}

fn path_check(label: &str, path: &Path, required: bool) -> bool {
    let present = path.exists();
    let readable = fs::File::open(path).is_ok() || path.is_dir();
    println!(
        "{label}={} readable={} required={} path={}",
        if present { "present" } else { "missing" },
        bit(readable),
        bit(required),
        path.display()
    );
    !required || (present && readable)
}

fn print_selinux_status() {
    match read_to_string(Path::new("/sys/fs/selinux/enforce")) {
        Some(value) => {
            let mode = match value.trim() {
                "1" => "enforcing",
                "0" => "permissive",
                other => other,
            };
            println!("selinux=present mode={mode}");
        }
        None => println!("selinux=missing mode=unknown"),
    }
}

fn binary_report(config: &LxcConfig, name: &str, required: bool) -> BinaryReport {
    for path in candidate_binary_paths(config, name) {
        if !path.exists() {
            continue;
        }
        let executable = is_executable(&path);
        let elf = if path.is_file() {
            read_elf_info(&path).unwrap_or_else(ElfInfo::none)
        } else {
            ElfInfo::none()
        };
        return BinaryReport {
            name: name.to_string(),
            required,
            path: Some(path),
            executable,
            elf,
        };
    }
    BinaryReport {
        name: name.to_string(),
        required,
        path: None,
        executable: false,
        elf: ElfInfo::none(),
    }
}

fn print_binary_report(report: &BinaryReport) {
    match &report.path {
        Some(path) => println!(
            "{}=found path={} executable={} required={} elf={} arch={} interpreter={}",
            report.name,
            path.display(),
            bit(report.executable),
            bit(report.required),
            bit(report.elf.is_elf),
            report.elf.arch.as_deref().unwrap_or("none"),
            report.elf.interpreter.as_deref().unwrap_or("none")
        ),
        None => println!("{}=missing required={}", report.name, bit(report.required)),
    }
}

fn candidate_binary_paths(config: &LxcConfig, name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    push_unique(&mut paths, config.lxc_bin.join(name));
    push_unique(&mut paths, config.achost_bin.join(name));
    if let Some(path) = find_in_path(name) {
        push_unique(&mut paths, path);
    }
    paths
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

fn run_checkconfig(path: &Path) {
    match Command::new(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => println!("lxc_checkconfig=ran exit={}", status.code().unwrap_or(-1)),
        Err(error) => println!("lxc_checkconfig=failed error={error}"),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CgroupMount {
    path: String,
    fstype: String,
}

fn parse_cgroup_mounts(mounts: &str) -> Vec<CgroupMount> {
    mounts
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 3 || !matches!(fields[2], "cgroup" | "cgroup2") {
                return None;
            }
            Some(CgroupMount {
                path: fields[1].to_string(),
                fstype: fields[2].to_string(),
            })
        })
        .collect()
}

fn pick_command(candidates: &[&str]) -> Option<PathBuf> {
    for candidate in candidates {
        let path = if candidate.contains('/') {
            PathBuf::from(candidate)
        } else if let Some(path) = find_in_path(candidate) {
            path
        } else {
            continue;
        };
        if path.exists() && is_executable(&path) {
            return Some(path);
        }
    }
    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn read_elf_info(path: &Path) -> Option<ElfInfo> {
    let bytes = fs::read(path).ok()?;
    elf_info_from_bytes(&bytes)
}

fn elf_info_from_bytes(bytes: &[u8]) -> Option<ElfInfo> {
    if bytes.len() < 20 || &bytes[0..4] != b"\x7fELF" {
        return Some(ElfInfo::none());
    }
    if bytes.get(5).copied() != Some(1) {
        return Some(ElfInfo {
            is_elf: true,
            arch: Some("unknown-endian".to_string()),
            interpreter: None,
        });
    }
    let class = bytes.get(4).copied().unwrap_or(0);
    let arch = machine_name(read_u16_le(bytes, 18)?).to_string();
    let interpreter = match class {
        1 => elf_interpreter_32(bytes),
        2 => elf_interpreter_64(bytes),
        _ => None,
    };
    Some(ElfInfo {
        is_elf: true,
        arch: Some(arch),
        interpreter,
    })
}

fn elf_interpreter_64(bytes: &[u8]) -> Option<String> {
    let phoff = read_u64_le(bytes, 32)? as usize;
    let phentsize = read_u16_le(bytes, 54)? as usize;
    let phnum = read_u16_le(bytes, 56)? as usize;
    for index in 0..phnum {
        let start = phoff.checked_add(index.checked_mul(phentsize)?)?;
        if start.checked_add(56)? > bytes.len() || read_u32_le(bytes, start)? != 3 {
            continue;
        }
        let offset = read_u64_le(bytes, start + 8)? as usize;
        let size = read_u64_le(bytes, start + 32)? as usize;
        return read_c_string(bytes, offset, size);
    }
    None
}

fn elf_interpreter_32(bytes: &[u8]) -> Option<String> {
    let phoff = read_u32_le(bytes, 28)? as usize;
    let phentsize = read_u16_le(bytes, 42)? as usize;
    let phnum = read_u16_le(bytes, 44)? as usize;
    for index in 0..phnum {
        let start = phoff.checked_add(index.checked_mul(phentsize)?)?;
        if start.checked_add(32)? > bytes.len() || read_u32_le(bytes, start)? != 3 {
            continue;
        }
        let offset = read_u32_le(bytes, start + 4)? as usize;
        let size = read_u32_le(bytes, start + 16)? as usize;
        return read_c_string(bytes, offset, size);
    }
    None
}

fn read_c_string(bytes: &[u8], offset: usize, size: usize) -> Option<String> {
    let end = offset.checked_add(size)?.min(bytes.len());
    let slice = bytes.get(offset..end)?;
    let nul = slice
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(slice.len());
    String::from_utf8(slice[..nul].to_vec()).ok()
}

fn machine_name(machine: u16) -> &'static str {
    match machine {
        40 => "arm",
        62 => "x86_64",
        183 => "aarch64",
        243 => "riscv",
        _ => "unknown",
    }
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    let slice = bytes.get(offset..offset + 8)?;
    Some(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn read_to_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn bit(value: bool) -> u8 {
    u8::from(value)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn first_non_flag(args: &[String]) -> Option<&str> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
}

fn parse_option_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn parse_lines_arg(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--lines" {
            return args.get(index + 1).and_then(|value| value.parse().ok());
        }
        index += 1;
    }
    None
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string(value)
            .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"json encode failed\"}".to_string())
    );
}

fn last_lines(text: &str, count: usize) -> String {
    if count == 0 {
        return String::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    let mut output = lines[start..].join("\n");
    if text.ends_with('\n') && !output.is_empty() {
        output.push('\n');
    }
    output
}

fn trim_trailing_newlines(mut text: String) -> String {
    while text.ends_with('\n') || text.ends_with('\r') {
        text.pop();
    }
    text
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LxcConfig {
        LxcConfig {
            achost: PathBuf::from("/module/achost"),
            achost_bin: PathBuf::from("/module/achost/bin"),
            common_bin: PathBuf::from("/base/achost/bin"),
            lxc_module: PathBuf::from("/module/achost"),
            lxc_root: PathBuf::from("/module/achost/lxc"),
            lxc_bin: PathBuf::from("/module/achost/lxc/bin"),
            lxc_etc: PathBuf::from("/module/achost/etc/lxc"),
            lxc_var: PathBuf::from("/data/adb/achost/lxc"),
            lxc_run: PathBuf::from("/data/adb/achost/run/lxc"),
            lxc_log: PathBuf::from("/data/adb/achost/log/lxc"),
            lxc_rootfs: PathBuf::from("/data/adb/achost/lxc/rootfs"),
            lxc_containers: PathBuf::from("/data/adb/achost/lxc/containers"),
            native_root: PathBuf::from("/data/adb/achost/native-root"),
            supervise: PathBuf::from("/base/achost/bin/achost-supervise"),
            bridge: "lxcbr0".to_string(),
            subnet: "172.32.0.0/16".to_string(),
        }
    }

    fn temp_test_config(root: &Path) -> LxcConfig {
        let mut config = test_config();
        config.lxc_var = root.join("lxc");
        config.lxc_run = root.join("run/lxc");
        config.lxc_log = root.join("log/lxc");
        config.lxc_rootfs = root.join("lxc/rootfs");
        config.lxc_containers = root.join("lxc/containers");
        config
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "achost-{name}-{}-{}",
            std::process::id(),
            unix_now()
        ))
    }

    fn write_test_rootfs_tar(path: &Path) {
        let file = File::create(path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut dir = tar::Header::new_gnu();
        dir.set_entry_type(tar::EntryType::Directory);
        dir.set_path("etc").unwrap();
        dir.set_size(0);
        dir.set_mode(0o755);
        dir.set_cksum();
        builder.append(&dir, std::io::empty()).unwrap();

        let contents = b"NAME=Ubuntu\nVERSION_ID=26.04\n";
        let mut file_header = tar::Header::new_gnu();
        file_header.set_path("etc/os-release").unwrap();
        file_header.set_size(contents.len() as u64);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder.append(&file_header, &contents[..]).unwrap();
        builder.finish().unwrap();
    }

    #[test]
    fn candidate_paths_prefer_lxc_bin() {
        let paths = candidate_binary_paths(&test_config(), "lxc-start");

        assert_eq!(paths[0], PathBuf::from("/module/achost/lxc/bin/lxc-start"));
        assert_eq!(paths[1], PathBuf::from("/module/achost/bin/lxc-start"));
    }

    #[test]
    fn parses_cgroup_mounts() {
        let mounts = parse_cgroup_mounts(
            "tmpfs /dev tmpfs rw 0 0\ncgroup /dev/memcg cgroup rw,memory 0 0\ncgroup2 /sys/fs/cgroup cgroup2 rw 0 0\n",
        );

        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].path, "/dev/memcg");
        assert_eq!(mounts[1].fstype, "cgroup2");
    }

    #[test]
    fn renders_lxcbr0_default_config() {
        let config = test_config();
        let text = default_config(&config);

        assert!(text.contains("lxc.include = /module/achost/etc/lxc/android-common.conf"));
        assert!(text.contains("lxc.net.0.link = lxcbr0"));
    }

    #[test]
    fn renders_unprivileged_placeholder_as_unsupported() {
        let text = unprivileged_config(&test_config());

        assert!(text.contains("unprivileged LXC is not supported yet"));
        assert!(!text.contains("lxc.idmap"));
    }

    #[test]
    fn validates_container_names() {
        assert!(validate_container_name("ubuntu-26.04").is_ok());
        assert!(validate_container_name("demo_1").is_ok());
        assert!(validate_container_name("../bad").is_err());
        assert!(validate_container_name("bad/name").is_err());
        assert!(validate_container_name("").is_err());
    }

    #[test]
    fn validates_import_labels_and_sha256() {
        assert!(validate_label("ubuntu-26.04_arm64").is_ok());
        assert!(validate_label("bad/name").is_err());
        assert!(validate_label("").is_err());
        assert_eq!(
            normalize_sha256("ABCDEFabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123")
                .unwrap(),
            "abcdefabcdef0123456789abcdef0123456789abcdef0123456789abcdef0123"
        );
        assert!(normalize_sha256("not-a-sha").is_err());
    }

    #[test]
    fn imports_rootfs_with_matching_sha256() {
        let dir = temp_test_dir("import-sha");
        fs::create_dir_all(&dir).unwrap();
        let asset = dir.join("rootfs.tar");
        write_test_rootfs_tar(&asset);
        let sha256 = sha256_file(&asset).unwrap();
        let config = temp_test_config(&dir);
        let args = ImportArgs {
            name: "demo".to_string(),
            rootfs_asset: asset,
            rootfs_sha256: Some(sha256.clone()),
            distro: "ubuntu".to_string(),
            release: "26.04".to_string(),
            arch: "arm64".to_string(),
        };

        let metadata = import_rootfs(&config, &args).unwrap();

        assert_eq!(metadata.rootfs_sha256.as_deref(), Some(sha256.as_str()));
        assert!(config
            .container_rootfs("demo")
            .join("etc/os-release")
            .exists());
        let stored = read_container_metadata(&config, "demo").unwrap();
        assert_eq!(stored.rootfs_sha256.as_deref(), Some(sha256.as_str()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn infers_raw_container_metadata_from_os_release() {
        let dir = temp_test_dir("fallback-metadata");
        let config = temp_test_config(&dir);
        let rootfs_etc = config.container_rootfs("raw").join("etc");
        fs::create_dir_all(&rootfs_etc).unwrap();
        fs::write(
            rootfs_etc.join("os-release"),
            "ID=ubuntu\nVERSION_ID=\"26.04\"\nPRETTY_NAME=\"Ubuntu 26.04 LTS\"\n",
        )
        .unwrap();

        let metadata = fallback_container_metadata(&config, "raw");

        assert_eq!(metadata.distro, "ubuntu");
        assert_eq!(metadata.release, "26.04");
        assert_eq!(metadata.arch, host_arch_label());
        assert_eq!(
            metadata.rootfs,
            config.container_rootfs("raw").display().to_string()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prepare_container_log_truncates_previous_log() {
        let dir = temp_test_dir("log-truncate");
        let config = temp_test_config(&dir);
        fs::create_dir_all(&config.lxc_log).unwrap();
        let log = config.container_log("demo");
        fs::write(&log, "old failure\n").unwrap();

        let prepared = prepare_container_log(&config, "demo").unwrap();

        assert_eq!(prepared, log);
        assert_eq!(fs::read_to_string(&prepared).unwrap(), "");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_container_log_returns_message() {
        let dir = temp_test_dir("missing-log");
        let config = temp_test_config(&dir);
        fs::create_dir_all(config.container_dir("demo")).unwrap();

        let text = read_container_logs(&config, "demo", 20).unwrap();

        assert!(text.contains("no log file:"));
        assert!(text.contains("demo.log"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_mismatched_rootfs_sha256_before_import() {
        let dir = temp_test_dir("import-sha-mismatch");
        fs::create_dir_all(&dir).unwrap();
        let asset = dir.join("rootfs.tar");
        write_test_rootfs_tar(&asset);
        let config = temp_test_config(&dir);
        let args = ImportArgs {
            name: "demo".to_string(),
            rootfs_asset: asset,
            rootfs_sha256: Some("0".repeat(64)),
            distro: "ubuntu".to_string(),
            release: "26.04".to_string(),
            arch: "arm64".to_string(),
        };

        let error = import_rootfs(&config, &args).unwrap_err();

        assert!(error.contains("rootfs sha256 mismatch"));
        assert!(!config.container_dir("demo").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unsafe_tar_paths() {
        assert_eq!(
            safe_relative_path(Path::new("./etc/os-release")).unwrap(),
            PathBuf::from("etc/os-release")
        );
        assert!(safe_relative_path(Path::new("/etc/passwd")).is_none());
        assert!(safe_relative_path(Path::new("../escape")).is_none());
    }

    #[test]
    fn normalizes_safe_relative_link_targets() {
        assert_eq!(
            normalize_link_target(Path::new("usr/bin"), Path::new("../lib/tool")).unwrap(),
            PathBuf::from("usr/lib/tool")
        );
        assert_eq!(
            normalize_link_target(Path::new("etc/alternatives"), Path::new("/usr/bin/mawk"))
                .unwrap(),
            PathBuf::from("usr/bin/mawk")
        );
        assert!(
            normalize_link_target(Path::new("usr/bin"), Path::new("../../../escape")).is_none()
        );
        assert!(normalize_link_target(Path::new("usr/bin"), Path::new("/../escape")).is_none());
    }

    #[test]
    fn renders_container_config() {
        let config = test_config();
        let metadata = ContainerMetadata {
            name: "demo".to_string(),
            distro: "ubuntu".to_string(),
            release: "26.04".to_string(),
            arch: "arm64".to_string(),
            rootfs: "/data/adb/achost/lxc/containers/demo/rootfs".to_string(),
            rootfs_sha256: None,
            init_cmd: Some("/bin/sleep 3600".to_string()),
            created_at_unix: 1,
        };
        let text = container_config_text(&config, &metadata);

        assert!(text.contains("lxc.include = /module/achost/etc/lxc/default.conf"));
        assert!(text.contains("lxc.rootfs.path = dir:/data/adb/achost/lxc/containers/demo/rootfs"));
        assert!(text.contains("lxc.uts.name = demo"));
        assert!(text.contains("lxc.net.0.ipv4.address = "));
        assert!(text.contains("/16"));
        assert!(text.contains("lxc.net.0.ipv4.gateway = 172.32.0.1"));
        assert!(text.contains("lxc.init.cmd = /bin/sleep 3600"));
    }

    #[test]
    fn repairs_raw_container_network_files() {
        let dir = temp_test_dir("container-network");
        let config = temp_test_config(&dir);
        let container_dir = config.container_dir("raw");
        let rootfs = config.container_rootfs("raw");
        fs::create_dir_all(rootfs.join("etc/netplan")).unwrap();
        fs::create_dir_all(rootfs.join("etc/systemd")).unwrap();
        fs::create_dir_all(rootfs.join("run/systemd/resolve")).unwrap();
        fs::write(
            container_dir.join("config"),
            "lxc.net.0.type = veth\nlxc.net.0.link = lxcbr0\n",
        )
        .unwrap();
        fs::write(rootfs.join("etc/netplan/10-lxc.yaml"), "dhcp4: true\n").unwrap();
        symlink(
            "../run/systemd/resolve/stub-resolv.conf",
            rootfs.join("etc/resolv.conf"),
        )
        .unwrap();

        ensure_container_network(&config, "raw").unwrap();

        let lxc_config = fs::read_to_string(container_dir.join("config")).unwrap();
        assert!(lxc_config.contains("lxc.net.0.ipv4.address = "));
        assert!(lxc_config.contains("lxc.net.0.ipv4.gateway = 172.32.0.1"));
        let resolv = fs::read_to_string(rootfs.join("etc/resolv.conf")).unwrap();
        assert!(resolv.contains("nameserver 1.1.1.1"));
        assert!(!path_is_symlink(&rootfs.join("etc/resolv.conf")));
        let netplan = fs::read_to_string(rootfs.join("etc/netplan/10-lxc.yaml")).unwrap();
        assert!(netplan.contains("dhcp4: false"));
        assert!(netplan.contains("nameservers:"));
        let networkd =
            fs::read_to_string(rootfs.join("etc/systemd/network/10-achost-lxc.network")).unwrap();
        assert!(networkd.contains("Gateway=172.32.0.1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_and_rewrites_lxc_autostart() {
        let text = "lxc.uts.name = demo\nlxc.start.auto = 0\nlxc.net.0.type = veth\nlxc.start.auto = 1 # latest wins\n";

        assert!(lxc_start_auto_value(text));
        assert_eq!(
            lxc_start_auto_line(" lxc.start.auto = false # comment"),
            Some(false)
        );
        assert_eq!(lxc_start_auto_line("lxc.uts.name = demo"), None);

        let updated = rewrite_lxc_start_auto(text, false);
        assert!(updated.contains("lxc.uts.name = demo"));
        assert!(updated.contains("lxc.net.0.type = veth"));
        assert!(updated.ends_with("lxc.start.auto = 0\n"));
        assert_eq!(updated.matches("lxc.start.auto").count(), 1);
    }

    #[test]
    fn validates_linux_usernames_for_guest_management() {
        assert!(validate_linux_user("root").is_ok());
        assert!(validate_linux_user("_service.user-1").is_ok());
        assert!(validate_linux_user("").is_err());
        assert!(validate_linux_user("1root").is_err());
        assert!(validate_linux_user("bad:name").is_err());
        assert!(validate_linux_user("bad\nname").is_err());
        assert!(validate_linux_user(&"a".repeat(65)).is_err());
    }

    #[test]
    fn validates_guest_password_input() {
        assert!(validate_password("GoodPass_-.!@#%+=").is_ok());
        assert!(validate_password("").is_err());
        assert!(validate_password("bad:pass").is_err());
        assert!(validate_password("bad\npass").is_err());
        assert!(validate_password("bad\0pass").is_err());
        assert!(validate_password("bad\x7fpass").is_err());
    }

    #[test]
    fn renders_sha512_crypt_hashes() {
        assert_eq!(
            sha512_crypt("password", "saltsalt", SHA512_CRYPT_ROUNDS),
            "$6$saltsalt$qFmFH.bQmmtXzyBY0s9v7Oicd2z4XSIecDzlB5KiA2/jctKu9YterLp8wwnSq.qc.eoxqOmSuNp2xS0ktL3nh/"
        );
    }

    #[test]
    fn rewrites_shadow_password_entry() {
        let dir = env::temp_dir().join(format!("achost-shadow-test-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let shadow = dir.join("shadow");
        fs::write(
            &shadow,
            "root:!:19000:0:99999:7:::\ndaemon:*:19000:0:99999:7:::\n",
        )
        .unwrap();
        update_shadow_password(&shadow, "root", "$6$salt$hash").unwrap();
        let text = fs::read_to_string(&shadow).unwrap();
        assert!(text.starts_with("root:$6$salt$hash:"));
        assert!(text.contains("\ndaemon:*:19000:0:99999:7:::\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_non_elf_payload() {
        let info = elf_info_from_bytes(b"#!/system/bin/sh\n").unwrap();

        assert_eq!(info, ElfInfo::none());
    }

    #[test]
    fn reports_aarch64_elf_header() {
        let mut bytes = vec![0_u8; 64];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());

        let info = elf_info_from_bytes(&bytes).unwrap();

        assert!(info.is_elf);
        assert_eq!(info.arch.as_deref(), Some("aarch64"));
    }
}
