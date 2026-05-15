# 构建与打包

本文说明如何从 fresh checkout 生成 `achost-base.zip`、`achost-docker.zip` 和 `achost-lxc.zip`。

## PC 依赖

需要：

- Python 3。
- Rust toolchain 和 `cargo`。
- Android NDK，提供 `aarch64-linux-android*-clang`。
- Node.js/npm，用于构建 WebUI。
- Docker Android/arm64 静态资产。
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

## 准备资产

Docker 至少需要包含：

```text
docker
dockerd
containerd
containerd-shim-runc-v2
ctr
runc
```

可选：

```text
docker-init
docker-proxy
docker-compose 或 Docker compose plugin
docker-buildx 或 Docker buildx plugin
buildctl
buildkitd
```

LXC asset 需要 Android/arm64 可执行的 LXC userland，例如：

```text
lxc-start
lxc-stop
lxc-attach
lxc-info
lxc-ls
lxc-destroy
lxc-create
lxc-checkconfig
share/lxc/templates/lxc-download
```

rootfs tarball 不进入 LXC 模块包。把 rootfs 放到设备路径后，用 `achost-lxc-runtime import-rootfs` 或 LXC WebUI 导入。

建议给所有资产准备 sha256：

```bash
sha256sum /path/to/docker-static-aarch64.tgz
sha256sum /path/to/lxc-userland-aarch64.tar.gz
```

## 生成 base 模块

PC，在仓库根目录：

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target base \
  --cgroup-mode v1 \
  --output out/achost-base \
  --zip out/achost-base.zip
```

base 模块不接受 Docker/LXC asset。

## 生成 Docker 模块

PC，在仓库根目录：

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target docker \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/achost-docker \
  --zip out/achost-docker.zip
```

带可选资产：

```bash
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
```

## 生成 LXC 模块

PC，在仓库根目录：

```bash
PYTHONPATH=$PWD python3 -m achost.cli runtime-install \
  --mode kernelsu-module \
  --module-target lxc \
  --cgroup-mode v1 \
  --lxc-asset /path/to/lxc-userland-aarch64.tar.gz \
  --lxc-sha256 <sha256> \
  --output out/achost-lxc \
  --zip out/achost-lxc.zip
```

安装器会确保 LXC template 文件可执行，并导出 `LXC_TEMPLATE_PATH`。

## 输出物检查

PC：

```bash
ls -lh out/achost-base.zip out/achost-docker.zip out/achost-lxc.zip
unzip -l out/achost-docker.zip | grep -E 'achost/bin/(docker|dockerd|containerd|achost-docker-runtime)'
unzip -l out/achost-lxc.zip | grep -E 'achost/(bin/achost-lxc-runtime|lxc/bin/lxc-start|lxc/share/lxc/templates/lxc-download)'
```

## 本地验证

PC：

```bash
python3 tests/test_runtime_install.py
python3 tests/test_runtime_test.py
cargo fmt --manifest-path Cargo.toml --all --check
cargo test --manifest-path Cargo.toml --workspace
npm run build --prefix webui
git diff --check
```

如果只改文档，可以至少跑 `git diff --check` 并确认链接路径存在。

## out 目录清理

`out/` 是生成目录，可以删除旧模块输出后重打包：

```bash
rm -rf out/achost-base out/achost-docker out/achost-lxc \
  out/achost-base.zip out/achost-docker.zip out/achost-lxc.zip
```

不要把 `out/` 里的旧 zip 当成最新发布物；每次发布前重新生成并记录 sha256。
