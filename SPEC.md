# 项目需求书：Android Container Host Kernel Layer
# 简称：ACHKL / Android-Container-Host
# 目标：把 Android 手机内核改造成可复用、可移植、低耦合的 LXC + Docker 原生级容器宿主内核支持层

---

## 0. 背景

我正在研究 Android 手机内核上原生运行 LXC 和 Docker，不想只是“打开 CONFIG_DOCKER=y”，也不想把某个机型、某个 ROM、某个 KernelSU Action 的逻辑硬耦合进内核源码。

已有参考项目：

1. tomxi1997/lxc-docker-support-for-android
   - URL: https://github.com/tomxi1997/lxc-docker-support-for-android
   - 作用：通过 Kconfig、fix_cgroup.patch、xt_qtaguid.patch 等方式给 Android kernel 添加 LXC/Docker 所需配置和兼容补丁。
   - 不足：配置粗暴，偏 checklist，验证层不足，网络/Android userspace/lmkd/SELinux/netd 适配不足，项目结构不适合长期复用到多个手机内核。

2. tomxi1997/LXC_KernelSU_Action
   - URL: https://github.com/tomxi1997/LXC_KernelSU_Action
   - 作用：面向 Non-GKI Kernel 的 GitHub Action，宣称支持 4.9 / 4.14 / 4.19 / 5.4，自动注入 KernelSU/LXC 相关能力并构建内核。
   - 不足：更像构建工作流，不是一个可复用、分层、可验证、可移植的 Android container host 支持层。

3. 当前初始目标内核：
   - URL: https://github.com/crdroidandroid/android_kernel_xiaomi_sm8250/tree/16.0-lmi
   - 机型目标：Xiaomi lmi / sm8250 / Android kernel 4.19 系列
   - 要求：先以该内核为第一适配目标，但项目设计必须能迁移到其他 Android 手机内核。

---

## 1. 项目总目标

实现一个外置、可复用、可移植、低耦合的 Android container host 支持项目，使 Android 手机内核尽可能达到普通 Linux 主机上的 Docker/LXC 原生支持水平。

这里的“原生级别”不是指跑 chroot，也不是虚拟机，而是：

1. Docker 可以直接使用 containerd/runc 在 Android kernel 上启动 OCI 容器。
2. Docker 默认 bridge 网络可用，容器能通过 docker0/br0 + veth + NAT/MASQUERADE 访问外网。
3. Docker overlay2 存储驱动可用。
4. Docker cgroup 资源限制可用。
5. Docker 默认 namespace 隔离可用。
6. LXC privileged container 可稳定启动。
7. LXC unprivileged container 尽可能支持，至少要明确缺失项。
8. LXC system container 能接近普通 Linux 行为运行。
9. Android 的 netd、qtaguid/eBPF、SELinux、lmkd/cgroup 管理不会频繁破坏容器。
10. 支持在多个 Android kernel 版本之间移植：4.9、4.14、4.19、5.4、5.10、5.15，优先 4.19。
11. 项目成果不能深度嵌入某一个 kernel tree。必须以独立项目维护，通过脚本/patch/config fragment 对目标 kernel 进行注入。
12. 所有能力必须有验证脚本，而不是只看 defconfig。

---

## 2. 非目标

这些不是第一阶段目标：

1. 不做完整 ROM 编译系统。
2. 不强制集成 KernelSU、Magisk、APatch；这些只能作为可选运行环境。
3. 不把 Docker daemon、containerd、runc 源码改造成 Android 专用 fork，除非必要且必须独立维护 patch。
4. 不为了“能跑”而直接破坏 Android 基础网络、SELinux、安全模型。
5. 不默认开启所有高风险选项，例如不必要的 AUFS、Btrfs、Device Mapper thin provisioning、IPVS、Swarm VXLAN 加密等。
6. 不把机型名、defconfig 路径、AnyKernel3 打包方式写死到核心逻辑里。
7. 不要求所有内核版本都支持所有功能；但必须能检测、报告、降级、给出 backport 列表。

---

## 3. 核心设计原则

### 3.1 外置项目，不污染目标内核

项目必须是一个独立仓库，例如：

```text
android-container-host/
````

目标 kernel tree 只是输入。项目对目标内核的修改只能通过以下方式产生：

1. 自动生成 patch series。
2. 自动添加 Kconfig source 行。
3. 自动 merge defconfig fragment。
4. 自动应用版本化补丁。
5. 自动生成报告。

禁止把整个项目复制成目标内核里的 `utils/` 后长期混在一起维护。

允许临时注入的目录名建议为：

```text
kernel_tree/vendor/android-container-host/
```

但该目录必须可删除、可重新生成、可通过脚本管理。

---

### 3.2 分层能力模型

项目必须把容器支持拆成多个 profile，而不是一个 `CONFIG_DOCKER=y` 全选项。

至少需要这些 profile：

```text
profiles/
  lxc-minimal
  lxc-privileged
  lxc-unprivileged
  docker-base
  docker-bridge-net
  docker-overlay2
  docker-resource-control
  docker-ipv6
  docker-rootless-optional
  android-qtaguid-compat
  android-ebpf-traffic
  android-cgroup-v1
  android-cgroup-v2
  android-lmkd-protection
  debug
```

每个 profile 必须有：

```text
1. Kconfig fragment
2. 依赖说明
3. 风险说明
4. 验证脚本
5. 运行时检查项
6. 失败时的诊断建议
```

---

### 3.3 Kconfig 使用 `imply` 优先，谨慎 `select`

Linux Kconfig 官方文档提醒：`select` 会强行设置符号，不会访问依赖，滥用会产生非法配置。因此本项目新增 Kconfig 里默认使用 `imply`，只有对无依赖、隐藏符号、确实安全的情况才允许 `select`。

参考：

* [https://docs.kernel.org/kbuild/kconfig-language.html](https://docs.kernel.org/kbuild/kconfig-language.html)

要求：

1. 不要写一个巨大的 `CONFIG_DOCKER` 并 `select` 一切。
2. 使用清晰命名：

   * `CONFIG_ANDROID_CONTAINER_HOST`
   * `CONFIG_ANDROID_CONTAINER_LXC`
   * `CONFIG_ANDROID_CONTAINER_DOCKER`
   * `CONFIG_ANDROID_CONTAINER_DOCKER_BRIDGE`
   * `CONFIG_ANDROID_CONTAINER_OVERLAY2`
   * `CONFIG_ANDROID_CONTAINER_CGROUP_V1`
   * `CONFIG_ANDROID_CONTAINER_CGROUP_V2`
   * `CONFIG_ANDROID_CONTAINER_ANDROID_NET_COMPAT`
3. Kconfig 只表达能力分组，真正是否生效必须由最终 `.config` 验证。

---

### 3.4 defconfig fragment 优先

不要直接往目标 defconfig 末尾硬塞大量配置。

应提供：

```text
config/fragments/
  common/
    namespaces.config
    cgroups-v1.config
    cgroups-v2.config
    netfilter-base.config
    docker-bridge.config
    docker-overlay2.config
    lxc-base.config
    android-compat.config
    lmkd-psi.config
    debug.config

  android-kernel/
    4.9.config
    4.14.config
    4.19.config
    5.4.config
    5.10.config
    5.15.config

  device/
    sm8250-lmi.config
```

合并方式：

1. 优先使用内核自带 `scripts/kconfig/merge_config.sh`。
2. 如果目标树没有该脚本，项目提供兼容版。
3. 合并后必须执行：

   * `make O=out ARCH=arm64 <defconfig>`
   * `make O=out ARCH=arm64 olddefconfig`
4. 再验证 `out/.config`，而不是只检查 defconfig。

---

## 4. 期望仓库结构

请创建如下项目结构：

```text
android-container-host/
  README.md
  SPEC.md
  LICENSE
  CHANGELOG.md

  achost/
    __init__.py
    cli.py
    kernel_detect.py
    config_merge.py
    patch_apply.py
    verify_config.py
    runtime_probe.py
    report.py

  bin/
    achost

  Kconfig/
    AndroidContainerHost.Kconfig

  config/
    fragments/
      common/
        namespaces.config
        cgroups-v1.config
        cgroups-v2.config
        lxc-base.config
        lxc-unprivileged.config
        docker-base.config
        docker-bridge-net.config
        docker-overlay2.config
        docker-resource-control.config
        docker-ipv6.config
        android-compat.config
        android-ebpf-traffic.config
        android-qtaguid-compat.config
        android-lmkd-protection.config
        debug.config

      kernel-version/
        linux-4.9.config
        linux-4.14.config
        linux-4.19.config
        linux-5.4.config
        linux-5.10.config
        linux-5.15.config

      device/
        xiaomi-sm8250-lmi.config
        generic-arm64.config

  patches/
    README.md

    linux-4.9/
      cgroup-noprefix-compat.patch
      xt-qtaguid-container-safe.patch
      android-paranoid-network-disable.patch
      overlayfs-compat-if-needed.patch

    linux-4.14/
      cgroup-noprefix-compat.patch
      xt-qtaguid-container-safe.patch
      android-paranoid-network-disable.patch

    linux-4.19/
      cgroup-noprefix-compat.patch
      xt-qtaguid-container-safe.patch
      android-paranoid-network-disable.patch
      optional-cgroup-ns-backports.patch
      optional-pidfd-backports.patch

    linux-5.4/
      cgroup-noprefix-compat.patch
      xt-qtaguid-or-ebpf-compat.patch

    linux-5.10/
      android-ebpf-traffic-compat-notes.patch

    linux-5.15/
      android-ebpf-traffic-compat-notes.patch

  profiles/
    lxc-minimal.yml
    lxc-full.yml
    docker-minimal.yml
    docker-full.yml
    docker-bridge-overlay2.yml
    android-container-host-v1.yml
    android-container-host-v2.yml

  devices/
    xiaomi-sm8250-lmi.yml
    generic-arm64.yml
    TEMPLATE.yml

  runtime/
    android/
      README.md

      init/
        init.container-host.rc
        init.container-host.cgroup-v1.rc
        init.container-host.cgroup-v2.rc
        init.container-host.net.rc

      cgroups/
        cgroups.android10plus.json
        task_profiles.android10plus.json
        cgroups.android12plus.json
        task_profiles.android12plus.json

      sysctl/
        99-container-host.conf

      docker/
        daemon.cgroup-v1.json
        daemon.cgroup-v2.json
        daemon.android-default.json

      lxc/
        default.conf
        unprivileged.conf
        android-common.conf
        templates/

      net/
        container-netd-compat.sh
        container-nat-manager.sh
        detect-uplink.sh
        restore-docker-iptables.sh

      memory/
        protect-container-daemons.sh
        oom-score-policy.sh
        lmkd-debug.sh

      sepolicy/
        README.md
        container_host.te.example
        file_contexts.example
        service_contexts.example

  scripts/
    detect-kernel.sh
    prepare-tree.sh
    inject-kconfig.sh
    merge-fragments.sh
    apply-patches.sh
    build-kernel.sh
    verify-config.sh
    verify-moby-check-config.sh
    verify-lxc-checkconfig.sh
    runtime-smoke-docker.sh
    runtime-smoke-lxc.sh
    runtime-net-debug.sh
    collect-logs.sh
    rollback.sh

  third_party/
    README.md
    moby-check-config/
      README.md
      fetch.sh

  docs/
    architecture.md
    porting-guide.md
    device-profile-guide.md
    kernel-config-guide.md
    patches-guide.md
    android-userspace-guide.md
    docker-runtime-guide.md
    lxc-runtime-guide.md
    network-debug-guide.md
    lmkd-memory-guide.md
    security-model.md
    test-matrix.md
    known-issues.md

  ci/
    github-actions/
      build.yml
      lint.yml
      config-check.yml
      patch-check.yml
```

---

## 5. CLI 工具需求

实现一个命令行工具：

```bash
achost
```

推荐 Python 实现，要求可在 Linux 环境运行。

### 5.1 基础命令

```bash
achost detect --kernel-tree /path/to/kernel
```

作用：

1. 检测 kernel 版本。
2. 检测 ARCH。
3. 检测是否 Android kernel。
4. 检测是否 GKI / Non-GKI。
5. 检测 defconfig 候选路径。
6. 检测是否存在 xt_qtaguid。
7. 检测是否存在 Android paranoid network。
8. 检测是否存在 cgroup noprefix 相关代码。
9. 检测是否存在 overlayfs。
10. 检测是否存在 bridge/netfilter/veth。
11. 输出 JSON 报告。

输出示例：

```json
{
  "kernel_version": "4.19.311",
  "arch": "arm64",
  "android_kernel": true,
  "gki": false,
  "defconfig_candidates": [
    "arch/arm64/configs/lmi_defconfig"
  ],
  "has_xt_qtaguid": true,
  "has_android_paranoid_network": true,
  "has_overlayfs": true,
  "has_cgroup_v1": true,
  "has_cgroup_v2": "unknown",
  "recommended_profile": "android-container-host-v1",
  "risk": [
    "xt_qtaguid present; container network may crash if not fixed",
    "cgroup v1 recommended for initial Docker stability"
  ]
}
```

---

```bash
achost plan \
  --kernel-tree /path/to/kernel \
  --defconfig arch/arm64/configs/lmi_defconfig \
  --profile docker-full,lxc-full \
  --android-api 16
```

作用：

1. 根据目标 kernel 生成改造计划。
2. 选择 config fragments。
3. 选择 patches。
4. 选择 Android runtime 文件。
5. 标出不可用功能。
6. 生成 `out/plan.md` 和 `out/plan.json`。

---

```bash
achost apply \
  --kernel-tree /path/to/kernel \
  --defconfig arch/arm64/configs/lmi_defconfig \
  --profile docker-full,lxc-full \
  --device devices/xiaomi-sm8250-lmi.yml
```

作用：

1. 注入 Kconfig。
2. 合并 defconfig fragment。
3. 应用 patch。
4. 生成变更报告。
5. 不直接 commit。
6. 所有 patch 必须可回滚。

---

```bash
achost verify-config \
  --kernel-tree /path/to/kernel \
  --out /path/to/kernel/out \
  --profile docker-full,lxc-full
```

作用：

1. 验证最终 `out/.config`。
2. 调用 Moby check-config.sh。
3. 调用项目自带规则。
4. 输出缺失配置。
5. 把缺失项分成：

   * required
   * recommended
   * optional
   * risky
   * unavailable
6. required 缺失必须返回非零退出码。

---

```bash
achost runtime-install \
  --mode manual|kernelsu-module \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --lxc-asset /path/to/lxc-android-arm64.tgz \
  --lxc-sha256 <sha256> \
  --output out/runtime-package
```

作用：

1. 生成 Android 运行时适配包。
2. 包括 sysctl、dockerd daemon.json、container NAT 管理脚本、LXC 默认配置、OOM/lmkd 保护脚本。
3. manual 模式生成可直接复制到 `/data/adb/achost` 的安装目录。
4. KernelSU 模块模式生成可刷模块结构，并可选 `--start-docker-on-boot`。
5. Docker/LXC 用户态二进制必须由用户显式提供；项目不得静默下载。

---

```bash
achost runtime-test
```

作用：

在手机上运行，检测：

1. cgroup 挂载。
2. namespace 是否可用。
3. Docker daemon 是否正常。
4. docker0 是否存在。
5. veth 是否能创建。
6. bridge 是否能转发。
7. ip_forward 是否开启。
8. NAT/MASQUERADE 是否存在。
9. 容器能否 ping 1.1.1.1。
10. 容器能否解析 DNS。
11. overlay2 是否正常。
12. cgroup memory/cpu limit 是否正常。
13. LXC 是否能启动。
14. lmkd 是否杀掉 dockerd/containerd/runc。
15. dmesg 是否有 kernel panic/oops/warn。

---

## 6. Kconfig 设计需求

新增 Kconfig 文件：

```text
Kconfig/AndroidContainerHost.Kconfig
```

示例内容方向：

```kconfig
menu "Android Container Host Support"

config ANDROID_CONTAINER_HOST
	bool "Android container host support"
	default n
	help
	  Enable grouped kernel features and compatibility helpers needed
	  to run Docker, containerd/runc and LXC directly on Android kernels.

config ANDROID_CONTAINER_LXC
	bool "LXC container support"
	depends on ANDROID_CONTAINER_HOST
	imply NAMESPACES
	imply UTS_NS
	imply IPC_NS
	imply PID_NS
	imply NET_NS
	imply USER_NS
	imply CGROUPS
	imply CGROUP_DEVICE
	imply CGROUP_PIDS
	imply CGROUP_FREEZER
	imply CGROUP_CPUACCT
	imply CGROUP_SCHED
	imply CPUSETS
	imply MEMCG
	imply POSIX_MQUEUE
	imply FHANDLE
	imply SECCOMP
	imply SECCOMP_FILTER
	imply DEVPTS_MULTIPLE_INSTANCES
	imply KEYS
	help
	  Enable core features expected by LXC system containers.

config ANDROID_CONTAINER_LXC_UNPRIVILEGED
	bool "LXC unprivileged container support"
	depends on ANDROID_CONTAINER_LXC
	imply USER_NS
	imply UIDGID_STRICT_TYPE_CHECKS
	help
	  Enable features needed for unprivileged LXC. Userspace still
	  needs newuidmap/newgidmap and uid/gid ranges.

config ANDROID_CONTAINER_DOCKER
	bool "Docker/containerd/runc support"
	depends on ANDROID_CONTAINER_HOST
	imply NAMESPACES
	imply PID_NS
	imply NET_NS
	imply IPC_NS
	imply UTS_NS
	imply CGROUPS
	imply CGROUP_DEVICE
	imply CGROUP_PIDS
	imply CGROUP_FREEZER
	imply CGROUP_CPUACCT
	imply CGROUP_SCHED
	imply MEMCG
	imply POSIX_MQUEUE
	imply FHANDLE
	imply SECCOMP
	imply SECCOMP_FILTER
	imply CHECKPOINT_RESTORE
	help
	  Enable core kernel features required by Docker Engine/containerd/runc.

config ANDROID_CONTAINER_DOCKER_BRIDGE
	bool "Docker bridge/NAT networking"
	depends on ANDROID_CONTAINER_DOCKER
	imply VETH
	imply BRIDGE
	imply BRIDGE_NETFILTER
	imply NETFILTER
	imply NETFILTER_XTABLES
	imply NF_CONNTRACK
	imply NF_NAT
	imply IP_NF_IPTABLES
	imply IP_NF_FILTER
	imply IP_NF_MANGLE
	imply IP_NF_NAT
	imply IP_NF_TARGET_MASQUERADE
	imply NETFILTER_XT_MATCH_ADDRTYPE
	imply NETFILTER_XT_MATCH_CONNTRACK
	imply NETFILTER_XT_MATCH_CGROUP
	imply NETFILTER_XT_MATCH_MULTIPORT
	imply NETFILTER_XT_TARGET_CHECKSUM
	imply NETFILTER_XT_TARGET_MASQUERADE
	imply NETFILTER_XT_TARGET_MARK
	imply NETFILTER_XT_MATCH_MARK
	imply DUMMY
	imply TUN
	imply MACVLAN
	imply IPVLAN
	imply VXLAN
	help
	  Enable veth, bridge, NAT and netfilter pieces needed for Docker's
	  default bridge network.

config ANDROID_CONTAINER_OVERLAY2
	bool "Docker overlay2 storage support"
	depends on ANDROID_CONTAINER_DOCKER
	imply OVERLAY_FS
	imply EXT4_FS_POSIX_ACL
	imply EXT4_FS_SECURITY
	imply TMPFS
	imply TMPFS_XATTR
	imply TMPFS_POSIX_ACL
	help
	  Enable storage features required by Docker overlay2.

config ANDROID_CONTAINER_CGROUP_V1
	bool "Prefer cgroup v1 layout"
	depends on ANDROID_CONTAINER_HOST
	imply CGROUPS
	help
	  Use cgroup v1/cgroupfs-oriented runtime configuration. This is the
	  initial recommended mode for Android 4.9/4.14/4.19 vendor kernels.

config ANDROID_CONTAINER_CGROUP_V2
	bool "Enable cgroup v2 support"
	depends on ANDROID_CONTAINER_HOST
	imply CGROUPS
	imply CGROUP_BPF
	help
	  Enable cgroup v2 where available. Docker supports cgroup v2 with
	  sufficiently recent containerd/runc and kernel versions, but Android
	  userspace integration must be verified.

config ANDROID_CONTAINER_ANDROID_NET_COMPAT
	bool "Android network compatibility helpers"
	depends on ANDROID_CONTAINER_HOST
	imply CGROUP_BPF
	imply BPF
	imply BPF_SYSCALL
	imply NETFILTER_XT_MATCH_BPF
	help
	  Enable Android eBPF traffic-monitoring related pieces where the
	  kernel supports them. Also allows optional qtaguid compatibility patches
	  for older kernels.

config ANDROID_CONTAINER_LMKD_PROTECTION
	bool "Container daemon memory-pressure protection"
	depends on ANDROID_CONTAINER_HOST
	imply MEMCG
	imply PSI
	help
	  Enable kernel pieces useful for lmkd/PSI integration and protecting
	  dockerd/containerd/runc from avoidable low-memory kills.

endmenu
```

要求：

1. Kconfig 不得只为单个机型写死。
2. 不得强制开启 AUFS、Btrfs、Device Mapper，除非用户选择高级 profile。
3. 每个配置项都必须有 help 文本。
4. 每个配置项都必须有对应验证规则。
5. Kconfig 注入方式必须可回滚。

---

## 7. 内核配置最低要求

### 7.1 LXC minimal required

最终 `.config` 至少应满足：

```text
CONFIG_NAMESPACES=y
CONFIG_UTS_NS=y
CONFIG_IPC_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
CONFIG_USER_NS=y
CONFIG_CGROUPS=y
CONFIG_CGROUP_DEVICE=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_FREEZER=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_SCHED=y
CONFIG_CPUSETS=y
CONFIG_MEMCG=y
CONFIG_POSIX_MQUEUE=y
CONFIG_FHANDLE=y
CONFIG_SECCOMP=y
CONFIG_SECCOMP_FILTER=y
CONFIG_DEVPTS_MULTIPLE_INSTANCES=y
CONFIG_KEYS=y
CONFIG_VETH=y
CONFIG_BRIDGE=y
CONFIG_TUN=y
```

### 7.2 Docker minimal required

```text
CONFIG_NAMESPACES=y
CONFIG_UTS_NS=y
CONFIG_IPC_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
CONFIG_CGROUPS=y
CONFIG_CGROUP_DEVICE=y
CONFIG_CGROUP_PIDS=y
CONFIG_CGROUP_FREEZER=y
CONFIG_CGROUP_CPUACCT=y
CONFIG_CGROUP_SCHED=y
CONFIG_MEMCG=y
CONFIG_POSIX_MQUEUE=y
CONFIG_FHANDLE=y
CONFIG_SECCOMP=y
CONFIG_SECCOMP_FILTER=y
CONFIG_KEYS=y
CONFIG_VETH=y
CONFIG_BRIDGE=y
CONFIG_BRIDGE_NETFILTER=y
CONFIG_OVERLAY_FS=y
CONFIG_EXT4_FS_POSIX_ACL=y
CONFIG_EXT4_FS_SECURITY=y
CONFIG_TMPFS_XATTR=y
CONFIG_TMPFS_POSIX_ACL=y
```

### 7.3 Docker bridge network required

```text
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

### 7.4 Android compatibility required

```text
# CONFIG_ANDROID_PARANOID_NETWORK is not set
CONFIG_PSI=y
CONFIG_MEMCG=y
CONFIG_MEMCG_SWAP=y
```

如果目标 Android 版本/内核走 eBPF traffic monitoring，还应检查：

```text
CONFIG_CGROUP_BPF=y
CONFIG_BPF=y
CONFIG_BPF_SYSCALL=y
CONFIG_NETFILTER_XT_MATCH_BPF=y
CONFIG_INET_UDP_DIAG=y
```

参考：

* [https://source.android.com/docs/core/data/ebpf-traffic-monitor](https://source.android.com/docs/core/data/ebpf-traffic-monitor)
* [https://source.android.com/docs/core/data/kernel-overview](https://source.android.com/docs/core/data/kernel-overview)
* [https://source.android.com/docs/core/perf/lmkd](https://source.android.com/docs/core/perf/lmkd)

---

## 8. patch 模块需求

### 8.1 patch 必须模块化

不要一个巨大的 all-in-one patch。

每个 patch 必须有：

```text
patches/<kernel-version>/<name>.patch
patches/<kernel-version>/<name>.md
```

`.md` 文档必须说明：

```text
1. 解决的问题
2. 影响的文件
3. 适用内核版本
4. 是否必需
5. 是否会改变 ABI
6. 是否影响 Android framework
7. 如何验证
8. 如何回滚
9. 已知风险
```

---

### 8.2 cgroup noprefix compatibility patch

目标：

解决 Android cpuset cgroup noprefix 和 runc/LXC 预期文件名不一致的问题。

问题背景：

普通 Linux/runc 常访问：

```text
cpuset.cpus
cpuset.mems
```

部分 Android cgroup noprefix 下可能是：

```text
cpus
mems
```

要求：

1. 实现兼容链接或兼容映射。
2. 不破坏 Android 原有 cpuset 行为。
3. 不直接删除 noprefix。
4. patch 必须只在需要时启用。
5. 添加 runtime probe：

   * 检查 `/sys/fs/cgroup/cpuset/cpuset.cpus`
   * 检查 `/dev/cpuset/cpuset.cpus`
   * 检查 `/dev/cpuset/cpus`
   * 检查 Docker/runc 是否报 cpuset 文件不存在。
6. 如果内核代码结构不同，patch 应能检测失败并输出人工修改建议。

参考：

* [https://github.com/tomxi1997/lxc-docker-support-for-android/blob/main/fix_cgroup.patch](https://github.com/tomxi1997/lxc-docker-support-for-android/blob/main/fix_cgroup.patch)
* [https://github.com/opencontainers/runc/issues/4443](https://github.com/opencontainers/runc/issues/4443)

---

### 8.3 xt_qtaguid container-safe patch

目标：

解决旧 Android 内核 `xt_qtaguid` 与容器 netns/veth/bridge/NAT 混用时可能崩溃、统计异常、引用失效的问题。

要求：

1. 不要简单粗暴把所有 `dev_get_stats()` 永远替换成空统计，除非作为 fallback profile。
2. 优先实现更安全的生命周期检查：

   * `iface_entry`
   * `iface_entry->net_dev`
   * net_device refcount/liveness
   * net namespace
   * device unregister race
3. 如果安全修复难度过高，提供两种 patch：

   * `xt-qtaguid-container-safe.patch`
   * `xt-qtaguid-disable-stats-fallback.patch`
4. fallback patch 必须在文档里明确说明：可能牺牲 Android 流量统计准确性。
5. 检测 Android 版本是否更适合 eBPF traffic monitoring。
6. 对 5.4+ / GKI 倾向 eBPF 方案，不优先修 qtaguid。

参考：

* [https://source.android.com/docs/core/data/kernel-overview](https://source.android.com/docs/core/data/kernel-overview)
* [https://source.android.com/docs/core/data/ebpf-traffic-monitor](https://source.android.com/docs/core/data/ebpf-traffic-monitor)
* [https://github.com/tomxi1997/lxc-docker-support-for-android/blob/main/xt_qtaguid.patch](https://github.com/tomxi1997/lxc-docker-support-for-android/blob/main/xt_qtaguid.patch)

---

### 8.4 Android paranoid network patch/config

目标：

确保非 Android app UID 的 Linux userspace/container 进程能正常访问网络。

要求：

1. 优先通过 defconfig fragment 禁用：

   ```text
   # CONFIG_ANDROID_PARANOID_NETWORK is not set
   ```
2. 如果目标内核实现差异，需要检测对应代码。
3. 必须验证容器内非 Android app UID 能访问网络。

---

### 8.5 overlayfs/overlay2 compatibility

目标：

Docker overlay2 可用。

要求：

1. 检查最终 `.config`：

   ```text
   CONFIG_OVERLAY_FS=y
   CONFIG_EXT4_FS_POSIX_ACL=y
   CONFIG_EXT4_FS_SECURITY=y
   CONFIG_TMPFS_XATTR=y
   CONFIG_TMPFS_POSIX_ACL=y
   ```
2. 运行时检查：

   ```bash
   docker info | grep -i 'Storage Driver'
   docker run --rm alpine sh -c 'echo ok > /tmp/x && cat /tmp/x'
   ```
3. 检查 backing filesystem 是否支持 d_type/xattr。
4. 如果 Android data 分区文件系统不适合 overlay2，报告：

   * 当前 backing fs
   * 当前 mount options
   * Docker root dir
   * 建议目录
5. 不要默认启用 AUFS。

---

### 8.6 cgroup v1/v2 strategy

初始目标内核为 4.19 Android vendor kernel，默认优先 cgroup v1 + cgroupfs。

要求：

1. Docker cgroup v1 配置：

   ```json
   {
     "exec-opts": ["native.cgroupdriver=cgroupfs"]
   }
   ```
2. cgroup v2 作为可选高级 profile。
3. 检测方式：

   ```bash
   test -e /sys/fs/cgroup/cgroup.controllers && echo v2 || echo v1
   grep cgroup /proc/mounts
   cat /proc/cgroups
   ```
4. 如果用户选择 cgroup v2，必须验证：

   * kernel >= 4.15
   * containerd >= 1.4
   * runc >= rc91 或稳定版
   * dockerd >= 20.10
5. Android 4.19 上不默认启用 cgroup v2，除非用户明确选择。

参考：

* [https://docs.docker.com/engine/containers/runmetrics/](https://docs.docker.com/engine/containers/runmetrics/)
* [https://docs.kernel.org/admin-guide/cgroup-v2.html](https://docs.kernel.org/admin-guide/cgroup-v2.html)

---

## 9. Android userspace 适配需求

仅修改内核不够。项目必须提供 runtime 层。

### 9.1 runtime 模式

支持三种模式：

```text
1. manual
   只输出脚本，用户手动执行。

2. magisk-module / kernelsu-module
   生成可刷模块，不要求修改 ROM。

3. rom-integrated
   生成 init.rc、cgroups.json、task_profiles.json、sepolicy 示例，供 ROM 维护者集成。
```

---

### 9.2 cgroup 挂载策略

Android 10+ 推荐使用：

```text
/system/etc/task_profiles/cgroups_<API>.json
/system/etc/task_profiles/task_profiles_<API>.json
/vendor/etc/cgroups.json
/vendor/etc/task_profiles.json
```

而不是随便在 init.rc 手动挂载。

参考：

* [https://source.android.com/docs/core/perf/cgroups](https://source.android.com/docs/core/perf/cgroups)

要求：

1. Android 9 及以下：允许 init.rc 兼容方案。
2. Android 10 及以上：优先生成 cgroups.json/task_profiles.json。
3. Magisk/KernelSU 模式：如果不能修改系统文件，则运行时挂载必须尽量不破坏 Android framework 的 cgroup。
4. 必须生成 cgroup layout 报告：

   ```bash
   grep cgroup /proc/mounts
   cat /proc/cgroups
   find /sys/fs/cgroup -maxdepth 3 -type d
   find /dev -maxdepth 2 -name '*cgroup*' -o -name '*cpuset*'
   ```

---

### 9.3 Docker daemon 配置

为 Android cgroup v1 默认提供：

```json
{
  "exec-opts": ["native.cgroupdriver=cgroupfs"],
  "storage-driver": "overlay2",
  "iptables": true,
  "ip-forward": true,
  "ip-masq": true,
  "bridge": "docker0",
  "bip": "172.31.0.1/16",
  "default-address-pools": [
    {
      "base": "172.30.0.0/16",
      "size": 24
    }
  ],
  "log-level": "debug"
}
```

要求：

1. 不默认使用 systemd cgroup driver。
2. 避免默认 172.17.0.0/16 与 Android/VPN/热点网络冲突。
3. 允许用户自定义网段。
4. 允许禁用 Docker 自己改 iptables，然后用项目 NAT manager 管理。
5. 输出最终 `docker info` 报告。

---

### 9.4 Docker bridge 网络修复器

这是重点。用户之前遇到的问题是：

```text
Docker host 网络能用，但 bridge 不能访问外网。
```

项目必须提供一个独立脚本：

```text
runtime/android/net/container-nat-manager.sh
```

功能：

1. 自动检测外网出口网卡：

   ```bash
   ip route get 1.1.1.1
   ```
2. 自动检测 docker bridge：

   ```bash
   ip addr show docker0
   ```
3. 开启：

   ```bash
   sysctl -w net.ipv4.ip_forward=1
   sysctl -w net.ipv6.conf.all.forwarding=1
   ```
4. 添加或修复 iptables：

   ```bash
   iptables -I FORWARD -i docker0 -o "$UPLINK" -j ACCEPT
   iptables -I FORWARD -o docker0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
   iptables -t nat -C POSTROUTING -s "$DOCKER_SUBNET" ! -o docker0 -j MASQUERADE || \
   iptables -t nat -A POSTROUTING -s "$DOCKER_SUBNET" ! -o docker0 -j MASQUERADE
   ```
5. 检测 legacy iptables / nft iptables。
6. 处理 Android netd 可能重写 iptables 的情况：

   * 开机后执行一次
   * 网络切换后执行一次
   * dockerd 重启后执行一次
   * 可选 watchdog 周期性 reconcile
7. 输出 debug：

   ```bash
   ip route
   ip rule
   ip addr
   iptables -S
   iptables -t nat -S
   iptables -L FORWARD -n -v
   iptables -t nat -L POSTROUTING -n -v
   conntrack -L 2>/dev/null
   ```
8. 支持 Wi-Fi：

   ```text
   wlan0
   ```
9. 支持移动网络：

   ```text
   rmnet_data0
   rmnet_data1
   ccmni0
   ```
10. 支持 VPN 场景，但必须报告风险。

#### 9.4.1 KernelSU 模块网络守护

KernelSU / ReSukiSU 模块模式必须提供一个常驻但轻量的网络守护，用来弥补 Android `netd` 和网络切换对容器 bridge NAT 的影响。该守护不负责分发 Docker/LXC 二进制，也不负责启动 `dockerd`；它只负责在容器 bridge 出现后持续修复网络。

目标：

1. Docker/LXC bridge 流量必须跟随当前 Android 默认出口。
2. Wi-Fi 出口通常是 `wlan0`，移动网络出口可能是 `rmnet_data0`、`rmnet_data1`、`ccmni0` 等。
3. Wi-Fi 切移动网络、移动网络切 Wi-Fi 后，容器出网应在守护轮询周期内自动恢复。
4. `dockerd` 重启、`docker0` 重建、Android `netd` 重写 iptables 后，应通过幂等 reconcile 自动恢复。
5. LXC 第一阶段可以复用 `docker0`，后续再扩展 `lxcbr0`。

实现要求：

1. 新增守护脚本：

   ```text
   runtime/android/net/container-network-watchdog.sh
   ```

2. 守护脚本调用 `container-nat-manager.sh`，不能复制另一套 iptables 逻辑。
3. `container-nat-manager.sh` 继续作为一次性幂等修复器，可手动执行，也可被守护循环执行。
4. 变量约定：

   ```text
   CONTAINER_BRIDGE      默认 docker0，优先级高于 DOCKER_BRIDGE
   DOCKER_BRIDGE         legacy alias
   DOCKER_SUBNET         可选，未设置时从 bridge IPv4 自动检测
   UPLINK                可选，未设置时自动检测
   TARGET                默认 1.1.1.1，用于 ip route get
   IPTABLES              可选，指定 iptables/iptables-legacy/iptables-nft
   ACHOST_NET_WATCH_INTERVAL 默认 5 秒
   ACHOST_NET_REPAIR_INTERVAL 默认 30 秒，状态不变时的周期性 reconcile 间隔
   ACHOST_NET_LOG        默认 /data/local/tmp/achost-network-watchdog.log
   ACHOST_DRY_RUN        dry-run 模式
   ```

5. KernelSU 模块 `service.sh` 必须后台启动守护；`post-fs-data.sh` 只做 sysctl 等早期初始化。
6. 守护不得 flush Android 原生 iptables 表，不得删除 `netd` 链，只能确保 ACHOST 自己需要的规则存在。
7. 守护必须记录当前 bridge、subnet、uplink、iptables 实现和最近一次 reconcile 结果。
8. `runtime-net-debug.sh` 和 `collect-logs.sh` 必须收集守护日志，方便判断是否因为网络切换、netd 重写或 bridge 缺失导致容器断网。

验收标准：

1. 安装 KernelSU 模块后，即使 Docker/LXC 尚未安装，守护也应稳定等待 bridge，不应刷屏或退出。
2. `docker0` 出现后自动写入/修复 FORWARD 与 MASQUERADE 规则。
3. 从 Wi-Fi 切移动网络后，守护检测到 uplink 改变并重新 reconcile。
4. 从移动网络切 Wi-Fi 后同样恢复。
5. Docker smoke test 和 LXC smoke test 能通过 bridge 网络访问外网。
6. 不以 `/proc/config.gz` 作为真实性校验来源；该项目允许内嵌 config 保持原厂伪装，真实能力以运行时行为和 `out/.config` 为准。

非目标：

1. 本阶段不捆绑 Docker、containerd、runc、LXC 用户态二进制。
2. 本阶段不默认要求 SELinux permissive。
3. 本阶段不实现侵入式 netd hook。
4. 本阶段不创建或管理完整 DHCP/DNS 服务；Docker 默认 bridge 和后续 LXC 配置负责容器侧地址分配。

#### 9.4.2 Docker/LXC 用户态资产集成

运行时包必须支持显式用户态资产输入，但不得内置或静默下载未知二进制。Docker 优先支持官方 static Linux arm64 tarball；LXC 只提供资产槽位和验证路径，直到选定 Android 兼容 userspace。

新增命令参数：

```text
--docker-asset PATH
--docker-sha256 SHA256
--lxc-asset PATH
--lxc-sha256 SHA256
--start-docker-on-boot
```

Docker 资产要求：

1. `--docker-asset` 指向本地 tar/tgz 文件。
2. 若提供 `--docker-sha256`，生成包前必须校验哈希。
3. 至少提取这些可执行文件到 `achost/bin/`：

   ```text
   docker
   dockerd
   containerd
   containerd-shim-runc-v2
   ctr
   runc
   ```

4. 可选提取 `containerd-shim`、`docker-init`、`docker-proxy`、`containerd-stress`。
5. 如果同一个 Docker tarball 已自带 Compose v2 CLI 插件，可被动提取为 `docker compose` 可发现的插件；不得为 Compose 增加静默下载流程。
6. `manifest.json` 必须记录资产来源、sha256、提取文件和文件可执行状态。

生成包布局：

```text
achost/
  bin/
    docker
    dockerd
    containerd
    containerd-shim-runc-v2
    ctr
    runc
    achost-container-env.sh
    achost-container-validate.sh
    achost-docker-start.sh
    achost-docker-stop.sh
    achost-lxc-validate.sh
  etc/docker/daemon.json
  etc/docker/cli-plugins/docker-compose  # only when present in the Docker asset
  etc/lxc/
  var/docker/
  var/run/
  var/log/
  var/containerd/
```

Android 侧启动要求：

1. `achost-container-env.sh` 统一设置 `PATH`、`DOCKER_HOST`、containerd socket、data-root、exec-root、log 路径。
2. `achost-container-validate.sh` 只做 presence/路径/内核运行态检查，不启动容器。
3. `achost-docker-start.sh` 必须以 root 运行，创建 ACHOST 目录，启动 containerd 和 dockerd，并触发网络守护。
4. Docker 29 等新版本若硬编码访问 `/run`，启动脚本必须优先使用 `/data/adb/achost/var/chroot` 提供可写 `/run`，不得 remount Android 根目录。
5. `achost-docker-stop.sh` 只停止 ACHOST pidfile 记录的 dockerd/containerd，并清理 ACHOST chroot bind mounts。
6. `runtime-test.sh`、`runtime-net-debug.sh`、`collect-logs.sh` 必须收集 validation、socket、daemon 日志和 watchdog 状态。

LXC 策略：

1. 不把任意发行版 LXC 二进制当作已验证资产。
2. 允许 `achost/lxc/bin`、`achost/lxc/lib`、`achost/lxc/share` 和 `achost/etc/lxc` 布局。
3. `achost-lxc-validate.sh` 只报告命令、配置、cgroup/bridge 前置条件；真正 LXC smoke 仍作为后续实机验证。

---

### 9.5 LXC 默认配置

提供：

```text
runtime/android/lxc/android-common.conf
runtime/android/lxc/default.conf
runtime/android/lxc/unprivileged.conf
```

要求支持：

1. privileged LXC。
2. 尽可能支持 unprivileged LXC。
3. veth + bridge。
4. devpts newinstance。
5. proc/sysfs 挂载。
6. cgroup 挂载。
7. DNS。
8. system container 基础能力。

示例方向：

```text
lxc.include = /path/to/android-common.conf
lxc.net.0.type = veth
lxc.net.0.link = docker0 或 lxcbr0
lxc.net.0.flags = up
lxc.apparmor.profile = unconfined
lxc.cap.drop =
```

注意：

1. Android 没有标准 systemd host 环境。
2. 不要假设 `/etc/subuid`、`/etc/subgid` 存在。
3. unprivileged LXC 需要 newuidmap/newgidmap 或替代机制。
4. 如果某项不可用，必须在 `lxc-check` 报告中说明。

参考：

* [https://github.com/lxc/lxc](https://github.com/lxc/lxc)
* [https://linuxcontainers.org/](https://linuxcontainers.org/)
* [https://linuxcontainers.org/lxc/security/](https://linuxcontainers.org/lxc/security/)

---

### 9.6 lmkd / OOM 保护

Android 的 lmkd 会在内存压力下杀进程。项目必须提供容器守护进程保护方案。

参考：

* [https://source.android.com/docs/core/perf/lmkd](https://source.android.com/docs/core/perf/lmkd)

要求：

1. 检测：

   ```bash
   logcat -b all | grep -iE 'lmkd|lowmemory|kill.*dockerd|kill.*containerd|kill.*runc'
   dmesg | grep -iE 'oom|killed process|out of memory|lowmemory'
   ```
2. 对关键进程设置较低 `oom_score_adj`：

   ```bash
   echo -900 > /proc/$(pidof dockerd)/oom_score_adj
   echo -900 > /proc/$(pidof containerd)/oom_score_adj
   ```
3. 对 containerd-shim 是否保护要可配置：

   * 默认不对所有容器进程设 -900，避免系统被容器拖死。
   * 只保护 dockerd/containerd。
4. 检查 kernel：

   ```text
   CONFIG_PSI=y
   CONFIG_MEMCG=y
   CONFIG_MEMCG_SWAP=y
   ```
5. 检查 Android 属性：

   ```bash
   getprop ro.lmk.use_psi
   getprop ro.lmk.debug
   ```
6. 提供压力测试：

   ```bash
   docker run --rm --memory=128m alpine sh -c '...'
   ```
7. 如果 lmkd 杀掉容器，报告：

   * 被杀进程
   * oom_score_adj
   * cgroup 路径
   * 内存压力日志
   * 建议策略

---

### 9.7 SELinux 策略

项目不应默认要求 SELinux permissive。

要求：

1. 先支持 permissive 作为 debug 模式。
2. 正式目标是 enforcing 下可用。
3. 提供 sepolicy 示例，但不要声称适用于所有 ROM。
4. 收集 avc：

   ```bash
   dmesg | grep -i avc
   logcat -b all | grep -i avc
   ```
5. 生成 `audit2allow` 风格提示，但不要自动无脑放权。
6. 文档必须说明：

   * Docker daemon socket 权限风险
   * 容器 root 风险
   * user namespace 风险
   * bridge/NAT 暴露风险

---

## 10. 验证系统需求

### 10.1 build-time 验证

必须提供：

```bash
scripts/verify-config.sh
```

检查输入：

```bash
scripts/verify-config.sh out/.config --profile docker-full,lxc-full
```

输出：

```text
[OK] CONFIG_NAMESPACES=y
[OK] CONFIG_PID_NS=y
[FAIL][required] CONFIG_NET_NS is missing
[WARN][recommended] CONFIG_CGROUP_BPF is missing
[INFO][optional] CONFIG_IPVLAN is missing
```

必须支持 JSON 输出：

```bash
scripts/verify-config.sh out/.config --json > report.json
```

---

### 10.2 集成 Moby check-config

项目必须能下载或引用 Docker/Moby 官方 check-config：

参考：

* [https://github.com/moby/moby/blob/master/contrib/check-config.sh](https://github.com/moby/moby/blob/master/contrib/check-config.sh)
* [https://raw.githubusercontent.com/moby/moby/master/contrib/check-config.sh](https://raw.githubusercontent.com/moby/moby/master/contrib/check-config.sh)

命令：

```bash
scripts/docker/verify-moby-check-config.sh out/.config
```

要求：

1. 不仅运行它，还要解析结果。
2. 把结果归类为：

   * Docker required
   * Docker recommended
   * Android-specific missing
   * ignored because Android
3. 如果 Moby 脚本和项目规则冲突，项目报告必须解释原因。

---

### 10.3 LXC check

提供：

```bash
scripts/verify-lxc-checkconfig.sh
```

如果目标设备有 `lxc-checkconfig`，调用它。否则用自定义检查替代。

检查：

```text
namespaces
cgroups
user namespace
network namespace
veth
bridge
devpts
seccomp
apparmor/selinux state
```

---

### 10.4 runtime Docker smoke tests

提供：

```bash
scripts/docker/runtime-smoke-docker.sh
```

默认安全 smoke 不依赖 Docker Hub：

```bash
docker version
docker info
docker info | grep -i 'Storage Driver: overlay2'
# script builds a tiny local rootfs from the packaged Docker binary
docker import /data/local/tmp/achost-local-rootfs-*.tar achost-local-smoke:<stamp>
docker run --rm --network none achost-local-smoke:<stamp> /bin/docker --version
docker rmi achost-local-smoke:<stamp>
```

如果需要让本地镜像创建 bridge/veth，但仍不测试外网访问：

```bash
DOCKER_SMOKE_MODE=local-bridge scripts/docker/runtime-smoke-docker.sh
```

只有在 Docker Hub/registry 可访问时才运行完整拉取/网络 smoke：

```bash
DOCKER_SMOKE_MODE=full scripts/docker/runtime-smoke-docker.sh
```

完整 smoke 覆盖：

```bash
docker run --rm hello-world
docker run --rm busybox uname -a
docker run --rm busybox ping -c 3 1.1.1.1
docker run --rm busybox nslookup google.com
docker run --rm --network host busybox ping -c 3 1.1.1.1
docker run --rm --network bridge busybox ping -c 3 1.1.1.1
docker run --rm -m 128m busybox true
docker run --rm --cpus=0.5 busybox true
docker run --rm -v /data/local/tmp:/mnt busybox sh -c 'echo ok > /mnt/docker-volume-test'
docker run -d --name achost-nginx -p 18080:80 nginx:alpine
curl http://127.0.0.1:18080
docker rm -f achost-nginx
```

必须记录 bridge 规则：

```bash
ip addr show docker0
iptables -t nat -S
iptables -S FORWARD
```

---

### 10.5 runtime LXC smoke tests

提供：

```bash
scripts/runtime-smoke-lxc.sh
```

测试：

```bash
lxc-checkconfig
lxc-create -n achost-alpine -t download -- -d alpine -r edge -a arm64
lxc-start -n achost-alpine -d
lxc-info -n achost-alpine
lxc-attach -n achost-alpine -- uname -a
lxc-attach -n achost-alpine -- ping -c 3 1.1.1.1
lxc-stop -n achost-alpine
```

如果 download template 不可用，允许使用本地 rootfs。

必须检查：

```bash
dmesg | tail -200
logcat -b kernel -d | tail -200
```

---

## 11. Debug 和日志收集

提供：

```bash
scripts/collect-logs.sh
```

收集：

```text
uname -a
cat /proc/version
zcat /proc/config.gz
cat /proc/cmdline
mount
cat /proc/mounts
grep cgroup /proc/mounts
cat /proc/cgroups
find /sys/fs/cgroup -maxdepth 4
find /dev -maxdepth 3 -name '*cgroup*' -o -name '*cpuset*'
ip addr
ip route
ip rule
iptables -S
iptables -t nat -S
iptables -t mangle -S
sysctl net.ipv4.ip_forward
sysctl net.ipv6.conf.all.forwarding
docker info
docker version
docker ps -a
docker network inspect bridge
lxc-checkconfig
lxc-ls -f
dmesg
logcat -b all -d
getprop
getenforce
```

输出压缩包：

```text
achost-debug-<device>-<date>.tar.gz
```

---

## 12. 设备 profile 设计

设备配置文件示例：

```yaml
id: xiaomi-sm8250-lmi
name: Xiaomi Mi 10 Pro / POCO F2 Pro family lmi
arch: arm64
kernel:
  repo: https://github.com/crdroidandroid/android_kernel_xiaomi_sm8250
  branch: 16.0-lmi
  version_hint: "4.19"
  defconfig: arch/arm64/configs/lmi_defconfig
  image: Image.gz-dtb
android:
  api_level: 16
  rom_hint: crDroid
container:
  default_cgroup_mode: v1
  profiles:
    - lxc-full
    - docker-full
    - docker-bridge-net
    - docker-overlay2
network:
  docker_bridge: docker0
  docker_subnet: 172.31.0.0/16
  default_address_pool: 172.30.0.0/16
  uplink_auto_detect: true
patches:
  cgroup_noprefix_compat: auto
  xt_qtaguid_container_safe: auto
  android_paranoid_network: config
runtime:
  mode: kernelsu-module
  protect_daemons_from_lmkd: true
  selinux_mode: detect
```

要求：

1. 新增设备只需添加 YAML，不改核心代码。
2. device profile 可覆盖默认 fragment。
3. device profile 可声明禁用某个 patch。
4. device profile 可声明 known issues。

---

## 13. 可移植性要求

项目必须支持这些 kernel family：

```text
linux-4.9 android vendor kernel
linux-4.14 android vendor kernel
linux-4.19 android vendor kernel
linux-5.4 android vendor kernel
linux-5.10 android / GKI-adjacent kernel
linux-5.15 android / GKI-adjacent kernel
```

每个 kernel family 至少有：

```text
1. config fragment
2. patch compatibility notes
3. known missing features
4. recommended cgroup mode
5. qtaguid/eBPF strategy
```

建议默认策略：

```text
4.9:
  cgroup: v1
  qtaguid: likely present
  ebpf traffic: limited/backport dependent
  docker: possible, but patch-heavy

4.14:
  cgroup: v1
  qtaguid: likely present
  ebpf traffic: possible on Android common backports
  docker: possible

4.19:
  cgroup: v1 first, v2 optional
  qtaguid/eBPF: device dependent
  docker: main target

5.4:
  cgroup: v1 or v2
  ebpf: preferred where Android stack supports it
  docker: good target

5.10+:
  cgroup: v2 optional
  ebpf: preferred
  qtaguid: avoid if possible
  docker: good target, but GKI/KMI constraints matter
```

---

## 14. 成功标准

### 14.1 第一阶段成功标准：Docker bridge 修通

在目标手机上：

```bash
docker run --rm --network bridge busybox ping -c 3 1.1.1.1
docker run --rm --network bridge busybox nslookup google.com
```

必须成功。

并且：

```bash
docker run --rm --network host busybox ping -c 3 1.1.1.1
```

也成功。

如果 host 成功但 bridge 失败，报告必须明确定位到：

```text
1. ip_forward
2. docker0
3. veth
4. FORWARD chain
5. nat POSTROUTING MASQUERADE
6. conntrack
7. netd/iptables 重写
8. SELinux
9. qtaguid/eBPF
```

---

### 14.2 第二阶段成功标准：Docker 基础功能

必须成功：

```bash
docker run --rm hello-world
docker run --rm busybox uname -a
docker run --rm busybox sh -c 'echo ok'
docker run --rm -m 128m busybox true
docker run --rm --cpus=0.5 busybox true
docker info | grep -i 'Storage Driver: overlay2'
```

---

### 14.3 第三阶段成功标准：LXC 基础功能

必须成功：

```bash
lxc-checkconfig
lxc-create
lxc-start
lxc-attach
lxc-stop
```

容器内必须能：

```bash
uname -a
ip addr
ping -c 3 1.1.1.1
```

---

### 14.4 第四阶段成功标准：稳定性

连续运行：

```bash
docker run -d --name achost-stress nginx:alpine
docker run --rm busybox sh -c 'for i in $(seq 1 100); do wget -qO- http://1.1.1.1 >/dev/null || exit 1; done'
```

至少观察：

```text
1. 无 kernel panic
2. 无 xt_qtaguid oops
3. 无 netfilter use-after-free
4. dockerd/containerd 未被 lmkd 杀
5. iptables 规则未被 netd 永久破坏
6. overlay2 未损坏
```

---

## 15. 安全要求

项目必须在文档中明确说明：

1. 开启 USER_NS、NET_NS、bridge、veth、Docker daemon 会扩大攻击面。
2. Docker daemon socket 等同高权限控制点。
3. 不要把 Docker TCP API 暴露到公网。
4. 不要默认 SELinux permissive。
5. 不要默认给所有容器进程 `oom_score_adj=-1000`。
6. 不要默认允许容器访问 Android 私有目录。
7. 所有危险 profile 必须显式启用。

文档：

```text
docs/security-model.md
```

必须包含：

```text
risk matrix
recommended default
debug-only setting
production/self-use warning
```

---

## 16. 文档要求

至少完成：

```text
docs/architecture.md
docs/porting-guide.md
docs/device-profile-guide.md
docs/kernel-config-guide.md
docs/patches-guide.md
docs/android-userspace-guide.md
docs/docker-runtime-guide.md
docs/lxc-runtime-guide.md
docs/network-debug-guide.md
docs/lmkd-memory-guide.md
docs/security-model.md
docs/test-matrix.md
docs/known-issues.md
```

每篇文档必须有：

```text
1. 问题
2. 原理
3. 实现方式
4. 验证方式
5. 失败排查
6. 参考链接
```

---

## 17. 参考资料

### Existing Android LXC/Docker projects

```text
https://github.com/tomxi1997/lxc-docker-support-for-android
https://github.com/tomxi1997/LXC_KernelSU_Action
https://github.com/grilix/kernel-docker-support
https://gist.github.com/FreddieOliveira/efe850df7ff3951cb62d74bd770dce27
```

### Target kernel

```text
https://github.com/crdroidandroid/android_kernel_xiaomi_sm8250/tree/16.0-lmi
```

### Docker / Moby

```text
https://github.com/moby/moby/blob/master/contrib/check-config.sh
https://raw.githubusercontent.com/moby/moby/master/contrib/check-config.sh
https://docs.docker.com/engine/network/drivers/bridge/
https://docs.docker.com/engine/containers/runmetrics/
https://docs.docker.com/reference/cli/dockerd/
https://docs.docker.com/engine/security/
https://docs.docker.com/engine/containers/resource_constraints/
```

### LXC / Linux Containers

```text
https://linuxcontainers.org/
https://github.com/lxc/lxc
https://linuxcontainers.org/lxc/security/
https://linuxcontainers.org/lxc/manpages/
```

### Linux kernel docs

```text
https://docs.kernel.org/kbuild/kconfig-language.html
https://docs.kernel.org/admin-guide/cgroup-v2.html
https://www.kernel.org/doc/Documentation/admin-guide/cgroup-v1/
https://docs.kernel.org/networking/bridge.html
https://docs.kernel.org/filesystems/overlayfs.html
```

### Android / AOSP

```text
https://source.android.com/docs/core/perf/cgroups
https://source.android.com/docs/core/perf/lmkd
https://source.android.com/docs/core/data/kernel-overview
https://source.android.com/docs/core/data/ebpf-traffic-monitor
https://source.android.com/docs/core/architecture/hidl/network-stack
```

### OCI / runtime

```text
https://github.com/opencontainers/runtime-spec
https://github.com/opencontainers/runc
https://github.com/containerd/containerd
```

---

## 18. CLI AI 执行顺序建议

请按这个顺序实现，不要一开始就写所有东西。

### Step 1：建立项目骨架

创建目录、README、SPEC、基础 Python CLI。

交付：

```text
achost detect
achost plan
```

---

### Step 2：实现 kernel detect

实现：

```text
kernel version detection
Android kernel detection
defconfig detection
qtaguid detection
overlayfs detection
cgroup detection
netfilter detection
```

---

### Step 3：实现 config fragments 和 verify-config

先不打 patch，只做配置合并和验证。

交付：

```text
config/fragments/common/*.config
scripts/merge-fragments.sh
scripts/verify-config.sh
```

---

### Step 4：集成 Moby check-config

交付：

```text
third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh
```

---

### Step 5：实现 patch framework

先支持 dry-run：

```bash
git apply --check
```

再支持 apply。

交付：

```text
scripts/apply-patches.sh
patches/linux-4.19/cgroup-noprefix-compat.patch
patches/linux-4.19/xt-qtaguid-container-safe.patch
```

---

### Step 6：实现 Android runtime 网络修复器

优先解决：

```text
host 网络能用但 bridge 不能出网
```

交付：

```text
runtime/android/net/container-nat-manager.sh
scripts/runtime-net-debug.sh
```

---

### Step 7：实现 Docker runtime smoke test

交付：

```text
scripts/docker/runtime-smoke-docker.sh
```

---

### Step 8：实现 LXC runtime smoke test

交付：

```text
scripts/runtime-smoke-lxc.sh
```

---

### Step 9：实现 lmkd/OOM 保护

交付：

```text
runtime/android/memory/protect-container-daemons.sh
docs/lmkd-memory-guide.md
```

---

### Step 10：做第一个设备 profile

目标：

```text
devices/xiaomi-sm8250-lmi.yml
```

基于：

```text
https://github.com/crdroidandroid/android_kernel_xiaomi_sm8250/tree/16.0-lmi
```

---

## 19. 第一目标设备的具体要求：xiaomi-sm8250-lmi

针对初始内核：

```text
repo: https://github.com/crdroidandroid/android_kernel_xiaomi_sm8250
branch: 16.0-lmi
defconfig: arch/arm64/configs/lmi_defconfig
kernel: 4.19.x
```

要求：

1. 不假设 KernelSU 必须存在。
2. 不依赖 Action。
3. 项目脚本必须能在本地 kernel tree 上执行。
4. 先生成 patch，不直接 push。
5. 先实现 cgroup v1。
6. 先修 Docker bridge 出网。
7. 先保证 Docker overlay2。
8. 再做 LXC。
9. 最后再考虑 cgroup v2、rootless Docker、高级网络。

---

## 20. 必须避免的错误

1. 不要只检查 defconfig，要检查最终 `out/.config`。
2. 不要把所有配置写进一个 `CONFIG_DOCKER`。
3. 不要无脑 `select`。
4. 不要默认开启所有存储后端。
5. 不要忽略 Android netd 会改 iptables。
6. 不要把 host network 能用误判成 Docker 网络完全正常。
7. 不要忽略 lmkd/OOM。
8. 不要默认 SELinux permissive。
9. 不要默认牺牲 qtaguid 统计，除非 fallback。
10. 不要把 Xiaomi lmi 写死进核心逻辑。
11. 不要把 KernelSU 和 container host 能力绑定。
12. 不要做不可回滚修改。

---

## 21. 最终交付物

项目完成时应能做到：

```bash
# 在任意目标内核外部运行
achost detect --kernel-tree ~/kernel

achost plan \
  --kernel-tree ~/kernel \
  --defconfig arch/arm64/configs/lmi_defconfig \
  --profile docker-full,lxc-full \
  --device devices/xiaomi-sm8250-lmi.yml

achost apply \
  --kernel-tree ~/kernel \
  --defconfig arch/arm64/configs/lmi_defconfig \
  --profile docker-full,lxc-full \
  --device devices/xiaomi-sm8250-lmi.yml

# 用户自己编译内核
make O=out ARCH=arm64 lmi_defconfig
make O=out ARCH=arm64 olddefconfig

# 验证配置
achost verify-config \
  --kernel-tree ~/kernel \
  --out ~/kernel/out \
  --profile docker-full,lxc-full

# 生成 Android 运行时包
achost runtime-install \
  --mode kernelsu-module \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/runtime-package

# 手机上验证
achost runtime-test
```

最终报告必须包含：

```text
1. build-time config report
2. applied patches report
3. runtime cgroup report
4. Docker report
5. LXC report
6. network bridge/NAT report
7. lmkd/OOM report
8. SELinux report
9. known limitations
10. next recommended fixes
```

---

## 22. 质量门槛

PR 合并前必须通过：

```text
1. shellcheck scripts/*.sh
2. Python unit tests
3. YAML schema validation
4. patch dry-run test
5. config merge test
6. verify-config test
7. docs existence test
```

对于无法在 CI 真机测试的部分，必须提供：

```text
mock / sample outputs
expected parser behavior
manual test checklist
```

---

## 23. 一句话总结

本项目不是“给 Android 内核加一个 Docker 开关”。

本项目要做的是：

把 Docker/LXC 所依赖的 Linux primitives、Android vendor kernel 差异补丁、Android userspace cgroup/netd/lmkd/SELinux 适配、运行时验证脚本，抽象成一个可移植的 Android Container Host 支持层。

要求它能先服务于 xiaomi-sm8250-lmi / crDroid 16.0-lmi / Linux 4.19 内核，但最终可以通过 device profile、kernel-version profile、config fragments、patch modules 迁移到其他 Android 手机内核。