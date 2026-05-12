# ACHKL patches

Patches are grouped by kernel family and treated as modules:

```text
patches/<kernel-family>/<name>.patch
patches/<kernel-family>/<name>.md
```

Use `bin/achost list-patches --kernel-tree /path/to/kernel` to inspect modules.

Use `bin/achost apply-patches --kernel-tree /path/to/kernel --dry-run` to run `git apply --check` without changing the target tree.

Patch modules marked `experimental` or `placeholder` are skipped by default.
