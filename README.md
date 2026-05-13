# Android Container Host Kernel Layer

Android Container Host Kernel Layer（ACHost / ACHKL）是一个外置的 Android 容器宿主支持项目，用来评估、准备和验证 Android 内核是否足以稳定运行 Docker / LXC。

项目不属于任何目标内核树。目标内核只是 `achost` 命令的输入；本项目通过 profile、config fragment、patch、运行时模块和验证脚本，把“这个 Android 内核能不能当容器宿主”变成可检查、可打包、可迁移的流程。

## 当前状态

Docker native runtime 是当前主线，已经完成核心闭环：

- Docker 使用 `achost-docker-runtime start|stop` 直接管理 containerd、dockerd、cgroup、native namespace、socket 和网络 reconcile。
- Docker 默认只走 native runtime：`ACHOST_RUNTIME_MODE=native`、`ACHOST_USE_CHROOT=0`。
- 不再生成 Docker chroot 启动配置；CLI 的 `--docker-runtime-mode` 只接受 `native`。
- split 模块边界已明确：
  - `achost-base`：公共 runtime、`achost-runtime-core`、`achost-supervise`。
  - `achost-docker`：Docker 引擎、`achost-docker-runtime`、WebUI API、Docker smoke/feature tests。
  - `achost-lxc`：LXC 配置和验证入口。
- WebUI 已支持 Docker 状态、启动/停止、容器列表/创建/启动/停止/重启/删除/日志/inspect、镜像列表/拉取/删除、daemon 日志。
- 已在 Xiaomi lmi / sm8250 / Android 16 / Linux 4.19 vendor kernel 上验证 native Docker socket、containerd、overlay2、bridge NAT、runtime smoke 和 WebUI API。

LXC 有配置槽位和验证路径，但完整 Android-compatible LXC userspace 仍处于实验阶段。qtaguid 修复和自动 patch 应用仍是后续工作。

## 这个项目是否通用

目标是通用项目：**只要设备内核和 Android 运行环境提供足够能力，就不应绑定某个机型才能用。**

但是它不是“所有 Android 手机直接刷模块就能跑 Docker”。是否可用取决于：

1. 内核是否有足够 namespace、cgroup、overlayfs、veth/bridge、netfilter/iptables 能力。
2. root 环境是否允许模块放置二进制、启动守护进程、创建 socket、挂载 cgroup。
3. Android ROM/SELinux/lmkd/netd 是否不会持续破坏容器所需进程、网络和挂载状态。
4. 是否提供 Android/arm64 可执行的 Docker userland 资产。

换句话说：ACHost 是通用的检测、打包、运行和验证层；每个设备仍必须先通过内核能力检查和设备验证。详细适配说明见 [`docs/device-compatibility.md`](docs/device-compatibility.md)。

## 目录结构

```text
achost/                         Python CLI 与打包逻辑
bin/achost                      CLI 入口
config/fragments/               内核配置片段
crates/achost-runtime-core/      公共 Rust runtime：网络、uplink、watchdog、OOM 保护
crates/achost-docker-runtime/    Docker Rust runtime：native root、cgroup、config、start/stop
crates/achost-supervise/         native namespace supervisor
crates/achost-webui-api/         WebUI 后端 API
runtime/android/                 Android 端 env、validate、模块入口模板
scripts/                         本地与设备运行时验证脚本
profiles/                        可组合的内核能力 profile
devices/                         已知设备元数据
out/                             生成的模块目录和 zip 包
```

## 快速检查目标内核

下面示例以 Xiaomi lmi 内核路径为例。换机型时替换 `--kernel-tree`、`--out` 和设备元数据文件。

```bash
bin/achost detect \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out

bin/achost plan \
  --kernel-tree /path/to/android_kernel \
  --out /path/to/android_kernel/out \
  --device devices/xiaomi-sm8250-lmi.yml \
  --write-report

bin/achost verify-config \
  --config /path/to/android_kernel/out/.config \
  --profile android-container-host-v1

scripts/verify-config.sh /path/to/android_kernel/out/.config

third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh /path/to/android_kernel/out/.config
```

如果目标设备没有现成 metadata，可以先用 `profiles/docker-bridge-overlay2.yml` 做最小 Docker 能力检查，再新增 `devices/<vendor-soc-device>.yml` 描述设备默认 cgroup、bridge、subnet 和已知限制。

## 生成运行时包

`runtime-install` 不下载 Docker/LXC 二进制。Docker 支持需要显式提供 Android/arm64 Docker static tarball；可选提供 Compose、buildx、BuildKit 资产。

### 推荐 split 模块

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
  --compose-asset /path/to/docker-compose-linux-aarch64 \
  --compose-sha256 <sha256> \
  --buildx-asset /path/to/docker-buildx-linux-arm64 \
  --buildx-sha256 <sha256> \
  --buildkit-asset /path/to/buildkit-aarch64.tar.gz \
  --buildkit-sha256 <sha256> \
  --output out/achost-docker \
  --zip out/achost-docker.zip

PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target lxc \
  --cgroup-mode v1 \
  --output out/achost-lxc \
  --zip out/achost-lxc.zip
```

安装顺序建议：先刷 `achost-base.zip`，再刷 `achost-docker.zip`，需要 LXC 时再刷 `achost-lxc.zip`。

### 手动包

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode manual \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/runtime-manual-docker
```

手动安装后按生成脚本提示复制到 `/data/adb/achost`。

## Android 设备上怎么用 Docker

split 模块安装后常用路径：

```text
/data/adb/modules/achost-base/achost       公共组件
/data/adb/modules/achost-docker/achost     Docker 组件
/data/adb/achost                           持久数据、run、log、Docker root
```

启动和停止：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime start'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-docker-runtime stop'
```

Docker socket：

```text
/data/adb/achost/run/docker.sock
/data/adb/achost/run/containerd.sock
```

Docker daemon 的 native namespace 内还会暴露：

```text
/run/docker.sock
/var/run/docker.sock
/var/run -> /run
```

模块会安装 Docker CLI wrapper；在 root shell 中通常可以直接运行：

```sh
docker version
docker ps
docker run --rm --network none <local-image> true
```

文档入口见 [`docs/README.md`](docs/README.md)，更多运行和验证说明见 [`docs/runtime-usage.md`](docs/runtime-usage.md)。

## WebUI

Docker 模块包含 WebUI 静态文件和 `achost-webui-api`。WebUI API 支持：

```sh
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api status'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api list-containers'
su -c '/data/adb/modules/achost-docker/achost/bin/achost-webui-api list-images'
```

如果通过模块或外部 HTTP 服务暴露 WebUI，确保只绑定可信网络或本机端口；Docker 管理接口具备启动/删除容器和删除镜像能力，不应暴露给不可信网络。

## 运行时验证

推荐先跑 Docker-only 验证：

```sh
su -c 'MODE=docker OUT_DIR=/data/local/tmp/achost-runtime-test /data/adb/modules/achost-base/achost/bin/runtime-test.sh'
```

默认 Docker smoke 是本地模式，不依赖 Docker Hub：脚本会用已打包的 Docker CLI 构造 tiny local image，验证 Docker version/info、overlay2 和 `--network none` 容器。

可选 smoke 模式：

```sh
DOCKER_SMOKE_MODE=local-bridge   # 额外验证 bridge/veth attach
DOCKER_SMOKE_MODE=publish        # 验证 docker-proxy 和 127.0.0.1 发布端口
DOCKER_SMOKE_MODE=full           # 需要 registry 访问和外网镜像拉取
```

`runtime-docker-feature-test.sh` 会测试 Docker exec、cp、bind mount、proxy env 等功能；如果没有 `/data/local/tmp/achost-dockertest-rootfs.tar`，相关 feature matrix 会 skip，不阻断基础 smoke。

## 内核能力基线

Docker native runtime 至少需要：

- namespaces：`CONFIG_NAMESPACES`、`CONFIG_UTS_NS`、`CONFIG_IPC_NS`、`CONFIG_PID_NS`、`CONFIG_NET_NS`。
- cgroup v1 基线：`CONFIG_CGROUPS`、`CONFIG_CGROUP_DEVICE`、`CONFIG_CGROUP_PIDS`、`CONFIG_CGROUP_SCHED`、`CONFIG_CPUSETS`、`CONFIG_MEMCG`。
- overlay2：`CONFIG_OVERLAY_FS`，底层文件系统需要 xattr/security/ACL 支持。
- bridge/NAT：bridge、veth、netfilter、conntrack、iptables nat/filter、MASQUERADE、addrtype、conntrack match。
- Android 兼容项：PSI、BPF/cgroup BPF、iptables/bpf 相关能力按设备情况检查。

不要只看 defconfig。必须验证最终 `out/.config`，再用设备 runtime-test 验证真实运行行为。

## 已知限制

- 已验证设备是 `devices/xiaomi-sm8250-lmi.yml` 描述的 lmi/sm8250 Android 16 4.19 vendor kernel；其它设备需要按同样流程验证。
- Docker native runtime 是支持路径；Docker chroot 启动路径已淘汰。
- cgroup v2 可生成配置，但当前已验证稳定路径是 Android 16/lmi 上的 cgroup v1 布局。
- qtaguid/container-safe patch 仍是占位风险项，不应宣称已经完整解决 Android 流量统计兼容。
- LXC 支持还没有像 Docker native 一样完成端到端闭环。

## 开发验证清单

修改 runtime、打包或 WebUI 后至少运行：

```bash
python3 tests/test_runtime_install.py
python3 tests/test_runtime_test.py
python3 tests/test_moby_check_config.py
cargo fmt --manifest-path Cargo.toml --all --check
cargo test --manifest-path Cargo.toml --workspace
cargo clippy --manifest-path Cargo.toml --workspace -- -D warnings
npm run build --prefix webui
```

设备验证后清理：

```sh
docker rm -f achost-nginx achost-publish-test achost-feature-life achost-feature-cp 2>/dev/null || true
for image in $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E '^achost-local-smoke:|^achost-dockertest:' 2>/dev/null || true); do docker rmi "$image" || true; done
rm -rf /data/local/tmp/achost-runtime-test* /data/local/tmp/achost-feature-* /data/local/tmp/achost-local-rootfs*
```
