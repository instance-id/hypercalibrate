#!/bin/bash
#===============================================================================
# HyperCalibrate - Docker Cross-Compile Script
# Builds the binary for Raspberry Pi using Docker (no local toolchain needed)
#===============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Configuration
TARGET="${1:-aarch64}"  # aarch64 (64-bit, Pi 4/5) or armv7 (32-bit, older Pi)
OUTPUT_DIR="$SCRIPT_DIR/dist"

# Map target name to Rust target triple
case "$TARGET" in
    aarch64|arm64|pi4|pi5|64)
        RUST_TARGET="aarch64-unknown-linux-gnu"
        ARCH_NAME="aarch64"
        ;;
    armv7|armhf|32)
        RUST_TARGET="armv7-unknown-linux-gnueabihf"
        ARCH_NAME="armv7"
        ;;
    *)
        echo "Unknown target: $TARGET"
        echo "Usage: $0 [aarch64|armv7]"
        echo "  aarch64 - 64-bit (Raspberry Pi 4/5, default)"
        echo "  armv7   - 32-bit (Raspberry Pi 2/3, older)"
        exit 1
        ;;
esac

echo "🐳 Cross-compiling HyperCalibrate for Raspberry Pi"
echo "   Target: $RUST_TARGET ($ARCH_NAME)"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Please install Docker first."
    echo "   https://docs.docker.com/get-docker/"
    exit 1
fi

# Build the Docker image if it doesn't exist or if Dockerfile changed
IMAGE_NAME="hypercalibrate-cross"
DOCKERFILE="Dockerfile.cross"

echo "🔨 Building Docker cross-compile image..."
docker build -t "$IMAGE_NAME" -f "$DOCKERFILE" . --quiet

# Run the build
echo "🔨 Compiling for $ARCH_NAME..."
docker run --rm \
    -v "$SCRIPT_DIR:/app" \
    -v "$OUTPUT_DIR:/output" \
    -e "TARGET=$RUST_TARGET" \
    "$IMAGE_NAME"

# Check if build succeeded
BINARY="$OUTPUT_DIR/hypercalibrate"
if [ -f "$BINARY" ]; then
    # Get binary info
    SIZE=$(du -h "$BINARY" | cut -f1)

    echo ""
    echo "✅ Build successful!"
    echo "   Binary: $BINARY"
    echo "   Size: $SIZE"
    echo "   Target: $RUST_TARGET"
    echo ""
    echo "📦 Next steps:"
    echo "   1. Deploy to Raspberry Pi:"
    echo "      ./deploy.sh <raspberry-pi-ip>"
    echo ""
    echo "   2. Or manually copy:"
    echo "      scp $BINARY hyperion@<pi-ip>:/home/hyperion/"
else
    echo "❌ Build failed - binary not found"
    exit 1
fi
