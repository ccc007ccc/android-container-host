use libc::{c_char, c_int, c_void, pid_t};
use std::env;
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::Duration;

const MAX_ARGS: usize = 128;
const MAX_STRING: usize = 65535;
const PR_SET_CHILD_SUBREAPER: c_int = 36;

static STOP_SERVER: AtomicBool = AtomicBool::new(false);
static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);

#[derive(Default)]
struct Options {
    pid_file: Option<String>,
    socket_path: Option<String>,
    name: String,
    launch_chroot: Option<String>,
    launch_pivot_root: Option<String>,
    native_root: Option<String>,
    launch_log_file: Option<String>,
    command_index: Option<usize>,
    server_mode: bool,
    client_mode: bool,
    launch_mode: bool,
}

extern "C" fn handle_signal(sig: c_int) {
    if sig == libc::SIGTERM || sig == libc::SIGINT || sig == libc::SIGHUP || sig == libc::SIGQUIT {
        STOP_SERVER.store(true, Ordering::SeqCst);
        PENDING_SIGNAL.store(sig, Ordering::SeqCst);
    }
}

fn usage(argv0: &str) {
    eprintln!(
        "usage:\n  {0} --server --socket PATH --pid-file PATH [--pivot-root PATH|--native-root PATH]\n  {0} --client --socket PATH --pid-file PATH [--name NAME] -- COMMAND [ARG...]\n  {0} --launch [--log-file PATH] [--chroot PATH|--pivot-root PATH] -- COMMAND [ARG...]\n  {0} --pid-file PATH [--name NAME] -- COMMAND [ARG...]",
        argv0
    );
}

fn cstring(value: &str) -> io::Result<CString> {
    CString::new(value.as_bytes())
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "string contains nul byte"))
}

fn write_pid_file(path: &str, pid: pid_t) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    writeln!(file, "{}", pid)?;
    Ok(())
}

fn write_string(stream: &mut UnixStream, value: &str) -> io::Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() > MAX_STRING {
        return Err(io::Error::new(ErrorKind::InvalidInput, "string too long"));
    }
    stream.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    stream.write_all(bytes)
}

fn read_string(stream: &mut UnixStream) -> io::Result<String> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_ne_bytes(len_bytes) as usize;
    if len > MAX_STRING {
        return Err(io::Error::new(ErrorKind::InvalidData, "string too long"));
    }
    let mut bytes = vec![0u8; len];
    stream.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| io::Error::new(ErrorKind::InvalidData, "invalid utf-8"))
}

fn write_u32(stream: &mut UnixStream, value: u32) -> io::Result<()> {
    stream.write_all(&value.to_ne_bytes())
}

fn read_u32(stream: &mut UnixStream) -> io::Result<u32> {
    let mut bytes = [0u8; 4];
    stream.read_exact(&mut bytes)?;
    Ok(u32::from_ne_bytes(bytes))
}

fn install_handler(sig: c_int) -> io::Result<()> {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = handle_signal as *const () as usize;
        action.sa_flags = if sig == libc::SIGCHLD {
            libc::SA_NOCLDSTOP
        } else {
            0
        };
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(sig, &action, std::ptr::null_mut()) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn install_handlers() -> io::Result<()> {
    install_handler(libc::SIGTERM)?;
    install_handler(libc::SIGINT)?;
    install_handler(libc::SIGHUP)?;
    install_handler(libc::SIGQUIT)?;
    install_handler(libc::SIGCHLD)?;
    Ok(())
}

fn set_signal(sig: c_int, handler: libc::sighandler_t) {
    unsafe {
        libc::signal(sig, handler);
    }
}

fn reset_child_signals() {
    set_signal(libc::SIGTERM, libc::SIG_DFL);
    set_signal(libc::SIGINT, libc::SIG_DFL);
    set_signal(libc::SIGHUP, libc::SIG_DFL);
    set_signal(libc::SIGQUIT, libc::SIG_DFL);
    set_signal(libc::SIGCHLD, libc::SIG_DFL);
    set_signal(libc::SIGPIPE, libc::SIG_DFL);
}

fn exit_code_from_status(status: c_int, have_status: bool) -> c_int {
    if !have_status {
        return 0;
    }
    if libc::WIFEXITED(status) {
        return libc::WEXITSTATUS(status);
    }
    if libc::WIFSIGNALED(status) {
        return 128 + libc::WTERMSIG(status);
    }
    1
}

fn forward_signal(child: pid_t, sig: c_int) {
    if child <= 0 {
        return;
    }
    unsafe {
        libc::kill(-child, sig);
        libc::kill(child, sig);
    }
}

fn reap_children(name: &str) {
    loop {
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if reaped > 0 {
            eprintln!(
                "{}: reaped pid={} status={}",
                name,
                reaped,
                exit_code_from_status(status, true)
            );
            continue;
        }
        if reaped < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ECHILD) && err.raw_os_error() != Some(libc::EINTR) {
                eprintln!("waitpid: {}", err);
            }
        }
        return;
    }
}

fn send_client_request(socket_path: &str, pid_file: &str, name: &str, argv: &[String]) -> c_int {
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("connect supervisor socket: {}", err);
            return 1;
        }
    };
    if write_string(&mut stream, name).is_err()
        || write_string(&mut stream, pid_file).is_err()
        || write_u32(&mut stream, argv.len() as u32).is_err()
    {
        eprintln!("write supervisor request: {}", io::Error::last_os_error());
        return 1;
    }
    for arg in argv {
        if let Err(err) = write_string(&mut stream, arg) {
            eprintln!("write supervisor argv: {}", err);
            return 1;
        }
    }
    let mut response = String::new();
    if let Err(err) = stream.read_to_string(&mut response) {
        eprintln!("read supervisor response: {}", err);
        return 1;
    }
    eprint!("{}", response);
    if response.starts_with("OK ") {
        0
    } else {
        1
    }
}

fn exec_command(argv: &[String]) -> ! {
    let mut cstrings = Vec::with_capacity(argv.len());
    for arg in argv {
        match cstring(arg) {
            Ok(value) => cstrings.push(value),
            Err(err) => {
                eprintln!("{}: {}", arg, err);
                unsafe { libc::_exit(126) };
            }
        }
    }
    let mut ptrs: Vec<*const c_char> = cstrings.iter().map(|arg| arg.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe {
        libc::execvp(ptrs[0], ptrs.as_ptr());
        let err = io::Error::last_os_error();
        eprintln!("{}: {}", argv[0], err);
        libc::_exit(if err.raw_os_error() == Some(libc::ENOENT) {
            127
        } else {
            126
        });
    }
}

fn fork_exec(argv: &[String]) -> io::Result<pid_t> {
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(io::Error::last_os_error());
    }
    if child == 0 {
        reset_child_signals();
        unsafe {
            libc::setpgid(0, 0);
        }
        exec_command(argv);
    }
    unsafe {
        libc::setpgid(child, child);
    }
    Ok(child)
}

fn handle_client(mut stream: UnixStream) {
    let name = match read_string(&mut stream) {
        Ok(value) => value,
        Err(_) => {
            let _ = stream.write_all(b"ERR malformed request\n");
            return;
        }
    };
    let pid_file = match read_string(&mut stream) {
        Ok(value) => value,
        Err(_) => {
            let _ = stream.write_all(b"ERR malformed request\n");
            return;
        }
    };
    let argc = match read_u32(&mut stream) {
        Ok(value) => value as usize,
        Err(_) => {
            let _ = stream.write_all(b"ERR malformed request\n");
            return;
        }
    };
    if argc == 0 || argc > MAX_ARGS {
        let _ = stream.write_all(b"ERR malformed request\n");
        return;
    }
    let mut child_argv = Vec::with_capacity(argc);
    for _ in 0..argc {
        match read_string(&mut stream) {
            Ok(value) => child_argv.push(value),
            Err(_) => {
                let _ = stream.write_all(b"ERR malformed argv\n");
                return;
            }
        }
    }
    let child = match fork_exec(&child_argv) {
        Ok(pid) => pid,
        Err(err) => {
            let _ = writeln!(stream, "ERR fork failed: {}", err);
            return;
        }
    };
    if let Err(err) = write_pid_file(&pid_file, child) {
        unsafe {
            libc::kill(child, libc::SIGTERM);
        }
        let _ = writeln!(stream, "ERR write pid file failed: {}", err);
    } else {
        let _ = writeln!(stream, "OK {} pid={}", name, child);
        eprintln!("achost-supervise: started {} pid={}", name, child);
    }
}

fn ensure_directory(path: &str, mode: u32) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn mount_call(
    source: Option<&str>,
    target: &str,
    fstype: Option<&str>,
    flags: libc::c_ulong,
    data: Option<&str>,
) -> io::Result<()> {
    let source_c = match source {
        Some(value) => Some(cstring(value)?),
        None => None,
    };
    let target_c = cstring(target)?;
    let fstype_c = match fstype {
        Some(value) => Some(cstring(value)?),
        None => None,
    };
    let data_c = match data {
        Some(value) => Some(cstring(value)?),
        None => None,
    };
    let rc = unsafe {
        libc::mount(
            source_c
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            target_c.as_ptr(),
            fstype_c
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            flags,
            data_c
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr() as *const c_void),
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn make_private_mount_namespace() -> io::Result<()> {
    let unshare_rc = unsafe { libc::syscall(libc::SYS_unshare, libc::CLONE_NEWNS) };
    if unshare_rc < 0 {
        return Err(io::Error::last_os_error());
    }
    mount_call(
        None,
        "/",
        None,
        (libc::MS_REC | libc::MS_PRIVATE) as libc::c_ulong,
        None,
    )
}

fn pivot_into_root(new_root: &str) -> io::Result<()> {
    env::set_current_dir(new_root)?;
    let _ = fs::remove_dir(".achost-old-root");
    ensure_directory(".achost-old-root", 0o700)?;
    let root = cstring(".")?;
    let old_root = cstring(".achost-old-root")?;
    let rc = unsafe { libc::syscall(libc::SYS_pivot_root, root.as_ptr(), old_root.as_ptr()) };
    if rc < 0 {
        let _ = fs::remove_dir(".achost-old-root");
        return Err(io::Error::last_os_error());
    }
    env::set_current_dir("/")?;
    let old = cstring("/.achost-old-root")?;
    if unsafe { libc::umount2(old.as_ptr(), libc::MNT_DETACH) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let _ = fs::remove_dir("/.achost-old-root");
    Ok(())
}

fn pivot_to_root(new_root: &str) -> io::Result<()> {
    make_private_mount_namespace()?;
    pivot_into_root(new_root)
}

fn join_root(root: &str, path: &str) -> String {
    if let Some(stripped) = path.strip_prefix('/') {
        format!("{}/{}", root, stripped)
    } else {
        format!("{}/{}", root, path)
    }
}

fn bind_native_path(native_root: &str, source: &str, required: bool) -> io::Result<()> {
    let metadata = match fs::metadata(source) {
        Ok(metadata) => metadata,
        Err(err) => {
            if required {
                eprintln!("required native path missing: {}", source);
                return Err(err);
            }
            return Ok(());
        }
    };
    if !metadata.is_dir() {
        return Ok(());
    }
    let destination = join_root(native_root, source);
    if let Err(err) = ensure_directory(&destination, 0o755) {
        if required {
            return Err(err);
        }
        return Ok(());
    }
    if let Err(err) = mount_call(
        Some(source),
        &destination,
        None,
        (libc::MS_BIND | libc::MS_REC) as libc::c_ulong,
        None,
    ) {
        eprintln!("bind {} to {}: {}", source, destination, err);
        if required {
            return Err(err);
        }
    }
    Ok(())
}

fn mount_native_cgroup_controller(cgroup_root: &str, controller: &str) -> io::Result<()> {
    let destination = join_root(cgroup_root, controller);
    ensure_directory(&destination, 0o755)?;
    if let Err(err) = mount_call(
        Some("none"),
        &destination,
        Some("cgroup"),
        0,
        Some(controller),
    ) {
        eprintln!("warning: unable to mount {} cgroup: {}", controller, err);
    }
    Ok(())
}

fn setup_native_cgroups(native_root: &str) -> io::Result<()> {
    let cgroup_root = join_root(native_root, "/sys/fs/cgroup");
    ensure_directory(&cgroup_root, 0o755)?;
    mount_call(
        Some("tmpfs"),
        &cgroup_root,
        Some("tmpfs"),
        0,
        Some("mode=755,size=1m"),
    )?;
    for controller in [
        "devices", "pids", "cpu", "cpuacct", "cpuset", "blkio", "freezer", "memory",
    ] {
        mount_native_cgroup_controller(&cgroup_root, controller)?;
    }
    Ok(())
}

fn setup_native_run(native_root: &str) -> io::Result<()> {
    let run_path = join_root(native_root, "/run");
    let var_path = join_root(native_root, "/var");
    let var_run_path = join_root(native_root, "/var/run");
    let tmp_path = join_root(native_root, "/tmp");
    ensure_directory(&run_path, 0o755)?;
    ensure_directory(&var_path, 0o755)?;
    ensure_directory(&tmp_path, 0o1777)?;
    mount_call(
        Some("tmpfs"),
        &run_path,
        Some("tmpfs"),
        0,
        Some("mode=755,size=64m"),
    )?;
    match fs::symlink_metadata(&var_run_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let _ = fs::remove_file(&var_run_path);
            symlink("/run", &var_run_path)?;
        }
        Ok(metadata) if metadata.is_dir() => {
            mount_call(
                Some(&run_path),
                &var_run_path,
                None,
                (libc::MS_BIND | libc::MS_REC) as libc::c_ulong,
                None,
            )?;
        }
        Ok(_) => {
            let _ = fs::remove_file(&var_run_path);
            symlink("/run", &var_run_path)?;
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            symlink("/run", &var_run_path)?;
        }
        Err(err) => return Err(err),
    }
    if let Err(err) = mount_call(
        Some("tmpfs"),
        &tmp_path,
        Some("tmpfs"),
        0,
        Some("mode=1777,size=64m"),
    ) {
        eprintln!("warning: unable to mount private /tmp: {}", err);
    }
    Ok(())
}

fn setup_native_root(native_root: &str) -> io::Result<()> {
    ensure_directory(native_root, 0o755)?;
    make_private_mount_namespace()?;
    mount_call(
        Some(native_root),
        native_root,
        None,
        libc::MS_BIND as libc::c_ulong,
        None,
    )?;
    for (path, required) in [
        ("/data", true),
        ("/proc", true),
        ("/sys", true),
        ("/system", true),
        ("/apex", true),
        ("/vendor", false),
        ("/product", false),
        ("/odm", false),
        ("/mnt", false),
        ("/storage", false),
        ("/metadata", false),
        ("/linkerconfig", false),
        ("/config", false),
        ("/debug_ramdisk", false),
        ("/second_stage_resources", false),
        ("/sdcard", false),
    ] {
        bind_native_path(native_root, path, required)?;
    }
    setup_native_cgroups(native_root)?;
    // Keep Android cgroup mounts after the private tree so containerd selects /sys/fs/cgroup.
    for (path, required) in [("/dev", true), ("/acct", false)] {
        bind_native_path(native_root, path, required)?;
    }
    setup_native_run(native_root)?;
    eprintln!(
        "achost-supervise: native root={} private-run=ready private-cgroup=ready",
        native_root
    );
    pivot_into_root(native_root)
}

fn redirect_log(log_file: &str) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;
    let fd = file.as_raw_fd();
    if unsafe { libc::dup2(fd, libc::STDOUT_FILENO) } < 0
        || unsafe { libc::dup2(fd, libc::STDERR_FILENO) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn run_launch(
    chroot_path: Option<&str>,
    pivot_root_path: Option<&str>,
    log_file: Option<&str>,
    command: &[String],
) -> c_int {
    if let Some(log_file) = log_file {
        if let Err(err) = redirect_log(log_file) {
            eprintln!("open log file: {}", err);
            return 1;
        }
    }
    if let Some(pivot_root_path) = pivot_root_path {
        if let Err(err) = pivot_to_root(pivot_root_path) {
            eprintln!("{}", last_error_with("pivot_root", err));
            return 1;
        }
    } else if let Some(chroot_path) = chroot_path {
        let path = match cstring(chroot_path) {
            Ok(path) => path,
            Err(err) => {
                eprintln!("chroot: {}", err);
                return 1;
            }
        };
        if unsafe { libc::chroot(path.as_ptr()) } < 0 {
            eprintln!("chroot: {}", io::Error::last_os_error());
            return 1;
        }
        if let Err(err) = env::set_current_dir("/") {
            eprintln!("chdir: {}", err);
            return 1;
        }
    }
    exec_command(command);
}

fn last_error_with(prefix: &str, err: io::Error) -> String {
    format!("{}: {}", prefix, err)
}

fn create_server_socket(socket_path: &str) -> io::Result<UnixListener> {
    let _ = fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn set_subreaper() -> io::Result<()> {
    if unsafe { libc::prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn run_server(
    socket_path: &str,
    pid_file: &str,
    pivot_root_path: Option<&str>,
    native_root_path: Option<&str>,
) -> c_int {
    if let Err(err) = install_handlers() {
        eprintln!("sigaction: {}", err);
        return 1;
    }
    set_signal(libc::SIGPIPE, libc::SIG_IGN);
    if let Err(err) = set_subreaper() {
        eprintln!("prctl(PR_SET_CHILD_SUBREAPER): {}", err);
        return 1;
    }
    if let Some(path) = pivot_root_path {
        if let Err(err) = pivot_to_root(path) {
            eprintln!("pivot_root: {}", err);
            return 1;
        }
    }
    if let Some(path) = native_root_path {
        if let Err(err) = setup_native_root(path) {
            eprintln!("native root: {}", err);
            return 1;
        }
    }
    let listener = match create_server_socket(socket_path) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("create supervisor socket: {}", err);
            return 1;
        }
    };
    if let Err(err) = write_pid_file(pid_file, unsafe { libc::getpid() }) {
        eprintln!("write supervisor pid file: {}", err);
        let _ = fs::remove_file(socket_path);
        return 1;
    }
    eprintln!(
        "achost-supervise: server pid={} socket={}",
        unsafe { libc::getpid() },
        socket_path
    );
    while !STOP_SERVER.load(Ordering::SeqCst) {
        reap_children("achost-supervise");
        match listener.accept() {
            Ok((stream, _)) => handle_client(stream),
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(250))
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                eprintln!("accept: {}", err);
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
    drop(listener);
    let _ = fs::remove_file(socket_path);
    let _ = fs::remove_file(pid_file);
    reap_children("achost-supervise");
    0
}

fn run_legacy(pid_file: &str, name: &str, command: &[String]) -> c_int {
    if let Err(err) = install_handlers() {
        eprintln!("sigaction: {}", err);
        return 1;
    }
    if let Err(err) = set_subreaper() {
        eprintln!("prctl(PR_SET_CHILD_SUBREAPER): {}", err);
        return 1;
    }
    if let Err(err) = write_pid_file(pid_file, unsafe { libc::getpid() }) {
        eprintln!("write pid file: {}", err);
        return 1;
    }
    let child = match fork_exec(command) {
        Ok(pid) => pid,
        Err(err) => {
            eprintln!("fork: {}", err);
            let _ = fs::remove_file(pid_file);
            return 1;
        }
    };
    eprintln!("{}: supervising pid={}", name, child);
    let mut main_status = 0;
    let mut main_exited = false;
    loop {
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if reaped > 0 {
            if reaped == child {
                main_status = status;
                main_exited = true;
                eprintln!(
                    "{}: main pid={} exited status={}",
                    name,
                    child,
                    exit_code_from_status(status, true)
                );
            }
            continue;
        }
        if reaped < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            if err.raw_os_error() == Some(libc::ECHILD) {
                let _ = fs::remove_file(pid_file);
                return exit_code_from_status(main_status, main_exited);
            }
            eprintln!("waitpid: {}", err);
        }
        let sig = PENDING_SIGNAL.swap(0, Ordering::SeqCst);
        if sig != 0 {
            forward_signal(child, sig);
            eprintln!("{}: forwarded signal={}", name, sig);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn parse_args(args: &[String]) -> Result<Options, ()> {
    let mut opts = Options {
        name: "achost-supervise".to_string(),
        ..Options::default()
    };
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--server" => opts.server_mode = true,
            "--client" => opts.client_mode = true,
            "--launch" => opts.launch_mode = true,
            "--socket" if i + 1 < args.len() => {
                i += 1;
                opts.socket_path = Some(args[i].clone());
            }
            "--pid-file" if i + 1 < args.len() => {
                i += 1;
                opts.pid_file = Some(args[i].clone());
            }
            "--name" if i + 1 < args.len() => {
                i += 1;
                opts.name = args[i].clone();
            }
            "--chroot" if i + 1 < args.len() => {
                i += 1;
                opts.launch_chroot = Some(args[i].clone());
            }
            "--pivot-root" if i + 1 < args.len() => {
                i += 1;
                opts.launch_pivot_root = Some(args[i].clone());
            }
            "--native-root" if i + 1 < args.len() => {
                i += 1;
                opts.native_root = Some(args[i].clone());
            }
            "--log-file" if i + 1 < args.len() => {
                i += 1;
                opts.launch_log_file = Some(args[i].clone());
            }
            "--" => {
                opts.command_index = Some(i + 1);
                break;
            }
            _ => return Err(()),
        }
        i += 1;
    }
    Ok(opts)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let argv0 = args.first().map_or("achost-supervise", String::as_str);
    let opts = match parse_args(&args) {
        Ok(opts) => opts,
        Err(()) => {
            usage(argv0);
            process::exit(2);
        }
    };
    let command_index = opts.command_index.unwrap_or(usize::MAX);
    if opts.launch_mode {
        if opts.server_mode
            || opts.client_mode
            || opts.pid_file.is_some()
            || opts.socket_path.is_some()
            || opts.native_root.is_some()
            || command_index == 0
            || command_index >= args.len()
            || (opts.launch_chroot.is_some() && opts.launch_pivot_root.is_some())
        {
            usage(argv0);
            process::exit(2);
        }
        let code = run_launch(
            opts.launch_chroot.as_deref(),
            opts.launch_pivot_root.as_deref(),
            opts.launch_log_file.as_deref(),
            &args[command_index..],
        );
        process::exit(code);
    }
    if opts.server_mode {
        if opts.client_mode
            || opts.socket_path.is_none()
            || opts.pid_file.is_none()
            || opts.launch_chroot.is_some()
            || opts.launch_log_file.is_some()
            || opts.command_index.is_some()
            || (opts.launch_pivot_root.is_some() && opts.native_root.is_some())
        {
            usage(argv0);
            process::exit(2);
        }
        let code = run_server(
            opts.socket_path.as_deref().unwrap(),
            opts.pid_file.as_deref().unwrap(),
            opts.launch_pivot_root.as_deref(),
            opts.native_root.as_deref(),
        );
        process::exit(code);
    }
    if opts.client_mode {
        if opts.socket_path.is_none()
            || opts.pid_file.is_none()
            || opts.launch_chroot.is_some()
            || opts.launch_pivot_root.is_some()
            || opts.native_root.is_some()
            || opts.launch_log_file.is_some()
            || command_index == 0
            || command_index >= args.len()
        {
            usage(argv0);
            process::exit(2);
        }
        let code = send_client_request(
            opts.socket_path.as_deref().unwrap(),
            opts.pid_file.as_deref().unwrap(),
            &opts.name,
            &args[command_index..],
        );
        process::exit(code);
    }
    if opts.launch_chroot.is_some()
        || opts.launch_pivot_root.is_some()
        || opts.native_root.is_some()
        || opts.launch_log_file.is_some()
        || opts.pid_file.is_none()
        || command_index == 0
        || command_index >= args.len()
    {
        usage(argv0);
        process::exit(2);
    }
    let code = run_legacy(
        opts.pid_file.as_deref().unwrap(),
        &opts.name,
        &args[command_index..],
    );
    process::exit(code);
}
