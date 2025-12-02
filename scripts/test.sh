#!/bin/bash
#===============================================================================
# HyperCalibrate - Environment Check & Test Script
# Verifies build artifacts and system requirements
#===============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

echo "🧪 HyperCalibrate Environment Check"
echo "═══════════════════════════════════════════════════════════════"
echo ""

#-------------------------------------------------------------------------------
# Check for binary in all possible locations
#-------------------------------------------------------------------------------
echo "📦 Checking for binary..."

BINARY=""
BINARY_LOCATIONS=(
    "./dist/hypercalibrate"
    "./hypercalibrate"
    "./target/release/hypercalibrate"
    "./target/debug/hypercalibrate"
    "./target/aarch64-unknown-linux-gnu/release/hypercalibrate"
    "/usr/local/bin/hypercalibrate"
)

for loc in "${BINARY_LOCATIONS[@]}"; do
    if [ -f "$loc" ]; then
        BINARY="$loc"
        SIZE=$(du -h "$BINARY" | cut -f1)
        echo "   ✅ Found: $loc ($SIZE)"

        # Check architecture
        if command -v file &> /dev/null; then
            ARCH=$(file "$BINARY" | grep -oE '(x86-64|aarch64|ARM|32-bit|64-bit)' | head -1)
            echo "      Architecture: $ARCH"
        fi
    fi
done

if [ -z "$BINARY" ]; then
    echo "   ❌ No binary found"
    echo ""
    echo "   Build with one of:"
    echo "      ./docker-build.sh            # Cross-compile for Raspberry Pi (recommended)"
    echo "      ./scripts/local-build.sh     # Build natively"
    echo "      cargo build --release    # Build for current platform"
    echo ""
fi

#-------------------------------------------------------------------------------
# Check system requirements
#-------------------------------------------------------------------------------
echo ""
echo "🔧 System Requirements..."

# Check for v4l2loopback
echo ""
echo "   v4l2loopback module:"
if lsmod 2>/dev/null | grep -q v4l2loopback; then
    echo "      ✅ Loaded"
    # Show current configuration
    if [ -f /sys/module/v4l2loopback/parameters/video_nr ]; then
        VIDEO_NR=$(cat /sys/module/v4l2loopback/parameters/video_nr 2>/dev/null || echo "unknown")
        echo "      Video device: /dev/video$VIDEO_NR"
    fi
else
    echo "      ⚠️  Not loaded"
    echo "      To load: sudo modprobe v4l2loopback devices=1 video_nr=10 card_label=\"HyperCalibrate\" exclusive_caps=0"
fi

# Check for v4l-utils
echo ""
echo "   v4l-utils:"
if command -v v4l2-ctl &> /dev/null; then
    echo "      ✅ Installed"
else
    echo "      ⚠️  Not installed (optional, for debugging)"
    echo "      To install: sudo apt install v4l-utils"
fi

# Check for Docker (for building)
echo ""
echo "   Docker (for cross-compilation):"
if command -v docker &> /dev/null; then
    DOCKER_VERSION=$(docker --version 2>/dev/null | cut -d' ' -f3 | tr -d ',')
    echo "      ✅ Installed (v$DOCKER_VERSION)"
else
    echo "      ⚠️  Not installed (needed for docker-build.sh)"
fi

#-------------------------------------------------------------------------------
# List video devices
#-------------------------------------------------------------------------------
echo ""
echo "📹 Video Devices..."

if command -v v4l2-ctl &> /dev/null; then
    DEVICES=$(v4l2-ctl --list-devices 2>/dev/null || true)
    if [ -n "$DEVICES" ]; then
        echo "$DEVICES" | sed 's/^/   /'
    else
        echo "   No video devices found"
    fi
else
    if ls /dev/video* &> /dev/null; then
        echo "   Available: $(ls /dev/video* 2>/dev/null | tr '\n' ' ')"
    else
        echo "   No video devices found"
    fi
fi

#-------------------------------------------------------------------------------
# Show binary help if available
#-------------------------------------------------------------------------------
if [ -n "$BINARY" ]; then
    # Only show help if binary is for current architecture
    CURRENT_ARCH=$(uname -m)
    BINARY_INFO=$(file "$BINARY" 2>/dev/null || echo "")

    CAN_RUN=false
    if [[ "$CURRENT_ARCH" == "x86_64" && "$BINARY_INFO" == *"x86-64"* ]]; then
        CAN_RUN=true
    elif [[ "$CURRENT_ARCH" == "aarch64" && "$BINARY_INFO" == *"aarch64"* ]]; then
        CAN_RUN=true
    elif [[ "$CURRENT_ARCH" == "armv7l" && "$BINARY_INFO" == *"ARM"* ]]; then
        CAN_RUN=true
    fi

    if [ "$CAN_RUN" = true ]; then
        echo ""
        echo "📋 Binary Options:"
        echo "───────────────────────────────────────────────────────────────"
        $BINARY --help 2>/dev/null | sed 's/^/   /' || echo "   (Could not run binary)"
    else
        echo ""
        echo "ℹ️  Binary is cross-compiled for a different architecture"
        echo "   Current system: $CURRENT_ARCH"
        echo "   Binary target:  $(echo "$BINARY_INFO" | grep -oE '(x86-64|aarch64|ARM)' | head -1 || echo "unknown")"
    fi
fi

#-------------------------------------------------------------------------------
# Summary
#-------------------------------------------------------------------------------
echo ""
echo "═══════════════════════════════════════════════════════════════"

if [ -n "$BINARY" ]; then
    echo "✅ Ready to deploy!"
    echo ""
    echo "   Deploy to Raspberry Pi:"
    echo "      ./deploy.sh <raspberry-pi-ip>"
    echo ""
    echo "   Or run locally (if on Pi):"
    echo "      $BINARY --input /dev/video0 --output /dev/video10"
    echo "      Then open http://localhost:8091"
else
    echo "⚠️  Build required before deployment"
    echo ""
    echo "   ./docker-build.sh    # Recommended for cross-compilation"
fi
echo ""
