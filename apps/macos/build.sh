#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP_DIR="$ROOT/apps/macos/build/Kite.app"
CONTENTS="$APP_DIR/Contents"
GEN_DIR="$ROOT/apps/macos/Generated"
PROFILE="${KITE_BUILD_PROFILE:-release}"

case "$PROFILE" in
    release)
        TARGET_DIR="$ROOT/target/release"
        ;;
    debug)
        TARGET_DIR="$ROOT/target/debug"
        ;;
    *)
        echo "Unsupported KITE_BUILD_PROFILE: $PROFILE" >&2
        exit 2
        ;;
esac

cd "$ROOT"
if [[ "${KITE_SKIP_KENSAKU:-0}" != "1" ]]; then
    if [[ -z "${KENSAKU_CLI_PATH:-}" || -z "${KENSAKU_DAEMON_PATH:-}" ]]; then
        python3 scripts/kensaku/build_sidecars.py
    fi
fi
if [[ "$PROFILE" == "release" ]]; then
    cargo build --release --locked --lib
else
    cargo build --locked --lib
fi

rm -rf "$GEN_DIR" "$APP_DIR"
mkdir -p "$GEN_DIR" "$CONTENTS/MacOS" "$CONTENTS/Frameworks" "$CONTENTS/Resources"

if [[ "$PROFILE" == "release" ]]; then
    cargo run --release --locked --features uniffi-bindgen --bin uniffi-bindgen -- \
        generate \
        --library "$TARGET_DIR/libkite_core.dylib" \
        --language swift \
        --out-dir "$GEN_DIR"
else
    cargo run --locked --features uniffi-bindgen --bin uniffi-bindgen -- \
        generate \
        --library "$TARGET_DIR/libkite_core.dylib" \
        --language swift \
        --out-dir "$GEN_DIR"
fi

xcrun swiftc \
    -O \
    -target arm64-apple-macos13.0 \
    -parse-as-library \
    -I "$GEN_DIR" \
    -Xcc -fmodule-map-file="$GEN_DIR/kite_coreFFI.modulemap" \
    -Xcc -I"$GEN_DIR" \
    -L "$TARGET_DIR" \
    -lkite_core \
    -framework AppKit \
    -framework QuickLookUI \
    -Xlinker -rpath \
    -Xlinker @executable_path/../Frameworks \
    "$ROOT"/apps/macos/Sources/*.swift \
    "$GEN_DIR"/kite_core.swift \
    -o "$CONTENTS/MacOS/Kite"

cp "$ROOT/apps/macos/Info.plist" "$CONTENTS/Info.plist"
cp "$ROOT/assets/kite.icns" "$CONTENTS/Resources/AppIcon.icns"
cp "$TARGET_DIR/libkite_core.dylib" "$CONTENTS/Frameworks/"

KENSAKU_CLI="${KENSAKU_CLI_PATH:-$ROOT/vendor/kensaku/kensaku-cli}"
KENSAKU_DAEMON="${KENSAKU_DAEMON_PATH:-$ROOT/vendor/kensaku/kensaku-daemon}"
if [[ "${KITE_SKIP_KENSAKU:-0}" != "1" ]]; then
    if [[ ! -f "$KENSAKU_CLI" || ! -f "$KENSAKU_DAEMON" ]]; then
        echo "Kensaku sidecars are missing; run scripts/kensaku/build_sidecars.py" >&2
        exit 1
    fi
    cp "$KENSAKU_CLI" "$CONTENTS/MacOS/kensaku-cli"
    cp "$KENSAKU_DAEMON" "$CONTENTS/MacOS/kensaku-daemon"
    chmod +x "$CONTENTS/MacOS/kensaku-cli" "$CONTENTS/MacOS/kensaku-daemon"
    cp "$ROOT/docs/search/kensaku-backend.md" "$CONTENTS/Resources/Kensaku-backend.md"
    cp "$ROOT/docs/search/KENSAKU_LICENSE" "$CONTENTS/Resources/KENSAKU_LICENSE"
fi
install_name_tool -id @rpath/libkite_core.dylib "$CONTENTS/Frameworks/libkite_core.dylib"

CORE_LINK="$(otool -L "$CONTENTS/MacOS/Kite" | awk '/libkite_core\.dylib/ { print $1; exit }')"
if [ -n "$CORE_LINK" ] && [ "$CORE_LINK" != "@rpath/libkite_core.dylib" ]; then
    install_name_tool -change "$CORE_LINK" @rpath/libkite_core.dylib "$CONTENTS/MacOS/Kite"
fi

codesign --force --deep --sign - "$APP_DIR"

echo "$APP_DIR"
