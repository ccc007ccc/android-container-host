use std::env;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
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
        Some("stop") => run_stop(),
        Some(command) => {
            eprintln!("unsupported command: {command}");
            2
        }
        None => {
            eprintln!("usage: achost-docker-runtime <cleanup-stale-iptables|stop>");
            2
        }
    };
    std::process::exit(code);
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

fn read_mount_destinations() -> Vec<String> {
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(decode_mount_field)
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
