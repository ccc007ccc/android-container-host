---
name: cgroup-noprefix-compat
kernel_family: linux-4.19
status: ready
default_enabled: true
risk: medium
affected_files: kernel/cgroup/cgroup.c
summary: add cpuset.cpus and cpuset.mems compatibility links when cpuset is mounted with noprefix
---

# cgroup noprefix compatibility

## Problem

Android 4.19 cpuset legacy mounting uses `cpuset,noprefix`, so files such as `cpus` and `mems` exist without the ordinary Linux `cpuset.` prefix. Some OCI/LXC tooling expects `cpuset.cpus` and `cpuset.mems`.

## Affected files

- `kernel/cgroup/cgroup.c`

## Kernel versions

Designed for Android vendor kernels in the Linux 4.19 family with cgroup v1 cpuset `noprefix` support.

## Required/optional status

Optional compatibility patch. It is useful for Docker/runc/LXC cpuset file-name compatibility but should only be applied after dry-run and build validation.

## ABI impact

Adds compatibility kernfs links for `cpuset.cpus` and `cpuset.mems` when the cgroup root uses `CGRP_ROOT_NOPREFIX`. Existing `cpus` and `mems` files remain unchanged.

## Android framework impact

Android's existing cpuset mount behavior is preserved. The patch does not remove `noprefix` and does not rename existing files.

## Validation

- `git apply --check patches/linux-4.19/cgroup-noprefix-compat.patch`
- Build the kernel.
- On device, check both `cpus`/`mems` and `cpuset.cpus`/`cpuset.mems` under the cpuset cgroup mount.
- Run Docker/runc or LXC cpuset probes.

## Rollback

Use `git apply -R patches/linux-4.19/cgroup-noprefix-compat.patch` from the target kernel tree before committing any target changes.

## Known risks

This relies on kernfs links in cgroupfs. It should be tested on-device because Android cgroup mount layouts vary by ROM.
