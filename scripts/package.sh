#!/usr/bin/env bash
# Build Microcosm for players on this machine and pack it into dist/:
#   Linux:   dist/Microcosm-<version>-linux.zip     (microcosm + README)
#   macOS:   dist/Microcosm-<version>-macos.zip     (Microcosm.app + README)
#   Windows: dist/Microcosm-<version>-windows.zip   (Microcosm.exe + README; run in Git Bash)
# Upload a zip to itch.io (or use butler: see docs/RELEASING.md).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
case "$(uname -s)" in
    Linux*) os=linux ;;
    Darwin*) os=macos ;;
    MINGW* | MSYS* | CYGWIN*) os=windows ;;
    *) echo "unknown system $(uname -s)" >&2; exit 1 ;;
esac

cargo build --profile dist -p evo_app

name="Microcosm-$version-$os"
out="dist/$name"
rm -rf "$out" "dist/$name.zip"
mkdir -p "$out"
cp packaging/README.txt "$out/README.txt"
cp crates/evo_app/assets/fonts/Kalam-OFL.txt "$out/"

case "$os" in
    linux)
        cp target/dist/evo_app "$out/microcosm"
        chmod +x "$out/microcosm"
        ;;
    windows)
        cp target/dist/evo_app.exe "$out/Microcosm.exe"
        ;;
    macos)
        app="$out/Microcosm.app/Contents"
        mkdir -p "$app/MacOS"
        cp target/dist/evo_app "$app/MacOS/Microcosm"
        sed "s/@VERSION@/$version/g" packaging/Info.plist > "$app/Info.plist"
        ;;
esac

(cd dist && if command -v zip >/dev/null; then
    zip -qr "$name.zip" "$name"
else
    7z a -tzip "$name.zip" "$name" >/dev/null
fi)
echo "dist/$name.zip"
