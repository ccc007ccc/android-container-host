# Android Container Host Kernel Layer

ACHost（Android Container Host Kernel Layer）是面向 KernelSU/ReSukiSU/root 环境的 Android 容器宿主层。它把“这台 Android 设备能不能稳定跑 Docker/LXC”拆成可检查、可打包、可启动、可停止、可诊断的流程。

本仓库不绑定某个内核树。内核仓库只是输入；ACHost 提供 profile、检测工具、KernelSU 模块打包、Docker/LXC runtime、WebUI 和设备诊断脚本。

## 当前主线

主线目标是可靠启动、可靠停止、可靠诊断：

- Docker 走 native runtime，不走旧 chroot：`ACHOST_RUNTIME_MODE=native`、`ACHOST_USE_CHROOT=0`。
- Docker start 会等待 supervisor、containerd socket/API、dockerd socket/API，并在失败时给出日志路径。
- Docker stop 只停止 ACHost 管理的 dockerd/containerd/watchdog/supervisor，不按全局进程名扫杀其它模块或系统进程。
- LXC start/stop 会等待 `RUNNING`/`STOPPED` 状态，并输出日志位置和下一步命令。
- `runtime-test.sh`、`runtime-net-debug.sh`、`collect-logs.sh` 用固定章节收集 daemon、native namespace、cgroup、network、Docker/LXC 和恢复建议。

## 模块组成

推荐 split KernelSU/ReSukiSU 模块：

| 模块 | 内容 | 是否依赖 |
| --- | --- | --- |
| `achost-base.zip` | 公共 env、`achost-runtime-core`、`achost-supervise`、诊断脚本、runtime-test | 无 |
| `achost-docker.zip` | Docker/containerd/runc、`achost-docker-runtime`、Docker WebUI、共享 WebUI API | `achost-base` |
| `achost-lxc.zip` | LXC userland、`achost-lxc-runtime`、LXC WebUI、共享 WebUI API | `achost-base` |

Docker 不依赖 LXC；LXC 不依赖 Docker。容器数据默认放在 `/data/adb/achost`，模块升级不会覆盖 Docker root、containerd state 或 LXC 容器目录。

## 最短路径

### 1. 在 PC 上检查内核配置

```bash
bin/achost detect \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out

bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1
```

不要只看 defconfig 或 `/proc/config.gz`。最终 `.config` 和设备运行时结果才是准确信号。

### 2. 在 PC 上构建 WebUI 并打包模块

```bash
npm install --prefix webui
npm run build --prefix webui

scripts/package-all.sh --version 0.1.3
```

脚本会分别生成 base/Docker/LXC 模块、校验 zip 内容并写入 `SHA256SUMS.txt`。Docker/LXC 模块需要 Android/arm64 用户态资产；默认从 `out/assets/` 读取，完整说明见 [`docs/build-and-package.md`](docs/build-and-package.md)。

### 3. 在设备上安装和验证

安装顺序：先 `achost-base.zip`，再按需要安装 `achost-docker.zip` 和 `achost-lxc.zip`。

Docker 验证（su shell）：

```sh
MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

LXC 验证（su shell）：

```sh
MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

## 文档导航

- [`docs/install-and-upgrade.md`](docs/install-and-upgrade.md)：安装顺序、升级行为、卸载和数据保留。
- [`docs/build-and-package.md`](docs/build-and-package.md)：从 fresh checkout 到 `achost-base.zip` / `achost-docker.zip` / `achost-lxc.zip`。
- [`docs/runtime-usage.md`](docs/runtime-usage.md)：设备上日常使用 Docker、LXC 和 WebUI。
- [`docs/diagnostics.md`](docs/diagnostics.md)：启动失败、停止残留、Docker stats、LXC template、网络/cgroup 排障。
- [`docs/device-compatibility.md`](docs/device-compatibility.md)：设备适配模型、内核能力和支持分级。
- [`docs/kernel-build.md`](docs/kernel-build.md)：如何检查/编译自己的内核以满足 ACHost。

## 已验证基线

当前主验证设备：

```text
Xiaomi lmi / sm8250
Android 16 / Linux 4.19 vendor kernel
ReSukiSU/KPM root 环境
Docker native runtime + cgroup v1
```

已验证 Docker socket、containerd、overlay2、bridge NAT、Docker stats、runtime smoke、WebUI API 和 split module boundary。其它设备必须按同样流程验证后再宣称支持。

## 已知限制

- ACHost 是 root/KSU 管理组件，不提供强安全隔离边界；不要把 WebUI/API 暴露给不可信网络。
- Docker chroot 启动路径已淘汰，当前维护路径是 native runtime。
- cgroup v1 是当前稳定验证路径；cgroup v2 设备需要单独验证 devices/memory/cpuset 等行为。
- LXC 端到端能力取决于 Android/arm64 LXC userland 和 rootfs 资产。
- qtaguid/container-safe patch 仍需按设备验证，不能只凭配置项判断可用。
