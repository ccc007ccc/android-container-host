use std::env;
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

static TERMINATE: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct RuntimeError {
    code: i32,
    message: String,
}

impl RuntimeError {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug)]
struct RuntimeConfig {
    bridge: String,
    docker_subnet: Option<String>,
    uplink: Option<String>,
    target: String,
    iptables: Option<String>,
    dry_run: bool,
    policy_rules: String,
    return_rule_priority: u32,
    source_rule_priority: u32,
}

impl RuntimeConfig {
    fn from_env() -> Self {
        let bridge = env_nonempty("CONTAINER_BRIDGE")
            .or_else(|| env_nonempty("DOCKER_BRIDGE"))
            .unwrap_or_else(|| "docker0".to_string());
        Self {
            bridge,
            docker_subnet: env_nonempty("CONTAINER_SUBNET")
                .or_else(|| env_nonempty("DOCKER_SUBNET")),
            uplink: env_nonempty("UPLINK"),
            target: env_nonempty("TARGET").unwrap_or_else(|| "1.1.1.1".to_string()),
            iptables: env_nonempty("IPTABLES"),
            dry_run: env::var("ACHOST_DRY_RUN").is_ok_and(|value| value == "1"),
            policy_rules: env::var("ACHOST_CONTAINER_POLICY_RULES")
                .unwrap_or_else(|_| "1".to_string()),
            return_rule_priority: env_priority("ACHOST_CONTAINER_RETURN_RULE_PRIORITY", 11999),
            source_rule_priority: env_priority("ACHOST_CONTAINER_SOURCE_RULE_PRIORITY", 12000),
        }
    }
}

struct WatchdogConfig {
    runtime: RuntimeConfig,
    watch_interval: u64,
    repair_interval: u64,
    log_file: PathBuf,
    pid_file: PathBuf,
}

impl WatchdogConfig {
    fn from_env() -> Self {
        Self {
            runtime: RuntimeConfig::from_env(),
            watch_interval: env_u64("ACHOST_NET_WATCH_INTERVAL", 5),
            repair_interval: env_u64("ACHOST_NET_REPAIR_INTERVAL", 30),
            log_file: env_path("ACHOST_NET_LOG")
                .unwrap_or_else(|| PathBuf::from("/data/local/tmp/achost-network-watchdog.log")),
            pid_file: env_path("ACHOST_NET_PID")
                .unwrap_or_else(|| PathBuf::from("/data/local/tmp/achost-network-watchdog.pid")),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct BridgeReconcileArgs {
    bridge: Option<String>,
    subnet: Option<String>,
    owner: String,
}

#[derive(Clone, Copy)]
enum Logger<'a> {
    Stdout,
    File(&'a Path),
}

impl Logger<'_> {
    fn line(&self, message: impl AsRef<str>) {
        match self {
            Logger::Stdout => println!("{}", message.as_ref()),
            Logger::File(path) => append_line_or_stdout(path, message.as_ref()),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("detect-uplink") => run_detect_uplink(&args[2..]),
        Some("net-reconcile") => run_net_reconcile(),
        Some("bridge-reconcile") => run_bridge_reconcile(&args[2..]),
        Some("net-watchdog") => run_net_watchdog(),
        Some("protect-daemons") => run_protect_daemons(),
        Some(command) => {
            eprintln!("unsupported command: {command}");
            2
        }
        None => {
            eprintln!("usage: achost-runtime-core <detect-uplink|net-reconcile|bridge-reconcile|net-watchdog|protect-daemons>");
            2
        }
    };
    std::process::exit(code);
}

fn parse_bridge_reconcile_args(args: &[String]) -> Result<BridgeReconcileArgs, String> {
    let mut parsed = BridgeReconcileArgs {
        bridge: None,
        subnet: None,
        owner: "container".to_string(),
    };
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--bridge" => parsed.bridge = Some(value.clone()),
            "--subnet" => parsed.subnet = Some(value.clone()),
            "--owner" => parsed.owner = value.clone(),
            _ => return Err(format!("unsupported bridge-reconcile argument: {flag}")),
        }
        index += 2;
    }
    Ok(parsed)
}

fn run_detect_uplink(args: &[String]) -> i32 {
    let target = args.first().map_or("1.1.1.1", String::as_str);
    match detect_uplink_iface(target) {
        Ok(iface) => {
            println!("{iface}");
            0
        }
        Err(error) => {
            eprintln!("{}", error.message);
            error.code
        }
    }
}

fn run_net_reconcile() -> i32 {
    match net_reconcile(&RuntimeConfig::from_env(), Logger::Stdout) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {}", error.message);
            error.code
        }
    }
}

fn run_bridge_reconcile(args: &[String]) -> i32 {
    let bridge_args = match parse_bridge_reconcile_args(args) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("error: {message}");
            return 2;
        }
    };
    let mut config = RuntimeConfig::from_env();
    if let Some(bridge) = bridge_args.bridge {
        config.bridge = bridge;
    }
    if let Some(subnet) = bridge_args.subnet {
        config.docker_subnet = Some(subnet);
    }
    println!("bridge_owner={}", bridge_args.owner);
    if let Some(subnet) = config.docker_subnet.as_deref() {
        if let Err(error) =
            ensure_bridge_ready(&config.bridge, subnet, config.dry_run, Logger::Stdout)
        {
            eprintln!("error: {}", error.message);
            return error.code;
        }
    }
    match net_reconcile(&config, Logger::Stdout) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {}", error.message);
            error.code
        }
    }
}

fn run_net_watchdog() -> i32 {
    match net_watchdog(WatchdogConfig::from_env()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: {}", error.message);
            error.code
        }
    }
}

fn run_protect_daemons() -> i32 {
    protect_daemons();
    0
}

fn ensure_bridge_ready(
    bridge: &str,
    subnet: &str,
    dry_run: bool,
    logger: Logger<'_>,
) -> Result<(), RuntimeError> {
    if !have_command("ip") {
        return Err(RuntimeError::new(1, "ip command not found"));
    }
    if !command_success_silent("ip", &["link", "show", bridge]) {
        let args = vec![
            "link".to_string(),
            "add".to_string(),
            "name".to_string(),
            bridge.to_string(),
            "type".to_string(),
            "bridge".to_string(),
        ];
        if !run_logged(logger, "ip", &args, dry_run)
            && !command_success_silent("ip", &["link", "show", bridge])
        {
            return Err(RuntimeError::new(
                1,
                format!("failed to create bridge {bridge}"),
            ));
        }
    }
    let gateway = bridge_gateway_cidr(subnet)
        .ok_or_else(|| RuntimeError::new(1, format!("invalid bridge subnet: {subnet}")))?;
    let addr_args = vec![
        "addr".to_string(),
        "replace".to_string(),
        gateway,
        "dev".to_string(),
        bridge.to_string(),
    ];
    if !run_logged(logger, "ip", &addr_args, dry_run) {
        return Err(RuntimeError::new(
            1,
            format!("failed to assign address to {bridge}"),
        ));
    }
    let up_args = vec![
        "link".to_string(),
        "set".to_string(),
        bridge.to_string(),
        "up".to_string(),
    ];
    if !run_logged(logger, "ip", &up_args, dry_run) {
        return Err(RuntimeError::new(
            1,
            format!("failed to bring up bridge {bridge}"),
        ));
    }
    Ok(())
}

fn net_reconcile(config: &RuntimeConfig, logger: Logger<'_>) -> Result<(), RuntimeError> {
    if !have_command("ip") {
        return Err(RuntimeError::new(1, "ip command not found"));
    }
    if !command_success_silent("ip", &["addr", "show", &config.bridge]) {
        return Err(RuntimeError::new(1, format!("{} not found", config.bridge)));
    }
    let iptables =
        pick_iptables(config).ok_or_else(|| RuntimeError::new(1, "iptables command not found"))?;
    let subnet = resolve_docker_subnet(config).ok_or_else(|| {
        RuntimeError::new(
            1,
            format!(
                "cannot determine IPv4 subnet for {}; set CONTAINER_SUBNET or DOCKER_SUBNET",
                config.bridge
            ),
        )
    })?;

    logger.line(format!("container_bridge={}", config.bridge));
    logger.line(format!("container_subnet={subnet}"));
    logger.line(format!("docker_bridge={}", config.bridge));
    logger.line(format!("docker_subnet={subnet}"));
    logger.line(format!("iptables={iptables}"));
    logger.line(format!(
        "policy_rules={} return_rule_priority={} source_rule_priority={}",
        config.policy_rules, config.return_rule_priority, config.source_rule_priority
    ));

    set_sysctl_value("net.ipv4.ip_forward", "1", config.dry_run, logger);
    set_sysctl_value("net.ipv6.conf.all.forwarding", "1", config.dry_run, logger);
    ensure_return_policy_rule(config, &subnet, logger);

    let uplink = resolve_uplink(config);
    let Some(uplink) = uplink else {
        logger.line("warn: cannot detect uplink interface; host access rule repaired only");
        logger.line("container host-route reconciliation complete");
        return Ok(());
    };

    logger.line(format!("uplink={uplink}"));
    ensure_source_policy_rule(config, &subnet, &uplink, logger);
    cleanup_bridge_forward_rules(&iptables, &config.bridge);

    ensure_rule(
        &iptables,
        "filter",
        "FORWARD",
        &[
            "-i".to_string(),
            config.bridge.clone(),
            "-o".to_string(),
            uplink,
            "-j".to_string(),
            "ACCEPT".to_string(),
        ],
        config.dry_run,
        logger,
    );
    ensure_rule(
        &iptables,
        "filter",
        "FORWARD",
        &[
            "-o".to_string(),
            config.bridge.clone(),
            "-m".to_string(),
            "conntrack".to_string(),
            "--ctstate".to_string(),
            "RELATED,ESTABLISHED".to_string(),
            "-j".to_string(),
            "ACCEPT".to_string(),
        ],
        config.dry_run,
        logger,
    );
    ensure_rule(
        &iptables,
        "nat",
        "POSTROUTING",
        &[
            "-s".to_string(),
            subnet,
            "!".to_string(),
            "-o".to_string(),
            config.bridge.clone(),
            "-j".to_string(),
            "MASQUERADE".to_string(),
        ],
        config.dry_run,
        logger,
    );

    logger.line("container NAT reconciliation complete");
    Ok(())
}

fn net_watchdog(config: WatchdogConfig) -> Result<(), RuntimeError> {
    ensure_parent_dir(&config.log_file);
    ensure_parent_dir(&config.pid_file);
    let logger = Logger::File(&config.log_file);

    if let Some(old_pid) = read_pid_file(&config.pid_file) {
        if pid_alive(old_pid) {
            watchdog_log(
                &config.log_file,
                format!("watchdog already running pid={old_pid}"),
            );
            return Ok(());
        }
    }

    fs::write(&config.pid_file, format!("{}\n", std::process::id())).ok();
    install_signal_handlers();
    watchdog_log(
        &config.log_file,
        format!(
            "watchdog starting pid={} bridge={} target={} interval={} repair_interval={}",
            std::process::id(),
            config.runtime.bridge,
            config.runtime.target,
            config.watch_interval,
            config.repair_interval
        ),
    );

    let mut last_state = String::new();
    let mut last_wait = String::new();
    let mut cycles = config.repair_interval;

    while !TERMINATE.load(Ordering::SeqCst) {
        if !have_command("ip") {
            log_wait_once(
                &config.log_file,
                &mut last_wait,
                "missing-ip",
                "waiting: ip command not found",
            );
            sleep_watch(config.watch_interval);
            continue;
        }

        if !command_success_silent("ip", &["addr", "show", &config.runtime.bridge]) {
            log_wait_once(
                &config.log_file,
                &mut last_wait,
                "missing-bridge",
                format!("waiting: {} not found", config.runtime.bridge),
            );
            sleep_watch(config.watch_interval);
            continue;
        }

        let Some(subnet) = bridge_subnet_for_watchdog(&config.runtime) else {
            log_wait_once(
                &config.log_file,
                &mut last_wait,
                "missing-subnet",
                format!(
                    "waiting: cannot determine IPv4 subnet for {}",
                    config.runtime.bridge
                ),
            );
            sleep_watch(config.watch_interval);
            continue;
        };

        let uplink = resolve_uplink(&config.runtime);
        if uplink.is_none() {
            log_wait_once(
                &config.log_file,
                &mut last_wait,
                "missing-uplink",
                format!(
                    "waiting: cannot determine uplink for target {}; repairing host route only",
                    config.runtime.target
                ),
            );
        } else {
            last_wait.clear();
        }

        let uplink_label = uplink.as_deref().unwrap_or("unavailable");
        let return_rule_needle = format!("to {subnet} lookup main");
        if !ip_rule_present(config.runtime.return_rule_priority, &return_rule_needle) {
            watchdog_log(
                &config.log_file,
                format!(
                    "repair: missing ip rule priority={} {}",
                    config.runtime.return_rule_priority, return_rule_needle
                ),
            );
            cycles = config.repair_interval;
        }

        let state = format!(
            "{}|{}|{}|{}",
            config.runtime.bridge,
            subnet,
            uplink_label,
            config.runtime.iptables.as_deref().unwrap_or_default()
        );
        if state != last_state {
            watchdog_log(
                &config.log_file,
                format!(
                    "state: bridge={} subnet={} uplink={} iptables={}",
                    config.runtime.bridge,
                    subnet,
                    uplink_label,
                    config.runtime.iptables.as_deref().unwrap_or("auto")
                ),
            );
            last_state = state;
            cycles = config.repair_interval;
        }

        if cycles >= config.repair_interval {
            let mut repair_config = config.runtime.clone();
            repair_config.docker_subnet = Some(subnet.clone());
            repair_config.uplink = uplink.clone();
            match net_reconcile(&repair_config, logger) {
                Ok(()) => watchdog_log(
                    &config.log_file,
                    format!(
                        "reconcile ok: bridge={} uplink={uplink_label}",
                        config.runtime.bridge
                    ),
                ),
                Err(error) => watchdog_log(
                    &config.log_file,
                    format!(
                        "reconcile failed: bridge={} uplink={} exit={}",
                        config.runtime.bridge, uplink_label, error.code
                    ),
                ),
            }
            cycles = 0;
        }

        sleep_watch(config.watch_interval);
        cycles = cycles.saturating_add(config.watch_interval);
    }

    fs::remove_file(&config.pid_file).ok();
    Ok(())
}

fn protect_daemons() {
    let oom_score_adj = env::var("OOM_SCORE_ADJ").unwrap_or_else(|_| "-900".to_string());
    let protect_shims = env::var("PROTECT_SHIMS").unwrap_or_else(|_| "0".to_string());
    let dry_run = env::var("ACHOST_DRY_RUN").is_ok_and(|value| value == "1");
    let mut daemons = vec!["dockerd", "containerd"];
    if protect_shims == "1" {
        daemons.extend(["containerd-shim", "containerd-shim-runc-v2"]);
    }

    println!("oom_score_adj_target={oom_score_adj}");
    println!("protect_shims={protect_shims}");

    for daemon in daemons {
        let pids = pids_for_name(daemon);
        if pids.is_empty() {
            println!("not running: {daemon}");
            continue;
        }
        for pid in pids {
            write_oom_score_adj(pid, daemon, &oom_score_adj, dry_run);
        }
    }

    if have_command("getprop") {
        println!(
            "ro.lmk.use_psi={}",
            command_stdout_trim("getprop", &["ro.lmk.use_psi"])
        );
        println!(
            "ro.lmk.debug={}",
            command_stdout_trim("getprop", &["ro.lmk.debug"])
        );
    }

    if let Ok(memory_pressure) = fs::read_to_string("/proc/pressure/memory") {
        println!("memory pressure:");
        print!("{memory_pressure}");
    }
}

fn detect_uplink_iface(target: &str) -> Result<String, RuntimeError> {
    if !have_command("ip") {
        return Err(RuntimeError::new(2, "ip command not found"));
    }
    let output = command_output("ip", &["route", "get", target])
        .map_err(|_| RuntimeError::new(2, "ip command not found"))?;
    let route = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || route.is_empty() {
        return Err(RuntimeError::new(
            1,
            format!("failed to resolve uplink for {target}"),
        ));
    }
    let iface = parse_dev_field(&route)
        .ok_or_else(|| RuntimeError::new(1, format!("no dev field in route: {route}")))?;
    if link_is_usable(&iface) {
        Ok(iface)
    } else {
        Err(RuntimeError::new(
            1,
            format!("route dev is not usable: {iface}"),
        ))
    }
}

fn link_is_usable(iface: &str) -> bool {
    let Ok(output) = command_output("ip", &["link", "show", iface]) else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let link = String::from_utf8_lossy(&output.stdout);
    !link.trim().is_empty() && !link.contains("NO-CARRIER") && !link.contains("state DOWN")
}

fn resolve_uplink(config: &RuntimeConfig) -> Option<String> {
    if let Some(uplink) = config.uplink.as_deref().filter(|value| !value.is_empty()) {
        return Some(uplink.to_string());
    }
    detect_uplink_iface(&config.target).ok()
}

fn pick_iptables(config: &RuntimeConfig) -> Option<String> {
    if let Some(iptables) = config.iptables.as_deref().filter(|value| !value.is_empty()) {
        return Some(iptables.to_string());
    }
    ["iptables", "iptables-legacy", "iptables-nft"]
        .into_iter()
        .find(|command| have_command(command))
        .map(str::to_string)
}

fn resolve_docker_subnet(config: &RuntimeConfig) -> Option<String> {
    subnet_from_route(&config.bridge)
        .or_else(|| {
            config
                .docker_subnet
                .clone()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| subnet_from_addr(&config.bridge))
}

fn bridge_subnet_for_watchdog(config: &RuntimeConfig) -> Option<String> {
    config
        .docker_subnet
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| subnet_from_route(&config.bridge))
        .or_else(|| subnet_from_addr(&config.bridge))
}

fn subnet_from_route(bridge: &str) -> Option<String> {
    let output = command_output(
        "ip",
        &["-4", "route", "show", "dev", bridge, "scope", "link"],
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| {
        let first = line.split_whitespace().next()?;
        looks_like_ipv4_cidr(first).then(|| first.to_string())
    })
}

fn subnet_from_addr(bridge: &str) -> Option<String> {
    let output = command_output("ip", &["-4", "addr", "show", bridge]).ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().find_map(|line| token_after(line, "inet"))
}

fn ensure_return_policy_rule(config: &RuntimeConfig, subnet: &str, logger: Logger<'_>) {
    if config.policy_rules != "1" {
        return;
    }
    ensure_ip_rule(
        config.return_rule_priority,
        &format!("to {subnet} lookup main"),
        &[
            "to".to_string(),
            subnet.to_string(),
            "lookup".to_string(),
            "main".to_string(),
        ],
        config.dry_run,
        logger,
    );
}

fn ensure_source_policy_rule(
    config: &RuntimeConfig,
    subnet: &str,
    uplink: &str,
    logger: Logger<'_>,
) {
    if config.policy_rules != "1" {
        return;
    }
    ensure_ip_rule(
        config.source_rule_priority,
        &format!("from {subnet} lookup {uplink}"),
        &[
            "from".to_string(),
            subnet.to_string(),
            "lookup".to_string(),
            uplink.to_string(),
        ],
        config.dry_run,
        logger,
    );
}

fn ensure_ip_rule(priority: u32, needle: &str, args: &[String], dry_run: bool, logger: Logger<'_>) {
    if !dry_run {
        let (count, exact) = count_ip_rules_at_priority(priority, needle);
        if count == 1 && exact == 1 {
            logger.line(format!("ok: ip rule priority {priority} {needle}"));
            return;
        }
    }
    delete_ip_rules_at_priority(priority, dry_run, logger);
    let mut add_args = vec!["rule".to_string(), "add".to_string()];
    add_args.extend(args.iter().cloned());
    add_args.extend(["priority".to_string(), priority.to_string()]);
    run_logged(logger, "ip", &add_args, dry_run);
}

fn count_ip_rules_at_priority(priority: u32, needle: &str) -> (usize, usize) {
    let Ok(output) = command_output("ip", &["rule", "show"]) else {
        return (0, 0);
    };
    if !output.status.success() {
        return (0, 0);
    }
    let priority_token = format!("{priority}:");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut count = 0;
    let mut exact = 0;
    for line in stdout.lines() {
        if line.split_whitespace().next() == Some(priority_token.as_str()) {
            count += 1;
            if line.contains(needle) {
                exact += 1;
            }
        }
    }
    (count, exact)
}

fn ip_rule_present(priority: u32, needle: &str) -> bool {
    count_ip_rules_at_priority(priority, needle).1 > 0
}

fn delete_ip_rules_at_priority(priority: u32, dry_run: bool, logger: Logger<'_>) {
    if dry_run {
        logger.line(format!("+ ip rule del priority {priority} # until empty"));
        return;
    }
    while command_success_null("ip", &["rule", "del", "priority", &priority.to_string()]) {}
}

fn ensure_rule(
    iptables: &str,
    table: &str,
    chain: &str,
    args: &[String],
    dry_run: bool,
    logger: Logger<'_>,
) {
    delete_existing_rules(iptables, table, chain, args, dry_run);
    let mut command_args = Vec::new();
    if table == "filter" {
        command_args.extend(["-I".to_string(), chain.to_string()]);
    } else {
        command_args.extend([
            "-t".to_string(),
            table.to_string(),
            "-A".to_string(),
            chain.to_string(),
        ]);
    }
    command_args.extend(args.iter().cloned());
    run_logged(logger, iptables, &command_args, dry_run);
}

fn delete_existing_rules(iptables: &str, table: &str, chain: &str, args: &[String], dry_run: bool) {
    if dry_run {
        return;
    }
    let mut command_args = Vec::new();
    if table != "filter" {
        command_args.extend(["-t".to_string(), table.to_string()]);
    }
    command_args.extend(["-D".to_string(), chain.to_string()]);
    command_args.extend(args.iter().cloned());
    while command_success_null_owned(iptables, &command_args) {}
}

fn cleanup_bridge_forward_rules(iptables: &str, bridge: &str) {
    let Ok(output) = command_output(iptables, &["-S", "FORWARD"]) else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.first() != Some(&"-A") || tokens.get(1) != Some(&"FORWARD") {
            continue;
        }
        let mut input = "";
        let mut output_iface = "";
        let mut jump = "";
        let mut index = 0;
        while index < tokens.len() {
            match tokens[index] {
                "-i" if index + 1 < tokens.len() => {
                    input = tokens[index + 1];
                    index += 1;
                }
                "-o" if index + 1 < tokens.len() => {
                    output_iface = tokens[index + 1];
                    index += 1;
                }
                "-j" if index + 1 < tokens.len() => {
                    jump = tokens[index + 1];
                    index += 1;
                }
                _ => {}
            }
            index += 1;
        }
        if input == bridge && jump == "ACCEPT" && !output_iface.is_empty() {
            while command_success_null(
                iptables,
                &[
                    "-D",
                    "FORWARD",
                    "-i",
                    bridge,
                    "-o",
                    output_iface,
                    "-j",
                    "ACCEPT",
                ],
            ) {}
        }
    }
}

fn set_sysctl_value(key: &str, value: &str, dry_run: bool, logger: Logger<'_>) {
    let proc_path = PathBuf::from("/proc/sys").join(key.replace('.', "/"));
    if path_writable(&proc_path) {
        if dry_run {
            logger.line(format!("+ echo {value} > {}", proc_path.display()));
            return;
        }
        if fs::write(&proc_path, format!("{value}\n")).is_ok() {
            return;
        }
    }
    if have_command("sysctl") {
        run_logged(
            logger,
            "sysctl",
            &["-w".to_string(), format!("{key}={value}")],
            dry_run,
        );
        return;
    }
    logger.line(format!("warn: cannot set {key}"));
}

fn write_oom_score_adj(pid: u32, name: &str, oom_score_adj: &str, dry_run: bool) {
    let path = PathBuf::from(format!("/proc/{pid}/oom_score_adj"));
    let score_path = PathBuf::from(format!("/proc/{pid}/oom_score"));
    if !path_writable(&path) {
        println!("skip {name} pid={pid}: {} not writable", path.display());
        return;
    }
    let old = read_trimmed(&path).unwrap_or_else(|| "unknown".to_string());
    let score = read_trimmed(&score_path).unwrap_or_else(|| "unknown".to_string());
    if dry_run {
        println!(
            "+ echo {oom_score_adj} > {} (old={old} oom_score={score} name={name})",
            path.display()
        );
        return;
    }
    match fs::write(&path, format!("{oom_score_adj}\n")) {
        Ok(()) => {
            let new = read_trimmed(&path).unwrap_or_else(|| "unknown".to_string());
            println!("protected {name} pid={pid} oom_score_adj {old}->{new} oom_score={score}");
        }
        Err(_) => println!("skip {name} pid={pid}: {} not writable", path.display()),
    }
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
        let comm = entry.path().join("comm");
        if read_trimmed(&comm).as_deref() == Some(name) {
            pids.push(pid);
        }
    }
    pids.sort_unstable();
    pids
}

fn read_pid_file(path: &Path) -> Option<u32> {
    read_trimmed(path)?.parse().ok()
}

fn pid_alive(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

fn install_signal_handlers() {
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
    }
}

extern "C" fn handle_signal(_: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
}

fn sleep_watch(seconds: u64) {
    thread::sleep(Duration::from_secs(seconds));
}

fn log_wait_once(path: &Path, last_wait: &mut String, key: &str, message: impl AsRef<str>) {
    if last_wait != key {
        watchdog_log(path, message.as_ref());
        *last_wait = key.to_string();
    }
}

fn watchdog_log(path: &Path, message: impl AsRef<str>) {
    append_line_or_stdout(path, &format!("{} {}", timestamp(), message.as_ref()));
}

fn timestamp() -> String {
    command_output("date", &["+%Y-%m-%d %H:%M:%S"])
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "now".to_string())
}

fn append_line_or_stdout(path: &Path, message: &str) {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => {
            if writeln!(file, "{message}").is_err() {
                println!("{message}");
            }
        }
        Err(_) => println!("{message}"),
    }
}

fn run_logged(logger: Logger<'_>, command: &str, args: &[String], dry_run: bool) -> bool {
    if dry_run {
        logger.line(format!("+ {}", format_command(command, args)));
        return true;
    }
    match Command::new(command).args(args).output() {
        Ok(output) => {
            log_command_output(logger, &output.stdout);
            log_command_output(logger, &output.stderr);
            output.status.success()
        }
        Err(error) => {
            logger.line(format!("{command}: {error}"));
            false
        }
    }
}

fn log_command_output(logger: Logger<'_>, bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        logger.line(line);
    }
}

fn format_command(command: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(command.to_string());
    parts.extend(args.iter().cloned());
    parts.join(" ")
}

fn command_output(command: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(command).args(args).output()
}

fn command_success_silent(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn command_success_null(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn command_success_null_owned(command: &str, args: &[String]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn command_stdout_trim(command: &str, args: &[&str]) -> String {
    command_output(command, args)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn parse_dev_field(route: &str) -> Option<String> {
    token_after(route, "dev")
}

fn token_after(text: &str, needle: &str) -> Option<String> {
    let mut tokens = text.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == needle {
            return tokens.next().map(str::to_string);
        }
    }
    None
}

fn bridge_gateway_cidr(subnet: &str) -> Option<String> {
    let (addr, prefix) = subnet.split_once('/')?;
    let mut octets = [0_u8; 4];
    let parts: Vec<&str> = addr.split('.').collect();
    if parts.len() != 4 || prefix.parse::<u8>().ok()? > 32 {
        return None;
    }
    for (index, part) in parts.iter().enumerate() {
        octets[index] = part.parse().ok()?;
    }
    octets[3] = 1;
    Some(format!(
        "{}.{}.{}.{}/{}",
        octets[0], octets[1], octets[2], octets[3], prefix
    ))
}

fn looks_like_ipv4_cidr(value: &str) -> bool {
    value.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'/'))
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
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

fn path_writable(path: &Path) -> bool {
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

fn ensure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_nonempty(name).map(PathBuf::from)
}

fn env_priority(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dev_field_from_route() {
        assert_eq!(
            parse_dev_field("1.1.1.1 via 192.0.2.1 dev wlan0 src 192.0.2.2"),
            Some("wlan0".to_string())
        );
        assert_eq!(parse_dev_field("local 1.1.1.1 src 127.0.0.1"), None);
    }

    #[test]
    fn parses_token_after_in_multiline_addr_output() {
        assert_eq!(
            token_after(
                "    inet 172.31.0.1/16 brd 172.31.255.255 scope global docker0",
                "inet"
            ),
            Some("172.31.0.1/16".to_string())
        );
    }

    #[test]
    fn detects_ipv4_cidr_tokens() {
        assert!(looks_like_ipv4_cidr("172.31.0.0/16"));
        assert!(!looks_like_ipv4_cidr("default"));
        assert!(!looks_like_ipv4_cidr("fe80::/64"));
    }

    #[test]
    fn derives_bridge_gateway_from_subnet() {
        assert_eq!(
            bridge_gateway_cidr("172.32.0.0/16"),
            Some("172.32.0.1/16".to_string())
        );
        assert_eq!(bridge_gateway_cidr("bad"), None);
    }

    #[test]
    fn parses_bridge_reconcile_args() {
        let args = vec![
            "--bridge".to_string(),
            "lxcbr0".to_string(),
            "--subnet".to_string(),
            "172.32.0.0/16".to_string(),
            "--owner".to_string(),
            "lxc".to_string(),
        ];

        assert_eq!(
            parse_bridge_reconcile_args(&args).unwrap(),
            BridgeReconcileArgs {
                bridge: Some("lxcbr0".to_string()),
                subnet: Some("172.32.0.0/16".to_string()),
                owner: "lxc".to_string(),
            }
        );
    }

    #[test]
    fn priority_falls_back_for_invalid_env() {
        env::set_var("ACHOST_CONTAINER_RETURN_RULE_PRIORITY", "bad");
        assert_eq!(
            env_priority("ACHOST_CONTAINER_RETURN_RULE_PRIORITY", 11999),
            11999
        );
        env::remove_var("ACHOST_CONTAINER_RETURN_RULE_PRIORITY");
    }
}
