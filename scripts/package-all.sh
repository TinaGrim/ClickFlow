#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

./scripts/package-macos.sh "$@"

if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    ./scripts/package-windows.sh "$@"
else
    echo "Skipping Windows build: mingw-w64 is not installed." >&2
fi

(
    cd dist
    shasum -a 256 ClickFlow-*.dmg ClickFlow-*.zip > SHA256SUMS.txt
)

echo
echo "Release artifacts:"
ls -lh dist/ClickFlow-*.dmg dist/ClickFlow-*.zip dist/SHA256SUMS.txt
