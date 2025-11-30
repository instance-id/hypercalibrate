#!/bin/bash
# Build script for HyperCalibrate
# Supports native and cross-compilation for Raspberry Pi

set -e

TARGET="${1:-native}"
PROFILE="${2:-release}"

echo "🔨 Building HyperCalibrate..."
echo "   Target: $TARGET"
echo "   Profile: $PROFILE"

case "$TARGET" in
    native)
        cargo build --profile $PROFILE
        BINARY="target/$PROFILE/hypercalibrate"
        ;;

    rpi|raspberry|aarch64)
        echo "🍓 Cross-compiling for Raspberry Pi (aarch64)..."

        # Check if target is installed
        if ! rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
            echo "Installing aarch64 target..."
            rustup target add aarch64-unknown-linux-gnu
        fi

        # Check for cross-compiler
        if ! command -v aarch64-linux-gnu-gcc &> /dev/null; then
            echo "⚠️  Cross-compiler not found. Install with:"
            echo "   sudo apt install gcc-aarch64-linux-gnu"
            exit 1
        fi

        # Set up cross-compilation environment
        export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
        export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc

        cargo build --profile $PROFILE --target aarch64-unknown-linux-gnu
        BINARY="target/aarch64-unknown-linux-gnu/$PROFILE/hypercalibrate"
        ;;

    armv7|armhf)
        echo "🍓 Cross-compiling for Raspberry Pi (armv7)..."

        if ! rustup target list --installed | grep -q "armv7-unknown-linux-gnueabihf"; then
            echo "Installing armv7 target..."
            rustup target add armv7-unknown-linux-gnueabihf
        fi

        if ! command -v arm-linux-gnueabihf-gcc &> /dev/null; then
            echo "⚠️  Cross-compiler not found. Install with:"
            echo "   sudo apt install gcc-arm-linux-gnueabihf"
            exit 1
        fi

        export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
        export CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc

        cargo build --profile $PROFILE --target armv7-unknown-linux-gnueabihf
        BINARY="target/armv7-unknown-linux-gnueabihf/$PROFILE/hypercalibrate"
        ;;

    *)
        echo "Unknown target: $TARGET"
        echo "Usage: $0 [native|rpi|armv7] [release|debug]"
        exit 1
        ;;
esac

if [ -f "$BINARY" ]; then
    SIZE=$(du -h "$BINARY" | cut -f1)
    echo ""
    echo "✅ Build successful!"
    echo "   Binary: $BINARY"
    echo "   Size: $SIZE"
else
    echo "❌ Build failed - binary not found"
    exit 1
fi
