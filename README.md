# Android Container Host Kernel Layer

Android Container Host Kernel Layer (ACHKL) is an external support project for evaluating and preparing Android kernels for Docker and LXC host use.

This project does not live inside a target kernel tree. Target kernels are inputs to `achost` commands.

## Phase 1 scope

Implemented first:

- `achost detect`
- `achost plan`
- `achost verify-config`
- `achost merge-fragments`
- `scripts/verify-config.sh`
- `scripts/merge-fragments.sh`
- `scripts/inject-kconfig.sh`
- `scripts/prepare-tree.sh`
- `scripts/rollback.sh`
- `scripts/docker/verify-moby-check-config.sh`
- `achost list-patches`
- `achost apply-patches` dry-run framework
- Android runtime NAT/debug scripts
- Docker/LXC smoke test scripts
- LXC checkconfig wrapper
- lmkd/OOM protection script
- Android runtime install package generator
- Android runtime test command wrapper
- KernelSU module runtime packaging
- Docker userland asset packaging and start/stop helpers
- LXC userland asset slot and validation helpers
- minimal config fragments
- first device metadata for `xiaomi-sm8250-lmi`

Not implemented yet:

- automatic patch application
- qtaguid fixes
- target-kernel AnyKernel/ReSukiSU build integration inside this project
- full Android-compatible LXC userland selection

## Quick start

```bash
bin/achost detect \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250 \
  --out /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out

bin/achost plan \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250 \
  --out /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out \
  --device devices/xiaomi-sm8250-lmi.yml \
  --write-report

bin/achost verify-config \
  --config /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out/.config \
  --profile android-container-host-v1

scripts/merge-fragments.sh \
  --base-config /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out/.config \
  --output /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out/achost/merged.config

scripts/verify-config.sh \
  /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out/.config

scripts/inject-kconfig.sh \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250

scripts/prepare-tree.sh \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250 \
  --out /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out \
  --device devices/xiaomi-sm8250-lmi.yml

scripts/rollback.sh \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250

third_party/moby-check-config/fetch.sh
scripts/docker/verify-moby-check-config.sh \
  /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250/out/.config

bin/achost list-patches \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250

bin/achost apply-patches \
  --kernel-tree /home/ccc007/Android/Kernel/android_kernel_xiaomi_sm8250 \
  --dry-run

runtime/android/net/container-nat-manager.sh
runtime/android/net/container-network-watchdog.sh
scripts/runtime-net-debug.sh
scripts/docker/runtime-smoke-docker.sh
scripts/verify-lxc-checkconfig.sh
scripts/runtime-smoke-lxc.sh
runtime/android/memory/protect-container-daemons.sh

bin/achost runtime-install \
  --mode manual \
  --cgroup-mode v1 \
  --output out/runtime-manual

bin/achost runtime-install \
  --mode manual \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/runtime-manual-docker

bin/achost runtime-install \
  --mode kernelsu-module \
  --cgroup-mode v1 \
  --docker-asset /path/to/docker-static-aarch64.tgz \
  --docker-sha256 <sha256> \
  --output out/achost-runtime-module-docker

bin/achost runtime-test \
  --package-root /data/adb/achost \
  --target all \
  --out-dir /data/local/tmp/achost-runtime-test
```

## Docker/LXC userland assets

`runtime-install` never downloads binaries. Docker support is enabled by passing an explicit Android/arm64 Docker static tarball with `--docker-asset`; when `--docker-sha256` is supplied, the package generator verifies it before extracting `docker`, `dockerd`, `containerd`, `containerd-shim-runc-v2`, `ctr`, and `runc` into `achost/bin`. If that same Docker tarball already contains a Compose v2 CLI plugin, the package also exposes it for `docker compose`, but Docker Engine startup does not depend on Compose.

Compose, buildx, and BuildKit are explicit optional assets. Use `--compose-asset` for the Docker Compose v2 plugin, `--buildx-asset` for the Docker buildx plugin, and `--buildkit-asset` for a BuildKit tarball containing `buildctl` and `buildkitd`. Compose/buildx are installed under `achost/etc/docker/cli-plugins/` plus standalone fallbacks in `achost/bin`; BuildKit binaries are installed in `achost/bin`.

On Android 16/lmi, Docker 29 needs a writable `/run`. `achost-docker-start.sh` starts Docker inside an ACHOST-managed chroot under `/data/adb/achost/var/chroot`, so the system rootfs is not remounted or modified. `--cgroup-mode v2` also changes the runtime cgroup layout by exposing the host cgroup2 tree in the chroot; test v2 packages under a separate prefix before replacing a stable v1 package.

`runtime-test.sh` starts Docker, runs validation, and stops Docker afterward. The default Docker smoke mode is local-only: it imports a tiny image from the packaged Docker binary and runs it with `--network none`, so it does not depend on Docker Hub. Use `DOCKER_SMOKE_MODE=local-bridge` to also exercise bridge/veth attachment without external traffic, `DOCKER_SMOKE_MODE=publish` to verify real host published-port traffic through `docker-proxy`, and `DOCKER_SMOKE_MODE=full` only when registry access and image pulls are expected to work.

On-device helpers installed under `/data/adb/achost/bin`:

```bash
achost-container-validate.sh
achost-docker-start.sh
achost-docker-stop.sh
achost-lxc-validate.sh
```

LXC has an asset slot and validation path, but full LXC runtime support remains experimental until a known Android-compatible LXC userspace package is selected.
