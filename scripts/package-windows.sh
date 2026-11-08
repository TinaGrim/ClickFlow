#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
DIST="$ROOT/dist"
STAGING="$DIST/windows-staging"
TARGET="x86_64-pc-windows-gnu"
ZIP="$DIST/ClickFlow-$VERSION-windows-x64.zip"

if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    echo "x86_64-w64-mingw32-gcc is required. Install mingw-w64 first." >&2
    exit 1
fi

if command -v rustup >/dev/null 2>&1 && rustup target list --installed | grep -qx "$TARGET"; then
    CARGO=(cargo)
elif [ -x "$HOME/.cargo/bin/cargo" ] && "$HOME/.cargo/bin/rustup" target list --installed | grep -qx "$TARGET"; then
    CARGO=("$HOME/.cargo/bin/cargo")
else
    echo "Rust target $TARGET is not installed. Install it with rustup." >&2
    exit 1
fi

echo "Building ClickFlow $VERSION for Windows x64…"
"${CARGO[@]}" build --release --target "$TARGET"

rm -rf "$STAGING" "$ZIP"
mkdir -p "$STAGING"

cp "target/$TARGET/release/auto-clicker.exe" "$STAGING/ClickFlow.exe"
cp assets/fonts/OFL.txt "$STAGING/FONT-LICENSE.txt"
cp packaging/README-Windows.txt "$STAGING/README.txt"

(
    cd "$STAGING"
    zip -9 -q "$ZIP" ClickFlow.exe README.txt FONT-LICENSE.txt
)

rm -rf "$STAGING"
echo "Done: $ZIP"
