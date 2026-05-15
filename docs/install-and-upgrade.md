# 安装、升级与卸载

本文说明 ACHost split KernelSU/ReSukiSU 模块的安装顺序、升级行为和数据保留策略。

## 路径约定

```text
/data/adb/modules/achost-base/achost       base 模块文件
/data/adb/modules/achost-docker/achost     Docker 模块文件
/data/adb/modules/achost-lxc/achost        LXC 模块文件
/data/adb/achost                           持久数据、run、log、Docker root、LXC 容器
/data/adb/ksu/bin                          可选 CLI wrapper 目录
```

模块目录可以被升级覆盖；`/data/adb/achost` 是持久数据目录，除非明确要重置环境，不要删除。

## 安装顺序

1. 在 PC 上生成 zip，见 [`build-and-package.md`](build-and-package.md)。
2. 在模块管理器中先安装 `achost-base.zip`。
3. 重启设备，或至少让模块管理器完成安装脚本。
4. 按需要安装：
   - Docker：`achost-docker.zip`
   - LXC：`achost-lxc.zip`
5. 再次重启或运行对应 runtime start/test。

Docker 和 LXC 模块都会检查 base 模块是否存在。没有 base 时，模块安装或 service 启动会失败/跳过并输出提示。

## 首次验证

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

日志包路径会在脚本末尾输出。失败时先保存日志包，不要直接删除 `/data/adb/achost`。

## 升级行为

升级模块时：

- 模块目录会替换为新版本文件。
- `/data/adb/achost` 下的 Docker 数据、containerd state、LXC 容器、日志和运行配置会保留。
- 可重建的运行态文件可以被清理，例如 socket、pid file、旧 wrapper。
- Docker/LXC 用户态资产由模块包提供；升级对应模块即可替换 runtime 二进制。

升级后建议在 su shell 运行：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop 2>/dev/null || true
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime start
/data/adb/modules/achost-docker/achost/bin/docker --host unix:///data/adb/achost/run/docker.sock info
```

LXC 升级后建议运行：

```sh
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime write-configs
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-host
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime validate-assets
/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json
```

## CLI wrapper

Docker 模块会在 `/data/adb/ksu/bin/docker` 安装 ACHost 管理的 wrapper。LXC 模块会为模块内实际存在的 `lxc*` 命令安装 wrapper。

安装器不会覆盖非 ACHost 管理的同名命令；如果已有命令不是 ACHost wrapper，会保留并输出提示。

root shell 中可直接用：

```sh
docker ps
lxc-ls -f
lxc-info -n <name>
```

如果 PATH 没包含 `/data/adb/ksu/bin`，用模块内绝对路径运行。

## 开机自启

Docker 自启由 Docker 模块自己的配置控制，不属于 base 模块。

LXC 容器自启写入容器 config：

```text
lxc.start.auto = 1
```

开机时 `achost-lxc-runtime autostart` 只处理 ACHost LXC 容器目录，不接管系统其它 LXC 目录。

## 卸载行为

卸载模块通常只删除 `/data/adb/modules/achost-*` 下的模块文件，不应自动删除 `/data/adb/achost` 持久数据。

如果只是卸载 Docker 模块，保留：

```text
/data/adb/achost/docker
/data/adb/achost/containerd
/data/adb/achost/log
```

如果只是卸载 LXC 模块，保留：

```text
/data/adb/achost/lxc/containers
/data/adb/achost/log/lxc
```

只有在确认要重置全部 ACHost 数据时，才手动删除 `/data/adb/achost`。

## 安全清理顺序

重置前先停止 runtime（su shell）：

```sh
/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop 2>/dev/null || true
for name in $(/data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime list --json 2>/dev/null | sed -n 's/.*"name":"\([^"]*\)".*/\1/p'); do
  /data/adb/modules/achost-lxc/achost/bin/achost-lxc-runtime stop "$name" --force 2>/dev/null || true
done
```

然后再决定是否删除持久数据。不要清理其它 KSU/root 模块目录，也不要删除和 ACHost 无关的 `/data/adb` 内容。
