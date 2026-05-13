# ACHost 文档索引

本文档目录面向项目使用者和后续适配者。

## 推荐阅读顺序

1. [`../README.md`](../README.md)
   - 项目定位、当前状态、快速检查、打包、设备使用和已知限制。
2. [`runtime-usage.md`](runtime-usage.md)
   - Android 端运行时安装、Docker native start/stop、WebUI API、runtime-test、测试清理。
3. [`device-compatibility.md`](device-compatibility.md)
   - 不同机型/内核是否适用、最小内核能力、新机型适配流程、支持程度分级。
4. [`superpowers/specs/2026-05-13-achost-rust-runtime-refactor-design.md`](superpowers/specs/2026-05-13-achost-rust-runtime-refactor-design.md)
   - 核心 runtime shell 逐步 Rust 化的设计、阶段和验证策略。

## 当前主线

Docker native runtime 是当前完成度最高、可验证闭环的路径：

```text
ACHOST_RUNTIME_MODE=native
ACHOST_USE_CHROOT=0
achost-docker-runtime start|stop
achost-runtime-core net-reconcile|net-watchdog|protect-daemons
```

split 模块边界：

```text
achost-base    公共 runtime、supervisor、runtime-test
achost-docker  Docker runtime、Docker userland、WebUI API、Docker tests
achost-lxc     LXC 配置和验证入口
```

## 文档维护原则

- README 写项目整体状态和最短可用路径。
- `runtime-usage.md` 写设备上怎么装、怎么跑、怎么测、怎么清理。
- `device-compatibility.md` 写通用性边界和新机型适配流程。
- 设计 spec 写长期重构背景，不替代用户手册。

当 runtime 行为、模块边界、默认路径或验证命令变化时，同步更新 README 和对应专题文档。
