# 机型与内核适配说明

ACHost 的目标是通用 Android container host 支持层，但“通用”不等于“所有 Android 设备直接可用”。Docker/LXC 能否稳定运行取决于内核、Android userspace、root 环境和容器用户态资产。

## 适用模型

设备适合 ACHost 需要同时满足四层条件：

1. **内核能力足够**：namespace、cgroup、overlayfs、bridge/veth、netfilter/iptables。
2. **root 环境允许 daemon/mount/socket**：KernelSU/ReSukiSU/Magisk 等环境能安装模块、启动守护进程、挂载 cgroup、创建 Unix socket。
3. **Android 系统服务不持续破坏状态**：netd、lmkd、SELinux、VPN/Wi-Fi 策略不会持续打断网络、进程或挂载。
4. **用户态资产匹配**：Docker/containerd/runc、LXC userland/rootfs 是 Android/arm64 可执行文件或归档。

ACHost 负责检测、打包、运行和验证；它不能凭空补齐缺失严重的内核能力。

## 已验证设备

当前主验证设备：

```text
Xiaomi lmi / sm8250
Android 16 / crDroid
Linux 4.19 vendor kernel
ReSukiSU/KPM root 环境
cgroup v1 first
Docker native runtime
```

对应项目文件：

```text
devices/xiaomi-sm8250-lmi.yml
profiles/android-container-host-v1.yml
profiles/docker-bridge-overlay2.yml
```

已验证：

- Docker native runtime：`ACHOST_RUNTIME_MODE=native`、`ACHOST_USE_CHROOT=0`。
- `/data/adb/achost/run/docker.sock` 和 `/data/adb/achost/run/containerd.sock`。
- supervisor native namespace 内 `/run/docker.sock`、`/var/run/docker.sock`、`/var/run -> /run`。
- containerd external socket。
- overlay2 storage driver。
- docker0 bridge、veth、IPv4 NAT/MASQUERADE、policy rule reconcile。
- Docker stats 通过 containerd/shim/cgroup v1 读取真实 CPU/memory/PIDS。
- Docker local smoke、feature matrix、WebUI API。
- split module boundary：base/docker/lxc 不混包，Docker 不依赖 LXC。

这说明该设备完成主链路，不代表其它机型可以跳过验证。

## 新机型适配流程

### 1. 新建设备 metadata

复制现有设备文件：

```text
devices/xiaomi-sm8250-lmi.yml -> devices/<vendor-soc-device>.yml
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

设备特殊行为写入 `known_limitations`，例如 cgroup mount 路径、SELinux、Wi-Fi/VPN、iptables 或 LMKD 行为。

### 2. 检查内核源码和最终配置

PC，在 ACHost 仓库根目录：

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

完整 baseline：

```bash
bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1
```

### 3. 交叉检查 Docker 参考配置

PC：

```bash
third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh /path/to/android_kernel/out/.config
```

Moby check-config 不是 Android 专用检查，结论不能替代设备 runtime-test。

### 4. 生成模块并安装

见 [`build-and-package.md`](build-and-package.md) 和 [`install-and-upgrade.md`](install-and-upgrade.md)。

### 5. 设备 runtime-test

su shell：

```sh
MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh

MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

如果要声明支持某个 LXC rootfs，还必须提供 rootfs 并验证 import/start/exec/logs/stop。

## 支持分级

### 支持 Docker native runtime

至少满足：

- `verify-config` 没有关键 required 缺项。
- `achost-docker-runtime start` 成功。
- supervisor、containerd socket/API、dockerd socket/API 都 ready。
- `docker info` 显示 overlay2、cgroupfs、预期 cgroup version。
- local Docker smoke 通过。
- `net-reconcile` 成功或 watchdog 能修复网络。
- `docker stats --no-stream` 能显示真实 CPU/memory/PIDS。
- stop 后无 ACHost-owned dockerd/containerd/watchdog 残留。

### 支持 LXC 基础模块

至少满足：

- LXC host validation 通过。
- LXC asset validation 通过。
- `prepare-bridge` 能创建或修复 `lxcbr0`。
- `achost-lxc-runtime list --json` 能读取受控容器目录。
- LXC template 可执行，`LXC_TEMPLATE_PATH` 正确。

### 支持指定 LXC rootfs

在 LXC 基础模块支持之外，还要验证：

- rootfs import 成功。
- start 后进入 `RUNNING`。
- exec 能运行 `/bin/sh` 或 rootfs 内可用 shell。
- logs 可读。
- stop/force-stop 能进入 `STOPPED`。
- destroy 不留下半残容器目录。

### 部分支持

常见情况：

- local Docker smoke 通过，但 publish/full 失败：通常是 DNS、registry、docker-proxy、iptables 或 Android 防火墙问题。
- Docker daemon 可启动，但 stats 或资源限制异常：检查 cgroup mount 顺序和 memory/devices/cpuset/cpu。
- Docker 可用但 WebUI 异常：先直接运行 WebUI API，再检查 WebUI dist 打包。
- LXC list/validate 通过，但 rootfs 启动失败：重点看 rootfs、init、设备节点、mount 和 LXC 日志。

### 不支持或需要内核改造

常见阻断：

- namespace 缺失。
- cgroup devices/memory/pids/cpuset 缺失且无法挂载。
- overlayfs 缺失或底层文件系统不支持 overlay2。
- veth/bridge/netfilter/NAT 缺失。
- root 环境禁止 daemon、mount 或 socket。

这种设备需要先改内核配置、补 patch 或更换 ROM/root 环境，再重新验证。

## 适配时不要做的事

- 不要只因为配置项出现在 defconfig 或 `/proc/config.gz` 就宣称支持。
- 不要为了某台设备把路径、defconfig、ROM 名写死进核心 runtime。
- 不要把 Docker 和 LXC 混成一个不可拆模块。
- 不要保留已淘汰 shell wrapper 作为回退路径。
- 不要把测试容器、镜像、临时 rootfs 留在设备上。
