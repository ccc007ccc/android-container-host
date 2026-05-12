---
name: xt-qtaguid-container-safe
kernel_family: linux-4.19
status: placeholder
default_enabled: false
risk: high
affected_files: net/netfilter/xt_qtaguid.c, net/netfilter/xt_qtaguid_internal.h
summary: research placeholder for container-safe qtaguid lifecycle and namespace handling
---

# xt_qtaguid container-safe patch

## Problem

Older Android `xt_qtaguid` code tracks interface stats globally by name and stores raw `struct net_device *` pointers. Container veth devices and network namespaces can stress unregister/re-register paths and may make Android traffic accounting unsafe or inaccurate.

## Affected files

- `net/netfilter/xt_qtaguid.c`
- `net/netfilter/xt_qtaguid_internal.h`

## Kernel versions

Linux 4.19 Android vendor kernels with `CONFIG_NETFILTER_XT_MATCH_QTAGUID` support.

## Required/optional status

Optional and currently not runnable. This module is a research placeholder because the current lmi config does not enable qtaguid and has `CONFIG_NETFILTER_XT_MATCH_OWNER=y`, which conflicts with qtaguid's Kconfig dependency.

## ABI impact

Not implemented yet.

## Android framework impact

A future patch must avoid silently breaking Android traffic accounting. A fallback that disables qtaguid stats must be separate and documented as sacrificing traffic stats accuracy.

## Validation

Future validation must include netdevice unregister races, veth/netns lifecycle, Android data usage accounting, and Docker/LXC network stress tests.

## Rollback

No target changes are made by this placeholder. Future patches must support `git apply -R` rollback.

## Known risks

The correct fix likely requires net namespace association, netdevice lifetime handling, and careful proc/stat behavior. It should not be applied automatically.
