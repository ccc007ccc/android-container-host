#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
URL="${MOBY_CHECK_CONFIG_URL:-https://raw.githubusercontent.com/moby/moby/master/contrib/check-config.sh}"
OUTPUT="$ROOT_DIR/check-config.sh"

python3 - "$URL" "$OUTPUT" <<'PY'
import sys
import urllib.request
from pathlib import Path

url = sys.argv[1]
output = Path(sys.argv[2])
with urllib.request.urlopen(url, timeout=30) as response:
    data = response.read()
output.write_bytes(data)
PY

chmod +x "$OUTPUT"
printf 'Fetched %s\n' "$OUTPUT"
