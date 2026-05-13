# ACHost LXC Rust runtime support design

## Goal

Bring LXC support up to the same project quality bar as Docker native runtime: clear module boundaries, Rust-owned lifecycle and validation logic, reproducible device tests, and no hidden dependency on Docker.

This is a Rust-first LXC implementation. Do not build a new layer of complex shell wrappers and then replace it later. Shell may remain only where Android module systems require shell entrypoints (`service.sh`, `customize.sh`, `post-fs-data.sh`, `uninstall.sh`) or where scripts are purely developer/test helpers. Runtime validation, config generation, asset checks, bridge preparation, and lifecycle orchestration should be Rust from the start.

High performance matters: avoid grep/sed/awk-heavy process chains, repeated filesystem scans, unbounded sleeps, and shell parsing. Prefer direct Rust parsing of procfs/sysfs/cgroup files, deterministic config rendering, bounded waits, and a small number of external process calls. LXC itself remains the container runtime; ACHost should not reimplement liblxc, but should orchestrate LXC tools through a fast Rust runtime boundary.

## Current state

Already present:

- Kernel fragments for LXC baseline:
  - `config/fragments/common/lxc-base.config`
  - namespace and cgroup fragments shared with Docker.
- Package slots:
  - `--module-target lxc`
  - `--lxc-asset`
  - `--lxc-sha256`
- LXC module content:
  - `achost-lxc-validate.sh`
  - `verify-lxc-checkconfig.sh`
  - `runtime-smoke-lxc.sh`
  - `runtime/android/lxc/android-common.conf`
  - `runtime/android/lxc/default.conf`
  - `runtime/android/lxc/unprivileged.conf`
- Split module structure:
  - `achost-lxc` depends on `achost-base`.
  - `achost-lxc` does not contain Docker runtime/WebUI/common runtime core.

Known gaps:

- No Rust LXC runtime binary exists yet.
- LXC lifecycle is not owned by ACHost Rust code.
- LXC validation and smoke are shell scripts.
- `runtime-test.sh` currently has Docker split path handling, but LXC split path handling still needs first-class wiring.
- Default LXC network config uses `docker0`, which silently couples LXC to Docker while `achost-lxc` only declares a dependency on `achost-base`.
- LXC asset extraction is generic and does not require a known working LXC binary set.
- Smoke test uses `lxc-create -t download`, which is network/template dependent and not reproducible by default.
- Unprivileged LXC config is only a placeholder; Android idmap/subuid/subgid/newuidmap handling is not implemented.

## Design principles

### Rust first

Add a dedicated Rust binary:

```text
crates/achost-lxc-runtime
achost/bin/achost-lxc-runtime
```

The LXC module owns this binary. New LXC runtime behavior should land here, not in shell.

Initial subcommands:

```text
achost-lxc-runtime validate-host
achost-lxc-runtime validate-assets
achost-lxc-runtime write-configs
achost-lxc-runtime prepare-bridge
achost-lxc-runtime import-rootfs
achost-lxc-runtime start
achost-lxc-runtime stop
achost-lxc-runtime status
achost-lxc-runtime list
achost-lxc-runtime exec
achost-lxc-runtime destroy
achost-lxc-runtime smoke
```

Not every subcommand must land in one commit, but new phases should add Rust subcommands directly.

### Reusable logic belongs in base

Anything useful for both Docker and LXC should live in `achost-base`, either as `achost-runtime-core` subcommands or a shared Rust library used by binaries.

On-device reusable runtime should be in base:

```text
achost-runtime-core detect-uplink
achost-runtime-core net-reconcile
achost-runtime-core net-watchdog
achost-runtime-core protect-daemons
```

Planned reusable additions:

```text
achost-runtime-core bridge-reconcile --bridge <name> --subnet <cidr> --owner <docker|lxc>
achost-runtime-core bridge-watchdog --bridge <name> --subnet <cidr>
achost-runtime-core cgroup-status
achost-runtime-core protect-daemons --name lxc-start --name lxc-monitor --name lxc-attach
```

Compile-time reusable code may be factored into a shared Rust library crate if useful:

```text
crates/achost-runtime-shared
```

That library would not change package boundaries by itself. Installed runtime commands still follow module ownership:

- base installs `achost-runtime-core` and `achost-supervise`.
- docker installs `achost-docker-runtime` and Docker/WebUI assets.
- lxc installs `achost-lxc-runtime`, LXC configs, and LXC userland assets.

### LXC must not depend on Docker by default

The first supported LXC network model should be independent from Docker:

```text
LXC_BRIDGE=lxcbr0
LXC_SUBNET=172.32.0.0/16
```

`docker0` may remain an explicit compatibility mode, but not the default. If a user chooses `docker0`, the docs and runtime output must say that Docker or an equivalent bridge owner must create it.

The default LXC path should be:

1. base owns generic uplink detection, bridge NAT reconciliation, and watchdog logic.
2. lxc owns LXC config and lifecycle.
3. lxc asks base runtime core to reconcile `lxcbr0`.
4. lxc can run without `achost-docker` installed.

### Privileged LXC first

Phase 1 support targets privileged LXC containers only.

Unprivileged LXC is deferred until there is a concrete Android idmap strategy for:

- `CONFIG_USER_NS`
- `/etc/subuid`
- `/etc/subgid`
- `newuidmap`
- `newgidmap`
- SELinux constraints
- Android root/user namespace side effects

Do not advertise unprivileged LXC as supported until it has a separate device verification path.

### Reproducible smoke first

Default smoke must not depend on external template downloads.

Preferred smoke input:

```text
/data/local/tmp/achost-lxc-rootfs.tar
```

or a package-time/rootfs asset added later:

```text
--lxc-rootfs-asset /path/to/rootfs.tar
--lxc-rootfs-sha256 <sha256>
```

`lxc-create -t download` should be optional, not the default verification path.

## Module boundaries

### `achost-base`

Owns reusable primitives:

- `achost-runtime-core`
- `achost-supervise`
- generic network reconcile/watchdog
- generic cgroup and host diagnostics
- daemon OOM protection
- runtime-test entrypoint

Base must not contain LXC binaries or LXC configs.

### `achost-lxc`

Owns LXC-specific runtime:

- `achost-lxc-runtime`
- LXC userland asset files
- LXC configs under `achost/etc/lxc/`
- optional rootfs import support
- LXC WebUI/API integration if added later

LXC must not contain Docker runtime, Docker WebUI, or `achost-runtime-core`.

### `achost-docker`

Unchanged by LXC support except where common code is factored into base. Docker must remain independently usable without LXC.

## Runtime paths

Default split paths:

```text
ACHOST_BASE=/data/adb/modules/achost-base/achost
ACHOST_LXC_MODULE=/data/adb/modules/achost-lxc/achost
ACHOST_LXC=/data/adb/modules/achost-lxc/achost/lxc
ACHOST_LXC_BIN=/data/adb/modules/achost-lxc/achost/lxc/bin
ACHOST_LXC_ETC=/data/adb/modules/achost-lxc/achost/etc/lxc
ACHOST_LXC_VAR=/data/adb/achost/lxc
ACHOST_LXC_RUN=/data/adb/achost/run/lxc
ACHOST_LXC_LOG=/data/adb/achost/log/lxc
ACHOST_LXC_ROOTFS=/data/adb/achost/lxc/rootfs
ACHOST_LXC_CONTAINERS=/data/adb/achost/lxc/containers
LXC_BRIDGE=lxcbr0
LXC_SUBNET=172.32.0.0/16
```

`achost-lxc-runtime write-configs` should render deterministic configs from these values.

Config files:

```text
achost/etc/lxc/android-common.conf
achost/etc/lxc/default.conf
achost/etc/lxc/unprivileged.conf
```

Generated runtime configs may live under:

```text
/data/adb/achost/lxc/config/<container>.conf
/data/adb/achost/lxc/containers/<container>/config
```

## LXC asset requirements

The LXC asset should be an Android/arm64 tarball supplied by the user. The package generator should keep safe extraction rules but require a known useful binary set.

Required binaries for privileged support:

```text
lxc-start
lxc-stop
lxc-attach
lxc-info
lxc-ls
lxc-destroy
lxc-execute
lxc-checkconfig
```

Conditionally required when using create/template paths:

```text
lxc-create
lxc-copy
lxc-console
```

The validator should report:

- missing required binaries
- executable bit
- ELF architecture when detectable
- dynamic linker path when detectable
- missing shared library roots when detectable
- `lxc-checkconfig` availability and result

Package manifest should include:

```json
"lxc": {
  "source": "...",
  "sha256": "...",
  "required_binaries": ["lxc-start", "lxc-stop", "..."],
  "files": {"lxc-start": "..."}
}
```

## Rust subcommands

### `validate-host`

Reads procfs/sysfs directly. Reports JSON or stable text.

Checks:

- namespace files: mnt, uts, ipc, pid, net, user
- cgroup v1/v2 controllers and mounts
- `/dev/pts` and devpts mode
- `/proc/cgroups`
- bridge/veth support visibility where possible
- iptables presence
- SELinux mode
- lmkd pressure hints
- configured bridge state
- LXC config readability

### `validate-assets`

Checks packaged LXC tools and libs. This replaces `achost-lxc-validate.sh`.

### `write-configs`

Renders deterministic LXC configs:

- Android common config
- privileged default config
- optional unprivileged placeholder config, clearly marked unsupported until Phase 2
- per-container generated config

Default network should use `lxcbr0`, not `docker0`.

### `prepare-bridge`

Prepares LXC bridge policy by calling or sharing base runtime logic.

Preferred flow:

```text
achost-lxc-runtime prepare-bridge
  -> achost-runtime-core bridge-reconcile --bridge lxcbr0 --subnet 172.32.0.0/16 --owner lxc
```

If base has not yet gained generic bridge subcommands, Phase 1 may call existing `net-reconcile` with `CONTAINER_BRIDGE=lxcbr0`, but the spec target is a generic base-owned bridge API.

### `import-rootfs`

Creates a named container from a local rootfs tar without network access.

Inputs:

```text
--name <name>
--rootfs /data/local/tmp/achost-lxc-rootfs.tar
--replace
```

Responsibilities:

- validate rootfs tar path is explicit and safe
- create container directory under `ACHOST_LXC_CONTAINERS`
- extract rootfs using safe path rules
- write container config
- avoid following symlinks outside destination
- produce deterministic status output

### `start` / `stop` / `status` / `list` / `destroy`

Orchestrates LXC tools with stable ACHost paths.

Rules:

- use direct Rust `Command`, no shell expansion
- set `PATH`, `LXC_PATH`, `LXC_ROOTFS`, `LD_LIBRARY_PATH` explicitly
- bounded waits for state transitions
- no unbounded sleep loops
- stop should attempt graceful stop, then bounded kill/destroy only for ACHost-owned containers
- destroy must only remove containers under `ACHOST_LXC_CONTAINERS`

### `exec`

Runs a command inside a named container through `lxc-attach`.

Rules:

- require explicit container name
- pass arguments directly, not through shell
- return child exit code

### `smoke`

Rust replacement for `runtime-smoke-lxc.sh`.

Default smoke steps:

1. `validate-host`
2. `validate-assets`
3. import or reuse local rootfs
4. start privileged container without external network dependency
5. `exec -- uname -a`
6. `exec -- ip addr`
7. optional bridge attach check
8. optional external ping
9. stop
10. destroy if the smoke created the container

Smoke modes:

```text
LXC_SMOKE_MODE=local          # no external network; default
LXC_SMOKE_MODE=bridge         # verify veth bridge attach
LXC_SMOKE_MODE=network        # verify external ping/DNS
LXC_SMOKE_MODE=download       # optional lxc-create -t download path
```

## Implementation phases

### Phase 0: split path and base reuse cleanup

- Update `runtime-test.sh` to locate LXC module paths the same way Docker paths are located.
- Update container validation to find LXC validator/runtime in `ACHOST_LXC_MODULE` / `ACHOST_LXC_BIN`.
- Add env defaults for LXC paths:
  - `ACHOST_LXC_ETC`
  - `ACHOST_LXC_VAR`
  - `ACHOST_LXC_RUN`
  - `ACHOST_LXC_LOG`
  - `ACHOST_LXC_CONTAINERS`
  - `LXC_BRIDGE`
  - `LXC_SUBNET`
- Keep Docker independent and do not move LXC files into base.

### Phase 1: add `achost-lxc-runtime` and asset validation

- Add crate:
  - `crates/achost-lxc-runtime`
- Add package install target:
  - `achost/bin/achost-lxc-runtime`
- Add subcommands:
  - `validate-host`
  - `validate-assets`
- Harden `--lxc-asset` extraction:
  - require key LXC binaries
  - keep allowed root restrictions
  - add manifest details
  - add package tests
- Keep `achost-lxc-validate.sh` only as a temporary thin exec wrapper if needed, then delete it once call sites use Rust directly.

### Phase 2: Rust config generation and independent bridge model

- Change default LXC config from `docker0` to `lxcbr0`.
- Add `write-configs`.
- Add base-owned generic bridge reconcile API in `achost-runtime-core`:
  - `bridge-reconcile`
  - `bridge-watchdog` if needed
- Reuse base runtime logic for uplink detection and NAT policy.
- Device verify LXC bridge without Docker installed or running.

### Phase 3: local rootfs import and privileged lifecycle

- Add `import-rootfs`.
- Add lifecycle subcommands:
  - `start`
  - `stop`
  - `status`
  - `list`
  - `destroy`
  - `exec`
- Use local rootfs by default.
- Make lifecycle state and cleanup deterministic.
- Add tests for path safety and argument handling.

### Phase 4: Rust smoke and deletion of LXC shell runtime wrappers

- Add `smoke` subcommand.
- Replace call sites for:
  - `achost-lxc-validate.sh`
  - `verify-lxc-checkconfig.sh`
  - `runtime-smoke-lxc.sh`
- Remove LXC runtime shell wrappers from package manifests once Rust call sites are direct.
- Add install/upgrade pruning for deleted LXC wrappers if they were ever installed.

### Phase 5: service/autostart and optional WebUI integration

- Add LXC autostart config if needed:
  - `/data/adb/achost/config/lxc.autostart`
- Keep `service.sh` minimal:
  - source env
  - exec `achost-lxc-runtime autostart` if enabled
- Add WebUI/API LXC support only after CLI/runtime behavior is stable:
  - list containers
  - start/stop
  - logs/status
  - exec command

### Phase 6: unprivileged LXC research path

Only start after privileged LXC is verified on device.

Required decisions:

- Android subuid/subgid storage
- `newuidmap` / `newgidmap` availability or replacement
- SELinux effects
- user namespace risk on target kernels
- cgroup delegation model

Do not mark unprivileged LXC supported until device tests pass.

## Verification

Local checks per phase:

```bash
cargo fmt --manifest-path Cargo.toml --all --check
cargo test --manifest-path Cargo.toml --workspace
cargo clippy --manifest-path Cargo.toml --workspace -- -D warnings
python3 tests/test_runtime_install.py
python3 tests/test_runtime_test.py
bash -n remaining shell entrypoints
```

Package checks:

- base contains `achost-runtime-core`, no LXC runtime/userland
- lxc contains `achost-lxc-runtime`, LXC configs, LXC userland, no Docker/WebUI/common runtime core
- docker remains independent from LXC
- manifests report LXC asset details
- stale LXC shell wrappers are absent after deletion phases

Device checks for privileged LXC:

1. Record Docker and LXC initial state.
2. Install/hot-update base and lxc modules.
3. Run:
   ```sh
   achost-lxc-runtime validate-host
   achost-lxc-runtime validate-assets
   achost-lxc-runtime prepare-bridge
   achost-lxc-runtime import-rootfs --name achost-lxc-smoke --rootfs /data/local/tmp/achost-lxc-rootfs.tar --replace
   achost-lxc-runtime start --name achost-lxc-smoke
   achost-lxc-runtime exec --name achost-lxc-smoke -- uname -a
   achost-lxc-runtime exec --name achost-lxc-smoke -- ip addr
   achost-lxc-runtime stop --name achost-lxc-smoke
   achost-lxc-runtime destroy --name achost-lxc-smoke
   ```
4. Run `MODE=lxc runtime-test.sh` through the base module.
5. Confirm no dependency on Docker for default `lxcbr0` path.
6. Clean:
   - test containers
   - test rootfs dirs
   - `/data/local/tmp/achost-lxc-*`
   - temporary bridge state if created by test
7. Restore Docker/LXC running state to pre-test state.

## Success criteria

Privileged LXC is considered supported when:

- LXC asset validation is strict and tested.
- `achost-lxc-runtime` owns validation, config generation, lifecycle, and smoke.
- Default LXC networking uses `lxcbr0` and does not require Docker.
- Local rootfs smoke passes without external downloads.
- split `MODE=lxc runtime-test.sh` works from `achost-base` and locates `achost-lxc` correctly.
- LXC module remains independent from Docker package contents.
- Device verification passes and leaves no test artifacts.

## Commit strategy

Use small commits by phase. Suggested messages:

- `重构：新增 LXC Rust 运行时入口`
- `完善：校验 LXC 用户态资产`
- `重构：用 Rust 生成 LXC 配置`
- `重构：为 LXC 准备独立 bridge`
- `重构：用 Rust 管理 LXC 生命周期`
- `重构：用 Rust 接管 LXC smoke 验证`
- `清理：删除 LXC runtime shell 包装`
