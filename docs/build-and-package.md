# 构建与打包

本文说明如何从 fresh checkout 生成 `achost-base`、`achost-docker` 和 `achost-lxc` 三个 KernelSU/ReSukiSU 模块。正式发布不要手写 `runtime-install` 长命令，统一使用 `scripts/package-*.sh`。

## PC 依赖

需要：

- Python 3。
- Rust toolchain 和 `cargo`。
- Android NDK，提供 `aarch64-linux-android*-clang`。
- Node.js/npm，用于构建 WebUI。
- Docker Android/arm64 静态资产。
- Docker Compose、buildx、BuildKit Android/arm64 资产。
- LXC Android/arm64 userland 资产。

`runtime-install` 会自动用 `cargo build --release --target aarch64-linux-android` 构建 Rust runtime。它会查找 NDK clang；也可以显式设置：

```bash
export ACHOST_ANDROID_LINKER=/path/to/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang
```

## 获取依赖并构建 WebUI

PC，在仓库根目录：

```bash
npm install --prefix webui
npm run build --prefix webui
```

生成：

```text
webui/dist/docker
webui/dist/lxc
```

Docker 和 LXC 模块各自打包自己的前端 bundle，base 模块不包含 WebUI host。

## 准备 release 资产

默认脚本从 `out/assets/` 读取这些文件：

```text
out/assets/docker-29.4.3.tgz
out/assets/docker-compose-linux-aarch64
out/assets/buildx-v0.33.0.linux-arm64
out/assets/buildkit-v0.29.0.linux-arm64.tar.gz
out/assets/lxc-userland-android-arm64-lxc-3.1.0.tar.gz
```

Docker release 包必须包含：

```text
docker
dockerd
containerd
containerd-shim-runc-v2
ctr
runc
docker-compose / compose plugin
docker-buildx / buildx plugin
buildctl
buildkitd
```

对 ACHost Docker 模块来说，Compose/buildx/BuildKit 是正式发布必备资产，不是可以漏掉的“可选项”。缺 Compose 时基础 Docker 仍可能能启动，但 `docker compose` / `docker-compose` 会坏，包也不符合模块说明。

LXC asset 需要 Android/arm64 可执行的 LXC userland，例如：

```text
lxc-start
lxc-stop
lxc-attach
lxc-info
lxc-ls
lxc-destroy
lxc-execute
lxc-checkconfig
share/lxc/templates/lxc-download
```

rootfs tarball 不进入 LXC 模块包。把 rootfs 放到设备路径后，用 `achost-lxc-runtime import-rootfs` 或 LXC WebUI 导入。

## 打包全部模块

PC，在仓库根目录：

```bash
scripts/package-all.sh --version 0.1.4
```

输出：

```text
out/achost-v0.1.4/achost-base-v0.1.4.zip
out/achost-v0.1.4/achost-docker-v0.1.4.zip
out/achost-v0.1.4/achost-lxc-v0.1.4.zip
out/achost-v0.1.4/SHA256SUMS.txt
```

不传 `--version` 时，脚本默认读取 `achost.__version__`。

## 单独打包某个模块

```bash
scripts/package-base.sh --version 0.1.4
scripts/package-docker.sh --version 0.1.4
scripts/package-lxc.sh --version 0.1.4
```

每个脚本都会：

1. 生成对应 stage 目录。
2. 生成版本化 zip。
3. 调用 `runtime-validate --release` 检查 stage、manifest 和 zip。
4. 更新同目录的 `SHA256SUMS.txt`。

默认拒绝覆盖已有输出。需要重打当前版本时，只清理当前目标输出：

```bash
scripts/package-docker.sh --version 0.1.4 --clean-output
```

`--clean-output` 只允许清理仓库 `out/` 下当前版本的对应 stage 和 zip，不会删除整个 `out/`。

## 自定义资产

如果不用默认 `out/assets/`，可以指定资产目录：

```bash
scripts/package-all.sh --version 0.1.4 --assets-dir /path/to/assets
```

也可以逐项覆盖。正式包要求自定义资产和 sha256 成对出现：

```bash
scripts/package-docker.sh --version 0.1.4 \
  --docker-asset /path/to/docker-29.4.3.tgz \
  --docker-sha256 <sha256> \
  --compose-asset /path/to/docker-compose-linux-aarch64 \
  --compose-sha256 <sha256> \
  --buildx-asset /path/to/buildx-v0.33.0.linux-arm64 \
  --buildx-sha256 <sha256> \
  --buildkit-asset /path/to/buildkit-v0.29.0.linux-arm64.tar.gz \
  --buildkit-sha256 <sha256>

scripts/package-lxc.sh --version 0.1.4 \
  --lxc-asset /path/to/lxc-userland-android-arm64-lxc-3.1.0.tar.gz \
  --lxc-sha256 <sha256>
```

可以先 dry-run 检查命令，不实际打包：

```bash
scripts/package-docker.sh --version 0.1.4 --dry-run
```

## 输出物检查

脚本会自动跑 release validator。手工复查可以用：

```bash
unzip -l out/achost-v0.1.4/achost-docker-v0.1.4.zip \
  | grep -E 'docker-compose|docker-buildx|buildctl|buildkitd'

unzip -l out/achost-v0.1.4/achost-lxc-v0.1.4.zip \
  | grep 'lxc-download'

cd out/achost-v0.1.4
sha256sum -c SHA256SUMS.txt
```

也可以直接调用底层校验：

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-validate \
  --module-target docker \
  --package-root out/achost-v0.1.4/achost-docker \
  --zip out/achost-v0.1.4/achost-docker-v0.1.4.zip \
  --release
```

## 本地验证

PC：

```bash
python3 tests/test_runtime_install.py
python3 tests/test_package_scripts.py
python3 tests/test_runtime_test.py
cargo fmt --manifest-path Cargo.toml --all --check
cargo test --manifest-path Cargo.toml --workspace
npm run build --prefix webui
git diff --check
```

如果只改文档，可以至少跑 `git diff --check` 并确认链接路径存在。

## 底层开发入口

`runtime-install` 仍然保留为底层开发入口，适合调试单个参数或写测试。正式 release 不直接手写它，避免漏掉 Compose/buildx/BuildKit/LXC template 这类发布必备内容。

示例：

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target base \
  --cgroup-mode v1 \
  --output out/debug-achost-base \
  --zip out/debug-achost-base.zip
```

## out 目录清理

`out/` 是生成目录，但不要把整个 `out/` 当垃圾随手清空；`out/assets/` 里通常放着复用的大型资产。重打包优先用：

```bash
scripts/package-all.sh --version 0.1.4 --clean-output
```

不要把 `out/` 里的旧 zip 当成最新发布物；每次发布前都要重新生成、校验 zip 内容并记录 sha256。
