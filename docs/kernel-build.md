# 编译自己的内核以支持 ACHost

ACHost 不能替代内核能力。要稳定运行 Docker/LXC，必须先让目标内核具备 namespace、cgroup、overlayfs、bridge/veth、netfilter 和 Android root 环境所需能力。

本文是通用流程；具体机型的 defconfig、toolchain、AnyKernel3 打包和刷入步骤应以目标内核仓库文档为准。

## 总流程

1. 取得目标设备内核源码。
2. 生成最终 `.config`。
3. 用 ACHost profile 检查 `.config`。
4. 编译内核和模块。
5. 用 AnyKernel3 或设备项目推荐方式打包。
6. 刷入后安装 ACHost split 模块。
7. 在设备上跑 `runtime-test.sh` 和手动 smoke。

不要只看 defconfig。不要只信 `/proc/config.gz`。有些内核会让 `/proc/config.gz` 看起来像原厂配置，必须用运行时行为验证。

## PC：检查源码和最终配置

在 ACHost 仓库根目录：

```bash
bin/achost detect \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out
```

如果已经有最终 `.config`：

```bash
bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1
```

Docker 最小路径可先检查：

```bash
bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile docker-bridge-overlay2
```

Moby 参考检查：

```bash
third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh /path/to/android_kernel/out/.config
```

Moby check-config 是 Linux Docker 参考，不理解所有 Android 差异；它只能辅助发现明显缺项，最终以设备 runtime-test 为准。

## 关键内核能力

### Namespace

```text
CONFIG_NAMESPACES=y
CONFIG_UTS_NS=y
CONFIG_IPC_NS=y
CONFIG_PID_NS=y
CONFIG_NET_NS=y
```

`CONFIG_USER_NS` 对部分 LXC/rootless 场景有帮助，但 Docker native 基础路径不应依赖 rootless。

### cgroup v1 baseline

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

设备如果默认只有 cgroup2，必须单独验证 Docker stats、devices、memory、cpuset、pids 和 Android userspace 兼容性。

### overlay2

```text
CONFIG_OVERLAY_FS=y
CONFIG_EXT4_FS_POSIX_ACL=y
CONFIG_EXT4_FS_SECURITY=y
CONFIG_TMPFS_XATTR=y
CONFIG_TMPFS_POSIX_ACL=y
```

`.config` 里有 overlayfs 还不够。Docker root 所在底层文件系统也要支持 xattr/security/ACL 行为。

### bridge、veth、NAT

```text
CONFIG_BRIDGE=y
CONFIG_VETH=y
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

还需要 Android userspace 有可用的 `iptables`。ACHost 的 `net-reconcile` 能修复 bridge NAT 和 policy rule，但不能绕过缺失的内核 netfilter 能力。

### Android 兼容项

```text
CONFIG_PSI=y
CONFIG_CGROUP_BPF=y
CONFIG_BPF=y
CONFIG_BPF_SYSCALL=y
CONFIG_NETFILTER_XT_MATCH_BPF=y
CONFIG_INET_UDP_DIAG=y
```

这些项影响 Android lmkd/pressure 观测、网络兼容和诊断能力。不同 ROM 要求会不同。

## ReSukiSU / KPM

如果目标是 ReSukiSU/KPM 环境，内核构建时应保持对应 SU/KPM 接入方案一致。切换 root 方案前先清理旧 KSU 残留，并核对官方 setup/non-GKI 文档。

ACHost 不要求把容器能力伪装成普通应用能力；它运行在 root/KSU 管理上下文中，重点是可靠生命周期、模块边界和可诊断性。

## 编译内核

不同内核仓库的构建系统差异很大。通用原则：

1. 使用设备项目推荐的 clang/GCC、LLVM、build script。
2. 先生成最终 `.config`。
3. 在最终 `.config` 上跑 ACHost `verify-config`。
4. 编译 boot image、dtbo、vendor_dlkm 或项目需要的输出物。
5. 用项目推荐的 AnyKernel3/刷机包方式打包。

如果你的内核仓库和 ACHost 并排 checkout，可以在内核仓库 README 中链接回本文，并在内核仓库 docs 中写具体机型命令。

## 刷入后验证

设备安装新内核和 ACHost 模块后，su shell：

```sh
MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh

MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

Docker 手动确认：

```sh
docker info
docker stats --no-stream
ip addr show docker0
iptables -t nat -S | grep -E 'DOCKER|MASQUERADE'
```

LXC 手动确认：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-host
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime prepare-bridge
```

只有 `.config` 检查和设备 runtime-test 都通过，才应宣称该内核支持 ACHost。
