# ACHost Rust runtime refactor design

## Goal

Move complex ACHost Docker/runtime/network lifecycle logic from shell into Rust while keeping the project simple. This is not a full rewrite of every script. The target is the high-risk runtime code that manages Docker startup, native runtime paths, sockets, cgroups, mounts, daemon lifecycle, and network reconciliation.

Rust replacements should delete the shell logic they replace. Do not add shell fallback paths or compatibility branches for rollback.

## Current state

Already implemented in Rust:

- `achost-runtime-core`
  - `detect-uplink`
  - `net-reconcile`
  - `net-watchdog`
  - `protect-daemons`
- `achost-webui-api`
  - Docker/WebUI API status and operations
- `achost-docker-runtime`
  - `cleanup-stale-iptables`
  - `stop`

Docker native runtime is the supported Docker start path. The package generator and CLI accept or write native Docker runtime configuration only, and runtime env defaults resolve:

- `ACHOST_RUNTIME_MODE=native`
- `ACHOST_USE_CHROOT=0`

Unknown or unsupported Docker runtime modes must not silently expand chroot usage; they should be normalized to native at runtime boundaries or rejected with a clear error.

The connected device validation showed native Docker can run with:

- primary socket: `/data/adb/achost/run/docker.sock`
- containerd socket: `/data/adb/achost/run/containerd.sock`
- native namespace sockets: `/run/docker.sock` and `/var/run/docker.sock`
- `/var/run -> /run` inside the native namespace
- no required chroot mounts

## Scope

### Rewrite and delete when replaced

1. Docker lifecycle shell entrypoints
   - Completed in Phase 5.
   - Startup and stop call sites use `achost-docker-runtime start|stop` directly.
   - Upgrade/install scripts prune the deleted legacy entrypoints from existing installs.

2. Common runtime shell wrappers
   - Completed in Phase 6.
   - Network, uplink, watchdog, and OOM protection call sites use `achost-runtime-core` directly.
   - Upgrade/install scripts prune the deleted legacy entrypoints from existing installs.

3. `runtime/android/bin/achost-container-validate.sh`
   - Rewrite later as Rust validation subcommands after Docker start has been simplified.

### Keep as minimal shell when required

KernelSU/Magisk entrypoints may remain shell because the platform expects them:

- `service.sh`
- `customize.sh`
- `post-fs-data.sh`
- `uninstall.sh`

These should only locate paths, source minimal environment if needed, and exec Rust helpers. Complex logic should move out of them.

### Defer

Do not prioritize test, smoke, diagnostic, or build helper scripts unless they block the runtime cleanup:

- `runtime-smoke-docker.sh`
- `runtime-docker-feature-test.sh`
- `runtime-test.sh`
- `collect-logs.sh`
- `verify-*`
- patch/config helper scripts

## Implementation phases

### Phase 0: Native default and Rust config ownership

Commit the validated default switch to native:

- `achost/runtime_install.py`
- `achost/cli.py`
- `runtime/android/bin/achost-container-env.sh`
- `tests/test_runtime_install.py`

Start building shared Rust config ownership in `achost-docker-runtime`: every future `start|stop|prepare-*` subcommand must be able to resolve ACHost paths, split-module locations, sockets, pid files, log files, and executable locations from env/config defaults without relying on a deleted shell wrapper to source everything first.

Regenerate split zips and verify the manifests report `docker_runtime_mode=native`.

### Phase 1: Native path and socket preparation

Add subcommands to `achost-docker-runtime`:

- `prepare-native-root`
- `native-preflight`

Move and delete shell equivalents from the legacy Docker start entrypoint:

- `native_preflight`
- `daemon_namespace_diagnostics`
- `write_native_resolv_conf`
- `setup_native_ca_certs`
- `setup_native_root_files`
- native portion of `prepare_docker_compat_socket`

Responsibilities:

- create runtime directories
- prepare `/data/adb/achost/native-root`
- write native `resolv.conf`
- expose CA certs as needed
- create `/var/run -> /run` in native root
- validate primary and compatibility socket paths
- report namespace diagnostics without depending on shell parsing

### Phase 2: Docker/containerd config generation

Add subcommand:

- `write-configs`

Move and delete shell equivalents:

- `write_dockerd_config`
- `write_containerd_config`

Responsibilities:

- write dockerd runtime config
- write containerd config
- set root/state/exec-root paths
- configure Docker hosts:
  - `unix:///data/adb/achost/run/docker.sock`
  - `unix:///var/run/docker.sock`
- set cgroup driver and runtime paths
- keep generated JSON/TOML deterministic for tests

### Phase 3: Cgroup and mount preparation

Add subcommand:

- `prepare-cgroups`

Move and delete shell equivalents:

- `setup_devices_cgroup`
- `ensure_host_memory_cgroup`
- cgroup v1/v2 diagnostics used by Docker startup

Responsibilities:

- mount `/dev/memcg` when needed
- mount `/dev/achost-cgroup/devices` when needed
- report cgroup v1/v2 state
- avoid expanding chroot support; native remains the default path

### Phase 4: Daemon startup orchestration

Add subcommand:

- `start`

Move and delete shell equivalents:

- supervisor server startup
- `start_containerd_daemon`
- `start_dockerd_external_containerd`
- `start_dockerd_managed_containerd`
- socket wait
- bridge wait
- network reconcile call
- startup status output

Responsibilities:

- start supervisor with native root
- start containerd
- start dockerd
- wait for Docker and containerd sockets
- reconcile network using `achost-runtime-core net-reconcile`
- preserve concise startup output used by tests and users

### Phase 5: Remove Docker lifecycle shell wrappers

After `achost-docker-runtime start` and `stop` are direct call targets:

- update `service.sh`, `customize.sh`, `uninstall.sh`, WebUI API, manual installer templates, runtime tests, package tests, README, and SPEC references
- remove legacy Docker lifecycle entrypoints from source and package manifests
- prune legacy Docker lifecycle entrypoints during install/upgrade

Deletion gates:

- grep finds no runtime call sites for the deleted wrappers
- generated manifests contain no deleted wrappers
- WebUI start/stop works
- boot autostart works
- uninstall stops Docker through Rust
- device verification leaves no unexpected chroot/native-root mounts or test artifacts

### Phase 6: Remove common runtime wrappers

After call sites use `achost-runtime-core` directly:

- remove legacy common runtime wrappers from source and package manifests
- prune legacy common runtime wrappers during install/upgrade
- keep split package boundaries intact:
  - base/common owns `achost-runtime-core`
  - docker owns `achost-docker-runtime` and WebUI API
  - lxc remains independent

## Verification

Each phase must run local checks relevant to the touched code:

- Rust format, tests, and clippy for workspace changes
- `tests/test_runtime_install.py`
- `tests/test_runtime_test.py`
- affected shell syntax checks while shell remains
- split zip regeneration when packaging changes
- zip boundary checks

Device verification uses Windows `adb.exe` from WSL.

For Docker/native phases verify:

- runtime mode is `native`
- WebUI API status reports `runtime_mode=native`
- Docker CLI works through `/data/adb/achost/run/docker.sock`
- native namespace has `/run/docker.sock` and `/var/run/docker.sock`
- container bind mounts work for primary and compatibility sockets
- `ctr --address /data/adb/achost/run/containerd.sock version` works
- no unexpected chroot mounts remain

Clean up after device tests:

- test containers
- test images
- `/data/local/tmp/achost-*`
- temporary rootfs directories
- temporary config backups
- unneeded sockets/pid files from failed tests

Restore Docker running/stopped state to what it was before each device test.

## Commit strategy

Use one commit per phase. Commit messages are Chinese and do not include Claude/AI/co-author trailers.

Suggested phase messages:

- `调整：默认使用 Docker native runtime`
- `重构：用 Rust 准备 Docker native 路径`
- `重构：用 Rust 生成 Docker 运行配置`
- `重构：用 Rust 准备 Docker cgroup`
- `重构：用 Rust 接管 Docker 启动流程`
- `清理：删除 Docker 生命周期 shell 入口`
- `清理：删除 common runtime shell 包装`
