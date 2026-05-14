# 机型与内核适配说明

ACHost 的目标是成为通用的 Android container host 支持层：不把 Docker/LXC 运行能力绑定到某个手机、某个 ROM 或某个内核仓库。

但“通用”不等于“所有 Android 设备直接可用”。Docker/LXC 能否稳定运行，最终取决于目标内核、Android userspace、root 环境和提供的容器用户态资产。

## 适用模型

一个设备适合使用 ACHost，需要同时满足四层条件：

1. **内核能力足够**：namespace、cgroup、overlayfs、bridge/veth、netfilter/iptables 等能力必须存在并能工作。
2. **Android root 环境允许运行守护进程**：KernelSU/ReSukiSU/Magisk 等环境需要允许模块安装二进制、创建持久目录、启动 daemon、挂载 cgroup 和创建 Unix socket。
3. **Android 系统服务不持续破坏容器状态**：netd、lmkd、SELinux、VPN/Wi-Fi 策略可能影响网络、进程存活和文件访问。
4. **用户态资产匹配**：Docker、containerd、runc、可选 compose/buildx/buildkit，以及 LXC userland/rootfs 必须是 Android/arm64 可执行文件或归档，并且依赖尽量静态或自带。

ACHost 负责检测、打包、运行和验证这些条件；它不能凭空补齐一个缺失严重的内核。

## 已验证设备

当前已验证主设备：

```text
Xiaomi lmi / sm8250
Android 16 / crDroid
Linux 4.19 vendor kernel
cgroup v1 first
Docker native runtime
```

对应文件：

```text
devices/xiaomi-sm8250-lmi.yml
profiles/android-container-host-v1.yml
profiles/docker-bridge-overlay2.yml
```

该设备上已验证：

- Docker native runtime：`ACHOST_RUNTIME_MODE=native`、`ACHOST_USE_CHROOT=0`。
- `/data/adb/achost/run/docker.sock`。
- `/data/adb/achost/run/containerd.sock`。
- Docker daemon native namespace 内 `/run/docker.sock`、`/var/run/docker.sock`、`/var/run -> /run`。
- containerd external socket。
- overlay2 storage driver。
- docker0 bridge、veth、IPv4 NAT/MASQUERADE、policy rule reconcile。
- WebUI API status/list containers/list images。
- local Docker smoke，不依赖 Docker Hub。
- split module boundary：base/docker/lxc 不混包，Ubuntu rootfs 通过设备路径导入，SHA-256 可选。

这说明项目在该设备上完成 Docker native 主链路，但不代表其它机型可以跳过内核验证。

## Docker native 最小内核能力

### Namespace

需要：

```text
CONFIG_NAMESPACES=y
CONFIG_UTS_NS=y
CONFIG_IPC_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
```

`CONFIG_USER_NS` 对部分容器能力有帮助，尤其是未来 unprivileged LXC/rootless 类场景；Docker native 基础路径不应依赖 rootless，但建议开启并验证风险。

### cgroup

当前稳定验证路径是 cgroup v1 first：

```text
CONFIG_CGROUPS=y
CONFIG_CGROUP_DEVICE=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_FREEZER=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_SCHED=y
CONFIG_CFS_BANDWIDTH=y
CONFIG_CPUSETS=y
CONFIG_MEMCG=y
CONFIG_MEMCG_SWAP=y
CONFIG_BLK_DEV_THROTTLING=y
```

Docker 启动时会准备：

```text
/dev/memcg
/dev/achost-cgroup/devices
```

如果设备默认只有 cgroup2，必须单独验证 Docker 资源限制、devices controller、memory controller 和 Android userspace 兼容性。不要假设 v2 在所有 Android 版本上等价可用。

### overlay2

需要：

```text
CONFIG_OVERLAY_FS=y
CONFIG_EXT4_FS_POSIX_ACL=y
CONFIG_EXT4_FS_SECURITY=y
CONFIG_TMPFS_XATTR=y
CONFIG_TMPFS_POSIX_ACL=y
```

还要确认 Docker root 所在底层文件系统支持 overlay2 需要的 xattr/security 行为。只在 `.config` 中看到 `CONFIG_OVERLAY_FS=y` 不够，必须跑 `docker info` 和 smoke。

### bridge / veth / NAT

需要 bridge、veth、conntrack、iptables filter/nat/mangle、MASQUERADE 和常见 match/target：

```text
CONFIG_BRIDGE_NETFILTER=y
CONFIG_NETFILTER=y
CONFIG_NETFILTER_XTABLES=y
CONFIG_NF_CONNTRACK=y
CONFIG_NF_NAT=y
CONFIG_IP_NF_IPTABLES=y
CONFIG_IP_NF_FILTER=y
CONFIG_IP_NF_MANGLE=y
CONFIG_IP_NF_NAT=y
CONFIG_IP_NF_TARGET_MASQUERADE=y
CONFIG_NETFILTER_XT_MATCH_ADDRTYPE=y
CONFIG_NETFILTER_XT_MATCH_CONNTRACK=y
CONFIG_NETFILTER_XT_MATCH_CGROUP=y
CONFIG_NETFILTER_XT_TARGET_CHECKSUM=y
CONFIG_NETFILTER_XT_TARGET_MARK=y
CONFIG_NETFILTER_XT_MATCH_MARK=y
```

设备还需要 Android userspace 提供可用的 `iptables` 命令。ACHost 的 `net-reconcile` 会根据当前 uplink 修复 docker0 相关 NAT 和 policy rule，但无法绕过缺失的内核 netfilter 能力。

### Android 兼容项

建议检查：

```text
CONFIG_PSI=y
CONFIG_CGROUP_BPF=y
CONFIG_BPF=y
CONFIG_BPF_SYSCALL=y
CONFIG_NETFILTER_XT_MATCH_BPF=y
CONFIG_INET_UDP_DIAG=y
```

这些能力影响 lmkd/pressure 观测、Android 网络栈兼容和诊断能力。不同 ROM 的要求会有差异。

## Root / 模块环境要求

推荐 KernelSU/ReSukiSU split 模块。需要：

- 模块脚本可以创建 `/data/adb/achost`。
- 模块脚本可以给 `achost/bin/*` 设置可执行权限。
- root shell 可以启动 long-running daemon。
- 可以挂载 cgroup 或访问已存在 cgroup mount。
- 可以创建 Unix socket。
- SELinux 策略不会阻止 Docker/containerd/runc 基础行为。

如果 ROM 或 root 方案限制 daemon、mount、socket 或 exec，Docker 可能无法完整运行，即使内核配置看起来满足。

## 用户态资产要求

Docker 模块至少需要以下 Android/arm64 可执行文件：

```text
docker
dockerd
containerd
containerd-shim-runc-v2
ctr
runc
```

可选但常用：

```text
docker-init
docker-proxy
docker-compose 或 docker compose plugin
docker-buildx 或 docker buildx plugin
buildctl
buildkitd
```

`docker-proxy` 影响 `-p 127.0.0.1:host:container` 这类发布端口场景。Compose/buildx/BuildKit 不影响 Docker daemon 启动。

LXC 基础模块需要 Android/arm64 可执行的 LXC userland，例如：

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

Ubuntu 26.04 LXC 模块只需要 rootfs tar/tar.gz asset。它不是 runtime 模块，不携带 LXC 二进制、通用配置或 WebUI。

LXC 容器内管理能力由基础 `achost-lxc` 提供：系统状态需要容器能通过 `lxc-attach` 运行 `/bin/sh`；密码修改由 runtime 直接更新容器 rootfs 的 `/etc/shadow` SHA-512 hash，不依赖容器内 `chpasswd`；生命周期、自启、强制停止和日志读取都由基础 LXC runtime/API 固定动作提供。安装/升级 LXC 模块时会在 `/data/adb/ksu/bin` 暴露 ACHost 管理的 `lxc*`/`lxd*` wrapper；这些 wrapper 依赖模块内 LXC userland 和当前设备 root 环境。

## 新机型适配流程

### 1. 新建设备 metadata

复制 `devices/xiaomi-sm8250-lmi.yml`，改成目标设备：

```text
devices/<vendor-soc-device>.yml
```

至少写清：

```yaml
id: <vendor-soc-device>
arch: arm64
kernel:
  repo: <kernel repo>
  branch: <branch>
  version_hint: '<kernel version>'
  defconfig: arch/arm64/configs/<device>_defconfig
android:
  release_hint: '<android version>'
container:
  default_cgroup_mode: v1
  profiles:
    - docker-bridge-overlay2
network:
  docker_bridge: docker0
  docker_subnet: 172.31.0.0/16
  uplink_auto_detect: true
runtime:
  mode: kernelsu-module
  cgroup_mode: v1
```

如果设备有特殊 cgroup、SELinux、Wi-Fi/VPN、iptables 行为，写入 `known_limitations`。

### 2. 检查目标内核

```bash
bin/achost detect \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out

bin/achost plan \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out \
  --device devices/<vendor-soc-device>.yml \
  --write-report

bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile docker-bridge-overlay2
```

如果目标是完整 ACHost baseline，再验证：

```bash
bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1
```

### 3. 用 moby check-config 交叉检查

```bash
third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh /path/to/android_kernel/out/.config
```

Moby check-config 是 Linux Docker 参考检查，不理解 Android 的全部差异。它适合发现明显缺项，但最终结论以设备 runtime-test 为准。

### 4. 生成模块并安装

先生成 split 包。Docker 路径安装 base + docker；LXC 路径安装 base + lxc。Ubuntu rootfs 不再是模块，使用设备路径上的已验证 tarball 导入。

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

### 5. 跑设备验证

```sh
su -c 'MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
su -c 'MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

如果要验证 Ubuntu LXC 容器启动，先把 rootfs 放到设备路径，再提供 rootfs asset；`ROOTFS_SHA256` 可选，设置时会先校验：

```sh
adb push ubuntu-26.04-arm64-rootfs.tar.gz /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz
su -c 'ROOTFS_ASSET=/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz ROOTFS_SHA256=<sha256> MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

最少应确认：

```text
runtime_mode=native
use_chroot=0
Docker daemon start OK
container network reconcile OK
Docker runtime smoke OK
Docker daemon stop OK
LXC host validation OK
LXC asset validation OK
LXC prepare bridge OK
```

手动补充检查：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/docker --host unix:///data/adb/achost/run/docker.sock version'
su -c '/data/adb/modules/achost-docker/achost/bin/ctr --address /data/adb/achost/run/containerd.sock version'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh status'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-status'
```

## 结果分级

### 支持

可以认为设备支持 Docker native runtime 的条件：

- `verify-config` 没有关键 required 缺项。
- Docker native start 成功。
- Docker/containerd socket 创建成功。
- `docker info` 显示 overlay2。
- local smoke 通过。
- `net-reconcile` 成功。
- stop 后无 dockerd/containerd 残留。

可以认为设备支持 LXC 基础模块的条件：

- LXC host validation 通过。
- LXC asset validation 通过。
- `prepare-bridge` 能创建或修复 `lxcbr0`。
- `achost-lxc-runtime list --json` 能读取受控容器目录。
- 如果声明支持 Ubuntu LXC，必须额外验证 rootfs import、start、exec、logs、stop。

### 部分支持

常见情况：

- local smoke 通过，但 publish/full 模式失败：通常是外网、DNS、registry、docker-proxy、iptables 或 Android 防火墙问题。
- Docker daemon 可启动，但资源限制异常：检查 cgroup memory/devices/cpuset/cpu。
- Docker 可用但 WebUI 暴露失败：检查 HTTP 暴露方式、端口转发和 API 执行权限。

### 不支持或需要内核改造

常见阻断：

- namespace 缺失。
- cgroup devices/memory 缺失且无法挂载。
- overlayfs 缺失或底层文件系统不支持 overlay2。
- veth/bridge/netfilter/NAT 缺失。
- SELinux/root 环境禁止 daemon 或 mount。

这种设备需要先改内核配置、补 patch 或更换 ROM/root 环境，再重新验证。

## 适配时不要做的事

- 不要只因为某个配置项在 defconfig 里出现就宣称支持；必须看最终 `.config` 和运行时结果。
- 不要为了某台设备把路径、defconfig、ROM 名写死进核心 runtime。
- 不要把 Docker 和 LXC 混成一个不可拆模块；Docker 必须不依赖 LXC 独立工作。
- 不要保留已删除 shell wrapper 作为回退路径；替换后的 Rust 路径是唯一维护路径。
- 不要把测试产生的容器、镜像、临时 rootfs 留在设备上。
