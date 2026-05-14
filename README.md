# Android Container Host Kernel Layer

Android Container Host Kernel Layer（ACHost / ACHKL）是一个外置的 Android 容器宿主支持项目，用来评估、准备和验证 Android 内核是否足以稳定运行 Docker / LXC。

项目不属于任何目标内核树。目标内核只是 `achost` 命令的输入；本项目通过 profile、config fragment、patch、运行时模块和验证脚本，把“这个 Android 内核能不能当容器宿主”变成可检查、可打包、可迁移的流程。

## 当前状态

Docker native runtime 是当前主线，已经完成核心闭环：

- Docker 使用 `achost-docker-runtime start|stop` 直接管理 containerd、dockerd、cgroup、native namespace、socket 和网络 reconcile。
- Docker 默认只走 native runtime：`ACHOST_RUNTIME_MODE=native`、`ACHOST_USE_CHROOT=0`。
- 不再生成 Docker chroot 启动配置；CLI 的 `--docker-runtime-mode` 只接受 `native`。
- split 模块边界已明确：
  - `achost-base`：公共 runtime、`achost-runtime-core`、`achost-supervise`。
  - `achost-docker`：Docker 引擎、`achost-docker-runtime`、WebUI API、Docker smoke/feature tests。
  - `achost-lxc`：通用 LXC runtime、LXC 配置、LXC userland asset、通用容器 WebUI/API 和 verified rootfs 导入。
- WebUI 已支持 Docker 状态、启动/停止、容器列表/创建/启动/停止/重启/删除/日志/inspect、镜像列表/拉取/删除、daemon 日志；LXC 模块提供独立的通用容器面板。
- 已在 Xiaomi lmi / sm8250 / Android 16 / Linux 4.19 vendor kernel 上验证 native Docker socket、containerd、overlay2、bridge NAT、runtime smoke 和 WebUI API。

LXC 基础模块已经具备 Rust lifecycle CLI、通用 WebUI/API、容器自启、强制停止、容器内系统状态和通用用户密码管理；模块安装时会在 `/data/adb/ksu/bin` 暴露 ACHost 管理的 `lxc*`/`lxd*` wrapper。端到端容器启动仍取决于可用的 Android/arm64 LXC userland 与 rootfs 资产。qtaguid 修复和自动 patch 应用仍是后续工作。

## 这个项目是否通用

目标是通用项目：**只要设备内核和 Android 运行环境提供足够能力，就不应绑定某个机型才能用。**

但是它不是“所有 Android 手机直接刷模块就能跑 Docker”。是否可用取决于：

1. 内核是否有足够 namespace、cgroup、overlayfs、veth/bridge、netfilter/iptables 能力。
2. root 环境是否允许模块放置二进制、启动守护进程、创建 socket、挂载 cgroup。
3. Android ROM/SELinux/lmkd/netd 是否不会持续破坏容器所需进程、网络和挂载状态。
4. 是否提供 Android/arm64 可执行的 Docker userland 资产。

换句话说：ACHost 是通用的检测、打包、运行和验证层；每个设备仍必须先通过内核能力检查和设备验证。详细适配说明见 [`docs/device-compatibility.md`](docs/device-compatibility.md)。

## 目录结构

```text
achost/                         Python CLI 与打包逻辑
bin/achost                      CLI 入口
config/fragments/               内核配置片段
crates/achost-runtime-core/      公共 Rust runtime：网络、uplink、watchdog、OOM 保护
crates/achost-docker-runtime/    Docker Rust runtime：native root、cgroup、config、start/stop
crates/achost-supervise/         native namespace supervisor
crates/achost-webui-api/         WebUI 后端 API
runtime/android/                 Android 端 env、validate、模块入口模板
scripts/                         本地与设备运行时验证脚本
profiles/                        可组合的内核能力 profile
devices/                         已知设备元数据
out/                             生成的模块目录和 zip 包
```

## 快速检查目标内核

下面示例以 Xiaomi lmi 内核路径为例。换机型时替换 `--kernel-tree`、`--out` 和设备元数据文件。

```bash
bin/achost detect \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out

bin/achost plan \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out \
  --device devices/xiaomi-sm8250-lmi.yml \
  --write-report

bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1

scripts/verify-config.sh /path/to/android_kernel/out/.config

third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh /path/to/android_kernel/out/.config
```

如果目标设备没有现成 metadata，可以先用 `profiles/docker-bridge-overlay2.yml` 做最小 Docker 能力检查，再新增 `devices/<vendor-soc-device>.yml` 描述设备默认 cgroup、bridge、subnet 和已知限制。

## 生成运行时包

`runtime-install` 不下载 Docker/LXC 二进制。Docker 支持需要显式提供 Android/arm64 Docker static tarball；可选提供 Compose、buildx、BuildKit 资产。

### 推荐 split 模块

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target base \
  --cgroup-mode v1 \
  --output out/achost-base \
  --zip out/achost-base.zip

PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target docker \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --compose-asset /path/to/docker-compose-linux-aarch64 \
  --compose-sha256 <sha256> \
  --buildx-asset /path/to/docker-buildx-linux-arm64 \
  --buildx-sha256 <sha256> \
  --buildkit-asset /path/to/buildkit-aarch64.tar.gz \
  --buildkit-sha256 <sha256> \
  --output out/achost-docker \
  --zip out/achost-docker.zip

PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target lxc \
  --cgroup-mode v1 \
  --lxc-asset /path/to/lxc-userland-aarch64.tar.gz \
  --lxc-sha256 <sha256> \
  --output out/achost-lxc \
  --zip out/achost-lxc.zip
```

安装顺序建议：先刷 `achost-base.zip`，再按需要刷 `achost-docker.zip` 或 `achost-lxc.zip`。Ubuntu 26.04 作为 rootfs tarball 进入导入流程，不再生成独立 `achost-lxc-ubuntu.zip`。

### 手动包

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode manual \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/runtime-manual-docker
```

手动安装后按生成脚本提示复制到 `/data/adb/achost`。

## Android 设备上怎么用 Docker

split 模块安装后常用路径：

```text
/data/adb/modules/achost-base/achost       公共组件
/data/adb/modules/achost-docker/achost     Docker 组件
/data/adb/achost                           持久数据、run、log、Docker root
```

启动和停止：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime start'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop'
```

Docker socket：

```text
/data/adb/achost/run/docker.sock
/data/adb/achost/run/containerd.sock
```

Docker daemon 的 native namespace 内还会暴露：

```text
/run/docker.sock
/var/run/docker.sock
/var/run -> /run
```

模块会安装 Docker CLI wrapper；在 root shell 中通常可以直接运行：

```sh
docker version
docker ps
docker run --rm --network none <local-image> true
```

文档入口见 [`docs/README.md`](docs/README.md)，更多运行和验证说明见 [`docs/runtime-usage.md`](docs/runtime-usage.md)。

## Android 设备上怎么用 LXC

`achost-lxc` 是通用 LXC 模块：

```text
/data/adb/modules/achost-lxc/achost          通用 LXC runtime、配置、userland、WebUI
/data/adb/achost/lxc/containers              容器可变状态
/data/adb/achost/log/lxc                     LXC 日志
```

基础模块安装后可以直接在 root shell 使用：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime write-configs'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-host'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json'
```

Ubuntu 26.04 不再作为独立模块打包。先把 rootfs tarball 放到设备路径，再导入；`--sha256` 可选，提供时会先校验再导入：

```sh
adb push ubuntu-26.04-arm64-rootfs.tar.gz /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime import-rootfs --name ubuntu-26.04 --rootfs-asset /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz --distro ubuntu --release 26.04 --arch arm64 --sha256 <sha256>'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime start ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime exec ubuntu-26.04 -- /bin/sh -c "cat /etc/os-release"'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04'
```

常用容器管理和密码命令也在基础 LXC runtime 中：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-autostart ubuntu-26.04 on'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime autostart'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime system-status ubuntu-26.04 --json'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04 --force'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime generate-password ubuntu-26.04 --user root --json'
printf '%s\n' "$NEW_PASSWORD" | su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-password ubuntu-26.04 --user root --stdin --json'
```

`achost-lxc` 不携带 Ubuntu rootfs。打开 `achost-lxc` 的 WebUI 管理任意 LXC 容器，包括 Ubuntu 容器；WebUI 导入页要求 rootfs 已在设备路径上，SHA-256 可留空，可选导入后启动容器。

## WebUI

Docker 模块和 LXC 模块各自包含独立 WebUI 静态文件，并复用同一个 Rust `achost-webui-api` 后端；base 模块不提供 WebUI host。Docker WebUI 只管理 Docker，LXC WebUI 只管理 LXC/rootfs/用户密码。

Docker API 示例：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh status'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-containers'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-images'
```

LXC API 示例：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-status'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-list'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-import-rootfs ubuntu-26.04 /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz ubuntu 26.04 arm64 [sha256]'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-check'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-start ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-stop ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-force-stop ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-set-autostart ubuntu-26.04 on'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-system-status ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-generate-password ubuntu-26.04 root'
su -c 'ACHOST_LXC_PASSWORD="$NEW_PASSWORD" /data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-set-password ubuntu-26.04 root'
```

rootfs 导入、生命周期、自启、强制停止和通用用户密码管理都由基础 `achost-lxc` 后端/API 提供。密码管理会直接更新容器 rootfs 的 `/etc/shadow` SHA-512 hash，不依赖容器内 `chpasswd`；自定义密码会尽量通过环境变量或 stdin 传递，不把明文拼进普通命令参数或日志，但不承诺抵御 root 级观察。

如果通过模块或外部 HTTP 服务暴露 WebUI，确保只绑定可信网络或本机端口；管理接口具备启动/删除容器、删除镜像、在容器内执行命令和修改容器用户密码的能力，不应暴露给不可信网络。

## 运行时验证

推荐先跑 Docker-only 验证：

```sh
su -c 'MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

默认 Docker smoke 是本地模式，不依赖 Docker Hub：脚本会用已打包的 Docker CLI 构造 tiny local image，验证 Docker version/info、overlay2 和 `--network none` 容器。

可选 smoke 模式：

```sh
DOCKER_SMOKE_MODE=local-bridge   # 额外验证 bridge/veth attach
DOCKER_SMOKE_MODE=publish        # 验证 docker-proxy 和 127.0.0.1 发布端口
DOCKER_SMOKE_MODE=full           # 需要 registry 访问和外网镜像拉取
```

`runtime-docker-feature-test.sh` 会测试 Docker exec、cp、bind mount、proxy env 等功能；如果没有 `/data/local/tmp/achost-dockertest-rootfs.tar`，相关 feature matrix 会 skip，不阻断基础 smoke。

LXC 基础验证：

```sh
su -c 'MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

默认只验证基础 LXC runtime、配置、userland asset 和 bridge 准备；如果要跑 import/start/exec/stop 端到端容器 smoke，显式提供 rootfs：

```sh
su -c 'ROOTFS_ASSET=/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz ROOTFS_SHA256=<sha256> MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

没有 `ROOTFS_ASSET` 时，`runtime-smoke-lxc.sh` 会明确输出 skipped，不会伪装成已验证容器启动。

## 内核能力基线

Docker native runtime 至少需要：

- namespaces：`CONFIG_NAMESPACES`、`CONFIG_UTS_NS`、`CONFIG_IPC_NS`、`CONFIG_PID_NS`、`CONFIG_NET_NS`。
- cgroup v1 基线：`CONFIG_CGROUPS`、`CONFIG_CGROUP_DEVICE`、`CONFIG_CGROUP_PIDS`、`CONFIG_CGROUP_SCHED`、`CONFIG_CPUSETS`、`CONFIG_MEMCG`。
- overlay2：`CONFIG_OVERLAY_FS`，底层文件系统需要 xattr/security/ACL 支持。
- bridge/NAT：bridge、veth、netfilter、conntrack、iptables nat/filter、MASQUERADE、addrtype、conntrack match。
- Android 兼容项：PSI、BPF/cgroup BPF、iptables/bpf 相关能力按设备情况检查。

不要只看 defconfig。必须验证最终 `out/.config`，再用设备 runtime-test 验证真实运行行为。

## 已知限制

- 已验证设备是 `devices/xiaomi-sm8250-lmi.yml` 描述的 lmi/sm8250 Android 16 4.19 vendor kernel；其它设备需要按同样流程验证。
- Docker native runtime 是支持路径；Docker chroot 启动路径已淘汰。
- cgroup v2 可生成配置，但当前已验证稳定路径是 Android 16/lmi 上的 cgroup v1 布局。
- qtaguid/container-safe patch 仍是占位风险项，不应宣称已经完整解决 Android 流量统计兼容。
- LXC 基础模块已有 Rust lifecycle、WebUI/API 和通用容器内管理能力，但端到端容器启动和密码修改仍需要实际 Android/arm64 LXC userland 与 rootfs 资产配合验证。

## 开发验证清单

修改 runtime、打包或 WebUI 后至少运行：

```bash
python3 tests/test_runtime_install.py
python3 tests/test_runtime_test.py
python3 tests/test_moby_check_config.py
cargo fmt --manifest-path Cargo.toml --all --check
cargo test --manifest-path Cargo.toml --workspace
cargo clippy --manifest-path Cargo.toml --workspace -- -D warnings
npm run build --prefix webui
```

设备验证后清理：

```sh
docker rm -f achost-nginx achost-publish-test achost-feature-life achost-feature-cp 2>/dev/null || true
for image in $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E '^achost-local-smoke:|^achost-dockertest:' 2>/dev/null || true); do docker rmi "$image" || true; done
for name in achost-lxc-smoke achost-lxc-smoke-*; do /data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime destroy "$name" 2>/dev/null || true; done
rm -rf /data/local/tmp/achost-runtime-test* /data/local/tmp/achost-feature-* /data/local/tmp/achost-local-rootfs*
```
