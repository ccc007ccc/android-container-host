# ACHost runtime 使用指南

本文说明如何生成、安装和验证 ACHost Android 运行时，重点是当前已经闭环的 Docker native runtime。

## 推荐部署方式

推荐使用 split KernelSU/ReSukiSU 模块：

- `achost-base.zip`：公共 runtime、网络 reconcile/watchdog、OOM 保护、supervisor、runtime-test。
- `achost-docker.zip`：Docker CLI/daemon/containerd/runc、Docker Rust runtime、WebUI API、Docker 测试脚本。
- `achost-lxc.zip`：LXC 配置和验证入口。

Docker 模块依赖 base 模块。LXC 模块也依赖 base 模块。Docker 不依赖 LXC。

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

打包器不会联网下载这些二进制。所有资产必须由调用者显式提供，并建议提供 sha256。

## 安装顺序

1. 刷入 `out/achost-base.zip`。
2. 重启或让模块管理器完成安装脚本。
3. 刷入 `out/achost-docker.zip`。
4. 如需 LXC，再刷入 `out/achost-lxc.zip`。
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
/data/adb/achost/run/docker.sock
/data/adb/achost/run/containerd.sock
/data/adb/achost/log/dockerd.log
/data/adb/achost/log/containerd.log
```

Docker 数据、run、log、containerd root/state 默认都放在 `/data/adb/achost`，而不是模块目录，避免模块升级覆盖数据。

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

## WebUI API

常用 API：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api status'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api list-containers'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api list-images'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api daemon-logs'
```

容器和镜像操作：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api start-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api stop-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api delete-container <name-or-id>'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api remove-image <image-id-or-name>'
```

WebUI 暴露的是 Docker 管理能力。不要把它绑定到不可信网络。

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
su -c 'rm -rf /data/local/tmp/achost-runtime-test* /data/local/tmp/achost-feature-* /data/local/tmp/achost-local-rootfs*'
```

如果测试前 Docker 是 running，测试后应恢复 running；如果测试前是 stopped，测试后应恢复 stopped。
