# ACHost 文档索引

本文档面向三类人：只想安装使用的人、要打包模块的人、要适配或编译内核的人。

## 推荐阅读顺序

1. [`../README.md`](../README.md)
   - 项目定位、模块组成、最短路径和已验证基线。
2. [`install-and-upgrade.md`](install-and-upgrade.md)
   - 安装顺序、升级行为、卸载行为、哪些数据会保留。
3. [`runtime-usage.md`](runtime-usage.md)
   - Android 设备上怎么启动/停止 Docker、怎么导入/管理 LXC、怎么用 WebUI。
4. [`diagnostics.md`](diagnostics.md)
   - 启动失败、停止残留、网络、cgroup、Docker stats、LXC template/rootfs 的排障树。
5. [`build-and-package.md`](build-and-package.md)
   - 从 fresh checkout 到三个 KernelSU 模块 zip。
6. [`device-compatibility.md`](device-compatibility.md)
   - 新机型是否适合 ACHost，如何分级支持程度。
7. [`kernel-build.md`](kernel-build.md)
   - 如何检查和编译自己的内核，让它满足 Docker/LXC 宿主能力。

## 命令运行位置约定

文档里的命令会标注语境：

- **PC**：在本仓库根目录运行。
- **adb shell**：普通 Android shell。
- **su shell**：已经进入 root shell 后运行；等价于 `adb shell` 后执行 `su`。
- **模块路径**：`/data/adb/modules/<module-id>/achost`。
- **持久数据路径**：`/data/adb/achost`。

## 当前维护边界

```text
achost-base    公共 runtime、supervisor、网络/诊断/runtime-test
achost-docker  Docker/containerd/runc、Docker runtime、Docker WebUI
achost-lxc     LXC userland/runtime、LXC WebUI、rootfs 导入
```

不要把 Docker 和 LXC 混成一个不可拆模块。Docker 必须不依赖 LXC 独立可用；LXC rootfs 不进入模块包，统一从设备路径导入。

## 文档维护原则

- README 只写入口和最短路径。
- 安装/升级/卸载写在 `install-and-upgrade.md`。
- 构建和资产准备写在 `build-and-package.md`。
- 日常命令写在 `runtime-usage.md`。
- 故障树和日志解释写在 `diagnostics.md`。
- 内核能力和新机型适配写在 `device-compatibility.md` 与 `kernel-build.md`。

如果 runtime 行为、模块边界、默认路径、测试脚本或 WebUI API 变化，优先同步这些专题文档，而不是把所有内容塞回 README。
