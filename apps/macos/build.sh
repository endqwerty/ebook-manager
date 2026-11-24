#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MODE="${1:-app}"

if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi

RUST_LIB_DIR="$SCRIPT_DIR/build/rust"
FFI_INCLUDE_DIR="$SCRIPT_DIR/Sources/ebook_bridgeFFI/include"
BRIDGE_SWIFT_DIR="$SCRIPT_DIR/Sources/EbookBridge"
BINDGEN_TMP="$SCRIPT_DIR/build/bindgen"

mkdir -p "$RUST_LIB_DIR" "$FFI_INCLUDE_DIR" "$BRIDGE_SWIFT_DIR" "$BINDGEN_TMP"

generate_bindings() {
    local staticlib="$1"
    cp "$staticlib" "$RUST_LIB_DIR/libebook_bridge.a"
    cargo run --manifest-path "$REPO_ROOT/Cargo.toml" -p ebook-bridge --features cli --bin uniffi-bindgen -- \
        generate --library "$staticlib" --language swift --out-dir "$BINDGEN_TMP"
    cp "$BINDGEN_TMP/ebook_bridgeFFI.h" "$FFI_INCLUDE_DIR/ebook_bridgeFFI.h"
    cp "$BINDGEN_TMP/ebook_bridgeFFI.modulemap" "$FFI_INCLUDE_DIR/module.modulemap"
    cp "$BINDGEN_TMP/ebook_bridge.swift" "$BRIDGE_SWIFT_DIR/ebook_bridge.swift"
}

case "$MODE" in
    app)
        cargo build --manifest-path "$REPO_ROOT/Cargo.toml" --release -p ebook-bridge
        cargo build --manifest-path "$REPO_ROOT/Cargo.toml" --release -p ebook-reader --bin ebook-reader-helper
        generate_bindings "$REPO_ROOT/target/release/libebook_bridge.a"

        swift build -c release --package-path "$SCRIPT_DIR"
        BIN_DIR="$(swift build -c release --package-path "$SCRIPT_DIR" --show-bin-path)"

        APP_DIR="$SCRIPT_DIR/build/Ebook Manager.app"
        MACOS_DIR="$APP_DIR/Contents/MacOS"
        RESOURCES_DIR="$APP_DIR/Contents/Resources"
        mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

        cp "$SCRIPT_DIR/Info.plist" "$APP_DIR/Contents/Info.plist"
        cp "$BIN_DIR/EbookManager" "$MACOS_DIR/EbookManager"
        cp "$REPO_ROOT/target/release/ebook-reader-helper" "$MACOS_DIR/ebook-reader-helper"
        chmod +x "$MACOS_DIR/EbookManager" "$MACOS_DIR/ebook-reader-helper"

        cp "$REPO_ROOT/apps/desktop/src/reader/reader.js" "$RESOURCES_DIR/reader.js"
        cp "$REPO_ROOT/apps/desktop/src/reader/reader.css" "$RESOURCES_DIR/reader.css"

        codesign --force -s - "$MACOS_DIR/ebook-reader-helper"
        codesign --force -s - "$APP_DIR"
        echo "Built $APP_DIR"
        ;;
    test)
        cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p ebook-bridge
        cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p ebook-reader --bin ebook-reader-helper
        generate_bindings "$REPO_ROOT/target/debug/libebook_bridge.a"

        EBOOK_READER_HELPER="$REPO_ROOT/target/debug/ebook-reader-helper" \
            swift test --package-path "$SCRIPT_DIR"
        ;;
    unit-test)
        swift test --package-path "$SCRIPT_DIR" --filter EbookShellCoreTests
        ;;
    *)
        echo "Usage: $0 [app|test|unit-test]" >&2
        exit 2
        ;;
esac
