#!/usr/bin/env bash
# Build macos/build/Annotator.app (launcher + Info.plist + icon). Usage: macos/build-app.sh [version]
set -euo pipefail
cd "$(dirname "$0")"
version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' ../Cargo.toml | head -1)}"
app=build/Annotator.app
rm -rf build && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
swiftc -O -o "$app/Contents/MacOS/Annotator" main.swift
sed "s/__VERSION__/$version/g" Info.plist > "$app/Contents/Info.plist"
swift icon.swift build/Annotator.iconset >/dev/null
iconutil -c icns -o "$app/Contents/Resources/Annotator.icns" build/Annotator.iconset
plutil -lint "$app/Contents/Info.plist" >/dev/null
codesign --force --sign - "$app" 2>/dev/null || true   # ad-hoc signature, local use
echo "built $app ($version)"
