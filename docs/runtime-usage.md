# 运行时使用指南

本文只写设备上的日常使用：启动/停止 Docker、管理 LXC 容器、使用 WebUI/API。构建打包见 [`build-and-package.md`](build-and-package.md)，排障见 [`diagnostics.md`](diagnostics.md)。

## 常用路径

```text
/data/adb/modules/achost-base/achost       公共组件
/data/adb/modules/achost-docker/achost     Docker 组件
/data/adb/modules/achost-lxc/achost        LXC 组件
/data/adb/achost                           持久数据、run、log、Docker root、LXC 容器
/data/adb/achost/run/docker.sock           Docker socket
/data/adb/achost/run/containerd.sock       containerd socket
/data/adb/achost/lxc/containers            LXC 容器目录
/data/adb/achost/log/lxc                   LXC 日志目录
```

## Docker

### 启动和停止

su shell：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime start
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop
```

start 成功至少应看到 Docker API 可用：

```sh
/data/adb/modules/achost-docker/achost/bin/docker \
  --host unix:///data/adb/achost/run/docker.sock info
```

stop 会输出剩余 ACHost-owned 进程、socket、pid file 和 watchdog 日志状态。它不会删除 Docker 数据目录。

### Docker CLI

如果 `/data/adb/ksu/bin` 在 PATH 中，可以直接运行：

```sh
docker version
docker info
docker ps
```

否则使用模块内路径：

```sh
/data/adb/modules/achost-docker/achost/bin/docker \
  --host unix:///data/adb/achost/run/docker.sock ps
```

本地 smoke 示例：

```sh
docker run --rm --network none <local-image> true
```

避免在首次验证时直接依赖 Docker Hub。先确认本地 daemon、containerd、overlay2 和 cgroup 正常，再排查外网。

### WebUI

Docker 模块包含独立 Docker WebUI。它只管理 Docker：

- Docker 状态、启动、停止。
- 容器列表、创建、启动、停止、重启、删除、日志、inspect。
- 镜像列表、拉取、删除。
- daemon 日志和 runtime check。

后端 API wrapper 示例（su shell）：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh status
/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-containers
/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh list-images
/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh daemon-logs
```

## LXC

### 初始化检查

su shell：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime write-configs
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-host
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime prepare-bridge
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json
```

安装后 root shell 通常可以直接使用 ACHost wrapper：

```sh
lxc-ls -f
lxc-info -n <name>
lxc-start -n <name> -d
```

如果 PATH 不包含 `/data/adb/ksu/bin`，用 `/data/adb/modules/achost-lxc/achost/lxc/bin/<command>`。

### 导入 rootfs

PC：

```bash
adb push ubuntu-26.04-arm64-rootfs.tar.gz /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz
```

su shell：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime import-rootfs \
  --name ubuntu-26.04 \
  --rootfs-asset /data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz \
  --distro ubuntu \
  --release 26.04 \
  --arch arm64 \
  --sha256 <sha256>
```

`--sha256` 可选；填写时会先校验再导入。导入完成后，rootfs tarball 只是源文件，可以按需删除。

### 生命周期

su shell：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime start ubuntu-26.04
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime status ubuntu-26.04 --json
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime exec ubuntu-26.04 -- /bin/sh -c 'cat /etc/os-release'
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04
```

如果正常 stop 等待太久：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop ubuntu-26.04 --force
```

start 会等待容器进入 `RUNNING`；stop 会等待容器进入 `STOPPED`。失败时看输出中的 log 路径。

### 自启

su shell：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-autostart ubuntu-26.04 on
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-autostart ubuntu-26.04 off
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime autostart
```

自启状态写在容器 config 中，不需要额外 rootfs 模块。

### 日志和系统状态

su shell：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime logs ubuntu-26.04
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime system-status ubuntu-26.04 --json
```

### 用户密码

生成密码 hash（su shell）：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime generate-password ubuntu-26.04 --user root --json
```

设置自定义密码时用 stdin，避免把明文放到普通命令参数：

```sh
printf '%s\n' "$NEW_PASSWORD" | \
  /data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime set-password ubuntu-26.04 --user root --stdin --json
```

该功能直接更新容器 rootfs 的 `/etc/shadow` SHA-512 hash，不依赖容器内 `chpasswd`。

### LXC WebUI

LXC 模块包含独立 LXC WebUI。它只管理 LXC：

- 概览、状态、运行检查。
- rootfs 导入。
- 容器启动、停止、强制停止、删除。
- 自启开关。
- 日志、系统状态、容器命令。
- 用户密码管理。

WebUI 长耗时命令有后端 timeout，失败输出会显示在最近输出/输出面板中。

## runtime-test

快速验证（su shell）：

```sh
MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh

MODE=lxc OUT_DIR=/data/local/tmp/achost-runtime-test \
  /data/adb/modules/achost-base/achost/bin/runtime-test.sh
```

如果测试前 Docker 已经运行，`runtime-test.sh` 会尽量保持测试后仍运行；如果测试前未运行，测试后会停止 Docker。

## 测试后清理

su shell：

```sh
docker rm -f achost-nginx achost-publish-test achost-feature-life achost-feature-cp 2>/dev/null || true
for image in $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E '^achost-local-smoke:|^achost-dockertest:' 2>/dev/null || true); do
  docker rmi "$image" || true
done
for name in achost-lxc-smoke achost-lxc-smoke-*; do
  /data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime destroy "$name" 2>/dev/null || true
done
rm -rf /data/local/tmp/achost-runtime-test* /data/local/tmp/achost-feature-* /data/local/tmp/achost-local-rootfs*
```
