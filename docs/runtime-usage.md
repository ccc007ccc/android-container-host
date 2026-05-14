# ACHost runtime 使用指南

本文说明如何生成、安装和验证 ACHost Android 运行时，重点是当前已经闭环的 Docker native runtime。

## 推荐部署方式

推荐使用 split KernelSU/ReSukiSU 模块：

- `achost-base.zip`：公共 runtime、网络 reconcile/watchdog、OOM 保护、supervisor、runtime-test，不包含 WebUI host。
- `achost-docker.zip`：Docker CLI/daemon/containerd/runc、Docker Rust runtime、独立 Docker WebUI、共享 Rust WebUI API、Docker 测试脚本。
- `achost-lxc.zip`：通用 LXC runtime、配置、LXC userland asset、独立 LXC WebUI、共享 Rust WebUI API 和 rootfs 导入管理。

Docker 模块依赖 base 模块。LXC 模块也依赖 base 模块。Docker 不依赖 LXC；Ubuntu 26.04 作为已验证 rootfs tarball 导入，不再是独立模块。

## 生成 split 包

在项目根目录运行：

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

可选资产：

```bash
--compose-asset /path/to/docker-compose-linux-aarch64
--compose-sha256 <sha256>
--buildx-asset /path/to/docker-buildx-linux-arm64
--buildx-sha256 <sha256>
--buildkit-asset /path/to/buildkit-aarch64.tar.gz
--buildkit-sha256 <sha256>
```

LXC 打包资产只有 `--lxc-asset`，用于基础 LXC 模块的用户态工具和库。容器 rootfs 不进入模块包，改为设备路径上的 tar/tar.gz 通过 `achost-lxc-runtime import-rootfs` 或 WebUI 导入；SHA-256 可选，提供时会先校验。

打包器不会联网下载这些二进制。所有资产必须由调用者显式提供，并建议提供 sha256。

## 安装顺序

1. 刷入 `out/achost-base.zip`。
2. 重启或让模块管理器完成安装脚本。
3. 如需 Docker，刷入 `out/achost-docker.zip`。
4. 如需 LXC，刷入 `out/achost-lxc.zip`。
5. 确认持久数据目录存在：`/data/adb/achost`。

模块安装/升级时会清理旧 runtime shell 入口残留，包括：

```text
achost-docker-start.sh
achost-docker-stop.sh
detect-uplink.sh
container-nat-manager.sh
container-network-watchdog.sh
protect-container-daemons.sh
```

这些入口已经由 Rust 程序替代，不再作为回退路径保留。

## 运行时目录

split 模块常用路径：

```text
/data/adb/modules/achost-base/achost/bin/achost-runtime-core
/data/adb/modules/achost-base/achost/bin/achost-supervise
/data/adb/modules/achost-base/achost/bin/runtime-test.sh
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime
/data/adb/modules/achost-docker/achost/bin/achost-webui-api
/data/adb/modules/achost-docker/achost/bin/docker
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime
/data/adb/modules/achost-lxc/achost/lxc/bin/lxc-start
/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz    示例 rootfs 导入源路径
/data/adb/achost/run/docker.sock
/data/adb/achost/run/containerd.sock
/data/adb/achost/lxc/containers
/data/adb/achost/log/dockerd.log
/data/adb/achost/log/containerd.log
/data/adb/achost/log/lxc
```

Docker 数据、run、log、containerd root/state 和 LXC 容器可变状态默认都放在 `/data/adb/achost`，而不是模块目录，避免模块升级覆盖数据。

## Docker native runtime

Docker 当前只支持 native start path：

```text
ACHOST_RUNTIME_MODE=native
ACHOST_USE_CHROOT=0
```

`achost-docker-runtime start` 会：

1. 准备 `/data/adb/achost/native-root`。
2. 准备 `/data/adb/achost/run`、Docker root、containerd root/state。
3. 写 dockerd/containerd 配置。
4. 准备 devices/memory cgroup。
5. 启动 `achost-supervise`，创建 Docker daemon 使用的 native mount namespace。
6. 启动 containerd。
7. 启动 dockerd。
8. 等待 Docker/containerd socket。
9. 调用 `achost-runtime-core net-reconcile` 修复 bridge NAT 和 policy rule。
10. 启动/保护网络 watchdog 与容器守护进程。

启动：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime start'
```

停止：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop'
```

查看 socket：

```sh
su -c 'ls -l /data/adb/achost/run/docker.sock /data/adb/achost/run/containerd.sock'
```

Docker native namespace 内会有：

```text
/run/docker.sock
/var/run/docker.sock
/var/run -> /run
```

这是为了兼容常见 Linux 容器工具和 `-v /var/run/docker.sock:/var/run/docker.sock` 这类用法，同时不 remount Android 根文件系统。

## Docker CLI

模块会安装 Docker CLI wrapper。通常 root shell 下可以直接运行：

```sh
docker version
docker info
docker ps
```

也可以显式指定 socket：

```sh
/data/adb/modules/achost-docker/achost/bin/docker \
  --host unix:///data/adb/achost/run/docker.sock ps
```

## LXC 基础模块与 verified rootfs 导入

基础 LXC 模块提供通用能力，shell 中直接调用 `achost-lxc-runtime`：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime write-configs'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-host'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime prepare-bridge'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json'
```

安装/升级 LXC 模块时，会在 `/data/adb/ksu/bin` 为模块内 `lxc*`/`lxd*` 可执行文件生成 ACHost 管理的同名 wrapper，root shell 中可以直接调用常用 LXC/LXD 命令。

Ubuntu 26.04 通过已验证 rootfs tarball 导入，不再需要 rootfs seed 模块：

```sh
adb push ubuntu-26.04-arm64-rootfs.tar.gz /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime import-rootfs --name ubuntu-26.04 --rootfs-asset /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz --distro ubuntu --release 26.04 --arch arm64 --sha256 <sha256>'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime status ubuntu-26.04 --json'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime start ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime exec ubuntu-26.04 -- /bin/sh -c "cat /etc/os-release"'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04'
```

基础 LXC runtime 也负责通用容器管理，适用于任意已导入容器：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-autostart ubuntu-26.04 on'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime autostart'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime system-status ubuntu-26.04 --json'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04 --force'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime generate-password ubuntu-26.04 --user root --json'
printf '%s\n' "$NEW_PASSWORD" | su -c '/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-password ubuntu-26.04 --user root --stdin --json'
```

容器自启写在每个容器的 `config` 里，即 `lxc.start.auto = 1|0`。`service.sh` 开机时只调用基础 LXC 模块的 `achost-lxc-runtime autostart`，不会让 Ubuntu rootfs 模块接管容器生命周期。

容器目录固定在 `/data/adb/achost/lxc/containers/<name>`，模块目录只存 runtime、配置模板和 userland asset。rootfs tarball 只是导入源文件，导入完成后可以按需删除。

## WebUI API

Docker/LXC 模块各自打包自己的前端 bundle：Docker WebUI 只显示 Docker 面板，LXC WebUI 只显示 LXC/rootfs/用户密码面板。两个模块仍复用同一个 Rust `achost-webui-api` 后端，由各自模块内的 wrapper 调用。

常用 API：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh status'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-containers'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-images'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh daemon-logs'
```

容器和镜像操作：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh start-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh stop-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh delete-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh remove-image <image-id-or-name>'
```

LXC API：

```sh
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-status'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-list'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-import-rootfs ubuntu-26.04 /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz ubuntu 26.04 arm64 [sha256]'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-check'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-start ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-stop ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-force-stop ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-logs ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-set-autostart ubuntu-26.04 on'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-system-status ubuntu-26.04'
su -c '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-generate-password ubuntu-26.04 root'
su -c 'ACHOST_LXC_PASSWORD="$NEW_PASSWORD" /data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh lxc-set-password ubuntu-26.04 root'
```

打开 `achost-lxc` 的 WebUI 管理 LXC 容器；导入页会要求 rootfs 已经在设备路径上，SHA-256 可留空，填写时会先校验再导入。rootfs 导入、生命周期、自启、强制停止、日志、系统状态和通用用户密码管理都是基础 `achost-lxc` 后端/API 能力。

生成密码和设置密码走同一条密码安全路径；runtime 会直接更新容器 rootfs 的 `/etc/shadow` SHA-512 hash，不依赖容器内 `chpasswd`。自定义密码只做到不把明文拼进普通命令参数或日志，不承诺抵御 root 级观察。WebUI 是强管理能力，不要把它绑定到不可信网络。

## 验证 Docker

推荐先跑 split Docker runtime-test：

```sh
su -c 'MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

它会执行：

- watchdog 状态检查。
- `achost-container-validate.sh` 用户态检查。
- `achost-runtime-core protect-daemons`。
- `achost-docker-runtime start`。
- `achost-runtime-core net-reconcile`。
- `runtime-smoke-docker.sh`。
- `runtime-docker-feature-test.sh`。
- `runtime-net-debug.sh`。
- `achost-docker-runtime stop`。
- `collect-logs.sh`。

默认 smoke 不依赖外网：

```sh
DOCKER_SMOKE_MODE=local
```

可选：

```sh
DOCKER_SMOKE_MODE=local-bridge   # 验证 bridge/veth attach
DOCKER_SMOKE_MODE=publish        # 验证 docker-proxy 和发布端口
DOCKER_SMOKE_MODE=full           # 拉取外部镜像并验证 bridge/host 网络，需要 registry 可用
```

`runtime-docker-feature-test.sh` 默认查找 `/data/local/tmp/achost-dockertest-rootfs.tar`。没有这个 rootfs 时，feature matrix 会 skip 镜像导入后的 exec/cp/bind/proxy-env 项；这不影响基础 Docker smoke 结论。

## 验证 LXC

基础验证不要求 Ubuntu rootfs：

```sh
su -c 'MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

这会检查 host 能力、LXC userland asset、全局配置写入和 `lxcbr0` bridge 准备。没有 rootfs 时，`runtime-smoke-lxc.sh` 会输出 skipped，不会声称容器启动已验证。

提供 rootfs 后可以跑端到端 smoke：

```sh
su -c 'ROOTFS_ASSET=/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz ROOTFS_SHA256=<sha256> MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

该 smoke 会导入临时容器、启动、执行 `/bin/sh` 检查、读取日志、停止并删除临时容器。

## 常见问题

### Docker start 输出 `/run=missing` 是否一定是错误？

不一定。Android 全局根命名空间通常没有普通 Linux 的 writable `/run`。ACHost 依赖 `achost-supervise` 给 Docker daemon 创建 native mount namespace；重点看 supervisor namespace 下的 `/proc/<supervisor-pid>/root/run` 是否存在、可写，以及 Docker/containerd socket 是否创建成功。

### 为什么不用 Docker chroot？

当前目标是 native Docker。Docker 29 在 Android 16/lmi 上已经验证可以通过 ACHost-managed native namespace 和 `/data/adb/achost/run` socket 运行，不需要 chroot mounts。chroot path 容易扩大维护面、隐藏路径差异，也不符合“像普通 Linux 宿主一样运行 Docker”的目标。

### Docker Hub 拉取失败是否表示 Docker 不完整？

不一定。Android 设备可能有 DNS、VPN、Wi-Fi 连通性检测或 registry 访问问题。先用默认 `DOCKER_SMOKE_MODE=local` 验证 daemon、containerd、overlay2 和本地容器；再单独排查外网、DNS 和 registry。

### 发布端口访问不到怎么办？

先确认 `docker-proxy` 是否存在并可执行，再跑：

```sh
DOCKER_SMOKE_MODE=publish su -c 'MODE=docker /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

如果 local smoke 通过但 publish 失败，重点检查 `docker-proxy`、iptables nat、Android 防火墙/SELinux 和监听地址。

## 清理测试环境

设备测试后建议清理：

```sh
su -c 'docker rm -f achost-nginx achost-publish-test achost-feature-life achost-feature-cp 2>/dev/null || true'
su -c 'for image in $(docker images --format "{{.Repository}}:{{.Tag}}" | grep -E "^achost-local-smoke:|^achost-dockertest:" 2>/dev/null || true); do docker rmi "$image" || true; done'
su -c 'for name in achost-lxc-smoke achost-lxc-smoke-*; do /data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime destroy "$name" 2>/dev/null || true; done'
su -c 'rm -rf /data/local/tmp/achost-runtime-test* /data/local/tmp/achost-feature-* /data/local/tmp/achost-local-rootfs*'
```

如果测试前 Docker 是 running，测试后应恢复 running；如果测试前是 stopped，测试后应恢复 stopped。
