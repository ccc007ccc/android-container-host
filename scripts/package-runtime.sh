#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
    printf 'package-runtime.sh requires module target: base, docker, or lxc\n' >&2
    exit 2
fi
shift

case "$TARGET" in
    base|docker|lxc) ;;
    *)
        printf 'unsupported module target: %s\n' "$TARGET" >&2
        exit 2
        ;;
esac

VERSION="$(PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 - <<'PY'
from achost import __version__
print(__version__)
PY
)"
VERSION_CODE="$(PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 - <<'PY'
from achost import __version_code__
print(__version_code__)
PY
)"
ASSETS_DIR="$PROJECT_ROOT/out/assets"
OUT_ROOT=""
CLEAN_OUTPUT=0
DRY_RUN=0

DOCKER_ASSET="$ASSETS_DIR/docker-29.4.3.tgz"
DOCKER_SHA256="fdde3dbedbd8dc9e9e54a3172e682b4e69e740296e580542ea9ceacebbbf4f4c"
COMPOSE_ASSET="$ASSETS_DIR/docker-compose-linux-aarch64"
COMPOSE_SHA256="e8105a3e687ea7e0b0f81abe4bf9269c8a2801fb72c2b498b5ff2472bc54145f"
BUILDX_ASSET="$ASSETS_DIR/buildx-v0.33.0.linux-arm64"
BUILDX_SHA256="204dc28447d3bb48f42ed1ce5747e0885cd57e306506a39029311becdb1ef786"
BUILDKIT_ASSET="$ASSETS_DIR/buildkit-v0.29.0.linux-arm64.tar.gz"
BUILDKIT_SHA256="99a279e30be2947294eece98d82d1461fcfdc47da59514cb85252bb5ef414801"
LXC_ASSET="$ASSETS_DIR/lxc-userland-android-arm64-lxc-3.1.0.tar.gz"
LXC_SHA256="8c3010f88b52472bf77a896e75df57c293b6172b050b6c906ff1e4eff213d504"

DOCKER_ASSET_CUSTOM=0
DOCKER_SHA_CUSTOM=0
COMPOSE_ASSET_CUSTOM=0
COMPOSE_SHA_CUSTOM=0
BUILDX_ASSET_CUSTOM=0
BUILDX_SHA_CUSTOM=0
BUILDKIT_ASSET_CUSTOM=0
BUILDKIT_SHA_CUSTOM=0
LXC_ASSET_CUSTOM=0
LXC_SHA_CUSTOM=0

usage() {
    cat <<'EOF'
Usage: package-runtime.sh <base|docker|lxc> [options]

Options:
  --version VERSION          Package version, default from achost.__version__
  --version-code CODE       Android module versionCode, default from achost.__version_code__
  --out-root DIR            Output root, default out/achost-v<VERSION>
  --assets-dir DIR          Asset directory, default out/assets
  --clean-output            Remove this target's stage dir and zip under out before packaging
  --dry-run                 Print runtime-install and runtime-validate commands without running

Docker asset overrides require matching checksum overrides:
  --docker-asset FILE       --docker-sha256 SHA256
  --compose-asset FILE      --compose-sha256 SHA256
  --buildx-asset FILE       --buildx-sha256 SHA256
  --buildkit-asset FILE     --buildkit-sha256 SHA256

LXC asset override requires matching checksum override:
  --lxc-asset FILE          --lxc-sha256 SHA256
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --version-code)
            VERSION_CODE="$2"
            shift 2
            ;;
        --out-root)
            OUT_ROOT="$2"
            shift 2
            ;;
        --assets-dir)
            ASSETS_DIR="$2"
            DOCKER_ASSET="$ASSETS_DIR/docker-29.4.3.tgz"
            COMPOSE_ASSET="$ASSETS_DIR/docker-compose-linux-aarch64"
            BUILDX_ASSET="$ASSETS_DIR/buildx-v0.33.0.linux-arm64"
            BUILDKIT_ASSET="$ASSETS_DIR/buildkit-v0.29.0.linux-arm64.tar.gz"
            LXC_ASSET="$ASSETS_DIR/lxc-userland-android-arm64-lxc-3.1.0.tar.gz"
            shift 2
            ;;
        --docker-asset)
            DOCKER_ASSET="$2"
            DOCKER_ASSET_CUSTOM=1
            shift 2
            ;;
        --docker-sha256)
            DOCKER_SHA256="$2"
            DOCKER_SHA_CUSTOM=1
            shift 2
            ;;
        --compose-asset)
            COMPOSE_ASSET="$2"
            COMPOSE_ASSET_CUSTOM=1
            shift 2
            ;;
        --compose-sha256)
            COMPOSE_SHA256="$2"
            COMPOSE_SHA_CUSTOM=1
            shift 2
            ;;
        --buildx-asset)
            BUILDX_ASSET="$2"
            BUILDX_ASSET_CUSTOM=1
            shift 2
            ;;
        --buildx-sha256)
            BUILDX_SHA256="$2"
            BUILDX_SHA_CUSTOM=1
            shift 2
            ;;
        --buildkit-asset)
            BUILDKIT_ASSET="$2"
            BUILDKIT_ASSET_CUSTOM=1
            shift 2
            ;;
        --buildkit-sha256)
            BUILDKIT_SHA256="$2"
            BUILDKIT_SHA_CUSTOM=1
            shift 2
            ;;
        --lxc-asset)
            LXC_ASSET="$2"
            LXC_ASSET_CUSTOM=1
            shift 2
            ;;
        --lxc-sha256)
            LXC_SHA256="$2"
            LXC_SHA_CUSTOM=1
            shift 2
            ;;
        --clean-output)
            CLEAN_OUTPUT=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -z "$OUT_ROOT" ]]; then
    OUT_ROOT="$PROJECT_ROOT/out/achost-v$VERSION"
fi

MODULE_NAME="achost-$TARGET"
STAGE_DIR="$OUT_ROOT/$MODULE_NAME"
ZIP_PATH="$OUT_ROOT/$MODULE_NAME-v$VERSION.zip"
SHA256SUMS="$OUT_ROOT/SHA256SUMS.txt"

run_achost() {
    PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}" python3 -m achost.cli "$@"
}

shell_quote_command() {
    printf 'PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=%q python3 -m achost.cli' "$PROJECT_ROOT${PYTHONPATH:+:$PYTHONPATH}"
    for arg in "$@"; do
        printf ' %q' "$arg"
    done
    printf '\n'
}

require_matching_sha() {
    local label="$1"
    local asset_custom="$2"
    local sha_custom="$3"
    if [[ "$asset_custom" == "1" && "$sha_custom" != "1" ]]; then
        printf '%s asset override requires matching sha256 override\n' "$label" >&2
        exit 2
    fi
}

require_asset_file() {
    local label="$1"
    local path="$2"
    if [[ ! -f "$path" ]]; then
        printf '%s asset not found: %s\n' "$label" "$path" >&2
        exit 2
    fi
}

if [[ "$DRY_RUN" != "1" ]]; then
    case "$TARGET" in
        docker)
            require_asset_file docker "$DOCKER_ASSET"
            require_asset_file compose "$COMPOSE_ASSET"
            require_asset_file buildx "$BUILDX_ASSET"
            require_asset_file buildkit "$BUILDKIT_ASSET"
            require_matching_sha docker "$DOCKER_ASSET_CUSTOM" "$DOCKER_SHA_CUSTOM"
            require_matching_sha compose "$COMPOSE_ASSET_CUSTOM" "$COMPOSE_SHA_CUSTOM"
            require_matching_sha buildx "$BUILDX_ASSET_CUSTOM" "$BUILDX_SHA_CUSTOM"
            require_matching_sha buildkit "$BUILDKIT_ASSET_CUSTOM" "$BUILDKIT_SHA_CUSTOM"
            ;;
        lxc)
            require_asset_file lxc "$LXC_ASSET"
            require_matching_sha lxc "$LXC_ASSET_CUSTOM" "$LXC_SHA_CUSTOM"
            ;;
    esac
fi

INSTALL_CMD=(
    runtime-install
    --mode kernelsu-module
    --module-target "$TARGET"
    --version "$VERSION"
    --version-code "$VERSION_CODE"
    --cgroup-mode v1
    --output "$STAGE_DIR"
    --zip "$ZIP_PATH"
)

case "$TARGET" in
    docker)
        INSTALL_CMD+=(
            --docker-asset "$DOCKER_ASSET"
            --docker-sha256 "$DOCKER_SHA256"
            --compose-asset "$COMPOSE_ASSET"
            --compose-sha256 "$COMPOSE_SHA256"
            --buildx-asset "$BUILDX_ASSET"
            --buildx-sha256 "$BUILDX_SHA256"
            --buildkit-asset "$BUILDKIT_ASSET"
            --buildkit-sha256 "$BUILDKIT_SHA256"
        )
        ;;
    lxc)
        INSTALL_CMD+=(
            --lxc-asset "$LXC_ASSET"
            --lxc-sha256 "$LXC_SHA256"
        )
        ;;
esac

VALIDATE_CMD=(
    runtime-validate
    --module-target "$TARGET"
    --package-root "$STAGE_DIR"
    --zip "$ZIP_PATH"
    --release
)

if [[ "$DRY_RUN" == "1" ]]; then
    shell_quote_command "${INSTALL_CMD[@]}"
    shell_quote_command "${VALIDATE_CMD[@]}"
    exit 0
fi

if [[ "$CLEAN_OUTPUT" == "1" ]]; then
    case "$STAGE_DIR" in
        "$PROJECT_ROOT"/out/*) rm -rf "$STAGE_DIR" ;;
        *)
            printf 'refusing to clean stage outside project out/: %s\n' "$STAGE_DIR" >&2
            exit 2
            ;;
    esac
    case "$ZIP_PATH" in
        "$PROJECT_ROOT"/out/*) rm -f "$ZIP_PATH" ;;
        *)
            printf 'refusing to clean zip outside project out/: %s\n' "$ZIP_PATH" >&2
            exit 2
            ;;
    esac
fi

mkdir -p "$OUT_ROOT"
run_achost "${INSTALL_CMD[@]}"
run_achost "${VALIDATE_CMD[@]}"

tmp_sums="$SHA256SUMS.tmp"
if [[ -f "$SHA256SUMS" ]]; then
    grep -v "  $(basename "$ZIP_PATH")$" "$SHA256SUMS" > "$tmp_sums" || true
else
    : > "$tmp_sums"
fi
(
    cd "$(dirname "$ZIP_PATH")"
    sha256sum "$(basename "$ZIP_PATH")"
) >> "$tmp_sums"
sort -k2 "$tmp_sums" > "$SHA256SUMS"
rm -f "$tmp_sums"

printf 'package: %s\n' "$ZIP_PATH"
printf 'sha256: %s\n' "$SHA256SUMS"
