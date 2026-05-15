#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"$SCRIPT_DIR/package-runtime.sh" base "$@"
"$SCRIPT_DIR/package-runtime.sh" docker "$@"
"$SCRIPT_DIR/package-runtime.sh" lxc "$@"
