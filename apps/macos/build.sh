#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT/apps/macos/build/Kite.app"
CONTENTS="$APP_DIR/Contents"
GEN_DIR="$ROOT/apps/macos/Generated"

cd "$ROOT"
cargo build --release --locked --lib

rm -rf "$GEN_DIR" "$APP_DIR"
mkdir -p "$GEN_DIR" "$CONTENTS/MacOS" "$CONTENTS/Frameworks" "$CONTENTS/Resources"

cargo run --release --locked --features uniffi-bindgen --bin uniffi-bindgen -- \
    generate \
    --library "$ROOT/target/release/libkite_core.dylib" \
    --language swift \
    --out-dir "$GEN_DIR"

xcrun swiftc \
    -O \
    -parse-as-library \
    -I "$GEN_DIR" \
    -Xcc -fmodule-map-file="$GEN_DIR/kite_coreFFI.modulemap" \
    -Xcc -I"$GEN_DIR" \
    -L "$ROOT/target/release" \
    -lkite_core \
    -framework AppKit \
    -framework QuickLookUI \
    -Xlinker -rpath \
    -Xlinker @executable_path/../Frameworks \
    "$ROOT"/apps/macos/Sources/*.swift \
    "$GEN_DIR"/kite_core.swift \
    -o "$CONTENTS/MacOS/Kite"

cp "$ROOT/apps/macos/Info.plist" "$CONTENTS/Info.plist"
cp "$ROOT/target/release/libkite_core.dylib" "$CONTENTS/Frameworks/"
install_name_tool -id @rpath/libkite_core.dylib "$CONTENTS/Frameworks/libkite_core.dylib"
codesign --force --deep --sign - "$APP_DIR"

echo "$APP_DIR"
