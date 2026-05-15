# 可靠诊断指南

ACHost 排障围绕三件事：可靠启动、可靠停止、可靠诊断。先保存日志，再决定是否重启、升级或清理数据。

## 一键收集日志

su shell：

```sh
OUT_DIR=/data/local/tmp /data/adb/modules/achost-base/achost/bin/collect-logs.sh
```

脚本会输出目录或 tar.gz 路径，内容包括：

- `achost_summary.txt`：系统/root、模块安装、daemon 状态、恢复建议。
- `achost_native_namespace.txt`：supervisor/containerd/dockerd mount namespace、run/socket/cgroup。
- `achost_network_watchdog.txt`：watchdog pid、cmdline、日志。
- `iptables_*`、`ip_addr`、`ip_route`、`ip_rule`：网络状态。
- `docker_info`、`docker_stats`、`docker_container_cgroups`、`docker_bridge`。
- `lxc_checkconfig`、`lxc_ls`。
- `achost_daemon_logs`、`dmesg`、`logcat_all`、`avc_logs`、`oom_logs`。

## runtime-test

Docker（su shell）：

```sh
MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

LXC（su shell）：

```sh
MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

网络专项（su shell）：

```sh
MODE=network OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

## Docker start 失败

### 1. 先看错误卡在哪一层

`achost-docker-runtime start` 的关键 readiness gate：

1. supervisor socket 和 client probe。
2. containerd socket。
3. `ctr --address ... version`。
4. dockerd socket。
5. `docker --host ... info`。
6. network reconcile。

常见错误和下一步：

| 输出 | 含义 | 下一步 |
| --- | --- | --- |
| `supervisor disabled` / `supervisor not ready` | native namespace supervisor 没启动或不可用 | 看 `/data/adb/achost/log/achost-supervise.log` 和 `achost_native_namespace.txt` |
| `containerd socket not ready` | containerd 进程或 socket 没起来 | 看 `/data/adb/achost/log/containerd.log` |
| `containerd API not ready` | socket 存在但 API 不可用 | 运行 `ctr --address /data/adb/achost/run/containerd.sock version` |
| `dockerd socket not ready` | dockerd 没创建 Docker socket | 看 `/data/adb/achost/log/dockerd.log` |
| `dockerd API not ready` | socket 存在但 Docker API 不可用 | 运行 `docker --host unix:///data/adb/achost/run/docker.sock info` |
| `network reconciliation pending` | daemon 已起，但 NAT/rule/bridge 需要检查 | 看 `/data/local/tmp/achost-network-watchdog.log` 和 `runtime-net-debug.txt` |

### 2. 确认不是旧 chroot 路径

su shell：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-container-validate.sh | grep -E 'runtime_mode|use_chroot|cgroup_mode'
```

期望：

```text
runtime_mode=native
use_chroot=0
```

### 3. 检查 Docker API

su shell：

```sh
/data/adb/modules/achost-docker/achost/bin/docker \
  --host unix:///data/adb/achost/run/docker.sock info
```

重点看：

```text
Storage Driver: overlay2
Cgroup Driver: cgroupfs
Cgroup Version: 1
```

## Docker stop 后有残留

先运行 stop（su shell）：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop
```

输出会列出：

```text
remaining_dockerd_pids=...
remaining_containerd_pids=...
remaining_network_watchdog_pids=...
docker_socket=...
containerd_socket=...
supervisor_socket=...
network_watchdog_pid=...
```

如果还有残留：

1. 确认 pid 是否来自 `/data/adb/modules/achost-docker/achost/bin` 或 `/data/adb/modules/achost-base/achost/bin`。
2. 不要按 `pidof dockerd` 全局杀；其它模块或用户进程可能同名。
3. 保存 `collect-logs.sh` 输出，再决定是否手动处理。

## Docker stats 为 0

Docker stats 不是 WebUI 问题。链路是：

```text
Docker API -> containerd task metrics -> shim Stats() -> containerd cgroup v1 Stat()
```

检查（su shell）：

```sh
docker stats --no-stream
ctr --address /data/adb/achost/run/containerd.sock tasks metrics <container-id>
```

如果 `runc events --stats` 有真实值，但 Docker/ctr 是 0，重点看 containerd/shim mount namespace 中 cgroup mount 顺序。ACHost 期望 native namespace 中 `/sys/fs/cgroup/*` v1 mounts 排在 Android `/dev/*` cgroup mounts 前面。

收集：

```sh
OUT_DIR=/data/local/tmp/achost-net /data/adb/modules/achost-base/achost/bin/runtime-net-debug.sh
```

看 `docker_stats`、`docker_container_cgroups`、`achost_native_namespace`。

## 网络不通

先确认 bridge 和 uplink（su shell）：

```sh
ip addr show docker0
ip route get 1.1.1.1
/data/adb/modules/achost-base/achost/bin/achost-runtime-core detect-uplink 1.1.1.1
/data/adb/modules/achost-base/achost/bin/achost-runtime-core net-reconcile
```

再看：

```sh
iptables -t nat -S
iptables -L FORWARD -n -v
cat /proc/sys/net/ipv4/ip_forward
```

如果本地 smoke 通过但外网 pull 失败，优先排查 DNS、Wi-Fi 连通性检测、VPN、registry 可达性和 Android 防火墙策略。

## LXC template 报 `bad template: download`

这个错误通常不是内核问题，而是 LXC template 不可执行或 `LXC_TEMPLATE_PATH` 未导出。

检查（su shell）：

```sh
ls -l /data/adb/modules/achost-lxc/achost/lxc/share/lxc/templates/lxc-download
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets
```

期望 template 有执行权限。模块 env 会导出：

```text
LXC_TEMPLATE_PATH=/data/adb/modules/achost-lxc/achost/lxc/share/lxc/templates
```

如果从手动 shell 使用原生 `lxc-create -t download`，先进入 su shell 并加载模块环境，或直接用 ACHost wrapper。

## LXC start/stop 卡住

start 失败会输出状态和日志路径。手动检查（su shell）：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime status <name> --json
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime logs <name>
lxc-info -n <name>
```

stop 太慢时：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop <name> --force
```

如果 force-stop 后仍显示运行，保存日志，再检查 LXC 进程和容器 cgroup。

## WebUI 卡住或空白

WebUI 后端命令有 timeout 和输出上限：

- 普通命令默认 120 秒。
- Docker pull、LXC rootfs import 最多 600 秒。
- start/stop/import 等长操作失败会在输出面板显示 rc、stdout/stderr 和 timeout 信息。
- 输出超过 64 KiB 会截断，并追加 `[output truncated by achost-webui-api]`。

如果前端空白，先在 su shell 直接运行对应 API：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh status
/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-status
```

如果 API 正常，再检查 WebUI dist 是否打包到对应模块：

```sh
ls -l /data/adb/modules/achost-docker/achost/webroot
ls -l /data/adb/modules/achost-lxc/achost/webroot
```

## 测试后清理

见 [`runtime-usage.md`](runtime-usage.md#测试后清理)。清理只针对 ACHost 测试容器、测试镜像和 `/data/local/tmp/achost-*` 临时文件，不要删除其它 root 模块目录。
