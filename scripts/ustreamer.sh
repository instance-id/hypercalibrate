#!/bin/bash
#
# uStreamer Wrapper Script
# Configurable MJPEG streaming for USB cameras
#
# Usage:
#   ./ustreamer.sh                    # Start with default/configured settings
#   ./ustreamer.sh --help             # Show this help
#   ./ustreamer.sh --list-devices     # List available video devices
#
# Configuration:
#   Edit the settings below, or override via environment variables:
#   USTREAMER_DEVICE=/dev/video2 USTREAMER_PORT=8092 ./ustreamer.sh
#

#===============================================================================
# CONFIGURATION - Edit these values as needed
#===============================================================================

# Video device path
DEVICE="${USTREAMER_DEVICE:-/dev/video2}"

# Resolution (WxH)
RESOLUTION="${USTREAMER_RESOLUTION:-1024x768}"

# Desired framerate
FPS="${USTREAMER_FPS:-15}"

# HTTP server settings
HOST="${USTREAMER_HOST:-0.0.0.0}"
PORT="${USTREAMER_PORT:-8093}"

# Image format: YUYV, YVYU, UYVY, YUV420, RGB24, BGR24, MJPEG, JPEG
FORMAT="${USTREAMER_FORMAT:-}"

# JPEG quality (1-100, only for software encoding)
QUALITY="${USTREAMER_QUALITY:-80}"

# Encoder: CPU, HW, M2M-VIDEO, M2M-IMAGE
ENCODER="${USTREAMER_ENCODER:-}"

# Number of buffers (default: auto based on CPU cores)
BUFFERS="${USTREAMER_BUFFERS:-}"

# Number of worker threads (default: auto based on CPU cores)
WORKERS="${USTREAMER_WORKERS:-}"

# Slowdown to 1 FPS when no clients connected (saves CPU)
SLOWDOWN="${USTREAMER_SLOWDOWN:-true}"

# Drop identical frames (reduces bandwidth, increases CPU)
DROP_SAME_FRAMES="${USTREAMER_DROP_SAME_FRAMES:-}"

# Log level: 0=info, 1=perf, 2=verbose, 3=debug
LOG_LEVEL="${USTREAMER_LOG_LEVEL:-0}"

# Path to ustreamer binary
USTREAMER_BIN="${USTREAMER_BIN:-./ustreamer}"

#===============================================================================
# Image control options (leave empty for camera defaults)
#===============================================================================

BRIGHTNESS="${USTREAMER_BRIGHTNESS:-}"
CONTRAST="${USTREAMER_CONTRAST:-}"
SATURATION="${USTREAMER_SATURATION:-}"
HUE="${USTREAMER_HUE:-}"
GAMMA="${USTREAMER_GAMMA:-}"
SHARPNESS="${USTREAMER_SHARPNESS:-}"
WHITE_BALANCE="${USTREAMER_WHITE_BALANCE:-}"
GAIN="${USTREAMER_GAIN:-}"
BACKLIGHT_COMP="${USTREAMER_BACKLIGHT_COMP:-}"
FLIP_VERTICAL="${USTREAMER_FLIP_VERTICAL:-}"
FLIP_HORIZONTAL="${USTREAMER_FLIP_HORIZONTAL:-}"
ROTATE="${USTREAMER_ROTATE:-}"

#===============================================================================
# Colors for output
#===============================================================================
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

#===============================================================================
# Functions
#===============================================================================

print_header() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

show_help() {
    print_header "uStreamer Wrapper Script"
    echo ""
    echo -e "${CYAN}Usage:${NC}"
    echo "  $0                    Start streaming with configured settings"
    echo "  $0 --help             Show this help"
    echo "  $0 --list-devices     List available video devices"
    echo "  $0 --show-config      Show current configuration"
    echo ""
    echo -e "${CYAN}Environment Variables:${NC}"
    echo "  USTREAMER_DEVICE       Video device path (default: /dev/video2)"
    echo "  USTREAMER_RESOLUTION   Resolution WxH (default: 1280x720)"
    echo "  USTREAMER_FPS          Framerate (default: 15)"
    echo "  USTREAMER_HOST         Listen address (default: 0.0.0.0)"
    echo "  USTREAMER_PORT         HTTP port (default: 8092)"
    echo "  USTREAMER_FORMAT       Image format: YUYV, MJPEG, etc."
    echo "  USTREAMER_QUALITY      JPEG quality 1-100 (default: 80)"
    echo "  USTREAMER_ENCODER      Encoder: CPU, HW, M2M-VIDEO, M2M-IMAGE"
    echo "  USTREAMER_SLOWDOWN     Slowdown when no clients (default: true)"
    echo "  USTREAMER_LOG_LEVEL    Log level 0-3 (default: 0)"
    echo ""
    echo -e "${CYAN}Image Controls:${NC}"
    echo "  USTREAMER_BRIGHTNESS, USTREAMER_CONTRAST, USTREAMER_SATURATION"
    echo "  USTREAMER_HUE, USTREAMER_GAMMA, USTREAMER_SHARPNESS"
    echo "  USTREAMER_WHITE_BALANCE, USTREAMER_GAIN, USTREAMER_BACKLIGHT_COMP"
    echo "  USTREAMER_FLIP_VERTICAL, USTREAMER_FLIP_HORIZONTAL, USTREAMER_ROTATE"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  # Start with defaults"
    echo "  $0"
    echo ""
    echo "  # Custom device and resolution"
    echo "  USTREAMER_DEVICE=/dev/video0 USTREAMER_RESOLUTION=1920x1080 $0"
    echo ""
    echo "  # High quality, low framerate"
    echo "  USTREAMER_FPS=5 USTREAMER_QUALITY=95 $0"
    echo ""
    echo -e "${CYAN}Stream URLs:${NC}"
    echo "  MJPEG Stream:  http://<ip>:<port>/stream"
    echo "  Snapshot:      http://<ip>:<port>/snapshot"
    echo "  Status:        http://<ip>:<port>/state"
    echo ""
}

list_devices() {
    print_header "Available Video Devices"
    echo ""

    for dev in /dev/video*; do
        if [ -e "$dev" ]; then
            # Get V4L2 device name
            name=$(v4l2-ctl -d "$dev" --info 2>/dev/null | grep "Card type" | cut -d: -f2 | xargs || echo "Unknown")

            # Check if it's a capture device
            caps=$(v4l2-ctl -d "$dev" --info 2>/dev/null | grep "Device Caps" -A5 | grep "Video Capture" || true)

            if [ -n "$caps" ]; then
                echo -e "${GREEN}$dev${NC} - $name"

                # Show available formats
                formats=$(v4l2-ctl -d "$dev" --list-formats-ext 2>/dev/null | grep -E "^\s+Size:|Pixel Format" | head -10)
                if [ -n "$formats" ]; then
                    echo "$formats" | sed 's/^/    /'
                fi
                echo ""
            fi
        fi
    done
}

show_config() {
    print_header "Current Configuration"
    echo ""
    echo -e "${CYAN}Video Settings:${NC}"
    echo "  Device:      $DEVICE"
    echo "  Resolution:  $RESOLUTION"
    echo "  FPS:         $FPS"
    echo "  Format:      ${FORMAT:-auto}"
    echo "  Encoder:     ${ENCODER:-CPU}"
    echo "  Quality:     $QUALITY"
    echo ""
    echo -e "${CYAN}Server Settings:${NC}"
    echo "  Host:        $HOST"
    echo "  Port:        $PORT"
    echo "  Slowdown:    $SLOWDOWN"
    echo ""
    echo -e "${CYAN}Image Controls:${NC}"
    [ -n "$BRIGHTNESS" ] && echo "  Brightness:  $BRIGHTNESS"
    [ -n "$CONTRAST" ] && echo "  Contrast:    $CONTRAST"
    [ -n "$SATURATION" ] && echo "  Saturation:  $SATURATION"
    [ -n "$HUE" ] && echo "  Hue:         $HUE"
    [ -n "$GAMMA" ] && echo "  Gamma:       $GAMMA"
    [ -n "$SHARPNESS" ] && echo "  Sharpness:   $SHARPNESS"
    [ -n "$WHITE_BALANCE" ] && echo "  White Bal:   $WHITE_BALANCE"
    [ -n "$GAIN" ] && echo "  Gain:        $GAIN"
    [ -z "$BRIGHTNESS$CONTRAST$SATURATION$HUE$GAMMA$SHARPNESS$WHITE_BALANCE$GAIN" ] && echo "  (using camera defaults)"
    echo ""
}

build_command() {
    local cmd="$USTREAMER_BIN"

    # Required options
    cmd+=" --device=$DEVICE"
    cmd+=" --resolution=$RESOLUTION"
    cmd+=" --desired-fps=$FPS"
    cmd+=" --host=$HOST"
    cmd+=" --port=$PORT"
    cmd+=" --quality=$QUALITY"

    # Optional capture settings
    [ -n "$FORMAT" ] && cmd+=" --format=$FORMAT"
    [ -n "$ENCODER" ] && cmd+=" --encoder=$ENCODER"
    [ -n "$BUFFERS" ] && cmd+=" --buffers=$BUFFERS"
    [ -n "$WORKERS" ] && cmd+=" --workers=$WORKERS"

    # Slowdown when no clients
    [ "$SLOWDOWN" = "true" ] && cmd+=" --slowdown"

    # Drop same frames
    [ -n "$DROP_SAME_FRAMES" ] && cmd+=" --drop-same-frames=$DROP_SAME_FRAMES"

    # Log level
    [ -n "$LOG_LEVEL" ] && [ "$LOG_LEVEL" != "0" ] && cmd+=" --log-level=$LOG_LEVEL"

    # Image controls
    [ -n "$BRIGHTNESS" ] && cmd+=" --brightness=$BRIGHTNESS"
    [ -n "$CONTRAST" ] && cmd+=" --contrast=$CONTRAST"
    [ -n "$SATURATION" ] && cmd+=" --saturation=$SATURATION"
    [ -n "$HUE" ] && cmd+=" --hue=$HUE"
    [ -n "$GAMMA" ] && cmd+=" --gamma=$GAMMA"
    [ -n "$SHARPNESS" ] && cmd+=" --sharpness=$SHARPNESS"
    [ -n "$WHITE_BALANCE" ] && cmd+=" --white-balance=$WHITE_BALANCE"
    [ -n "$GAIN" ] && cmd+=" --gain=$GAIN"
    [ -n "$BACKLIGHT_COMP" ] && cmd+=" --backlight-compensation=$BACKLIGHT_COMP"
    [ -n "$FLIP_VERTICAL" ] && cmd+=" --flip-vertical=$FLIP_VERTICAL"
    [ -n "$FLIP_HORIZONTAL" ] && cmd+=" --flip-horizontal=$FLIP_HORIZONTAL"
    [ -n "$ROTATE" ] && cmd+=" --rotate=$ROTATE"

    echo "$cmd"
}

start_stream() {
    # Check if ustreamer is installed
    if ! command -v "$USTREAMER_BIN" &> /dev/null; then
        echo -e "${RED}Error: ustreamer not found!${NC}"
        echo ""
        echo "Install ustreamer:"
        echo "  sudo apt install libevent-dev libjpeg-dev libbsd-dev"
        echo "  git clone --depth=1 https://github.com/pikvm/ustreamer"
        echo "  cd ustreamer && make"
        echo "  sudo make install"
        exit 1
    fi

    # Check if device exists
    if [ ! -e "$DEVICE" ]; then
        echo -e "${RED}Error: Device $DEVICE not found!${NC}"
        echo ""
        echo "Available devices:"
        ls -la /dev/video* 2>/dev/null || echo "  No video devices found"
        exit 1
    fi

    print_header "Starting uStreamer"
    show_config

    local cmd=$(build_command)

    echo -e "${CYAN}Command:${NC}"
    echo "  $cmd"
    echo ""
    echo -e "${GREEN}Stream URLs:${NC}"
    echo "  MJPEG Stream:  http://$(hostname -I | awk '{print $1}'):$PORT/stream"
    echo "  Snapshot:      http://$(hostname -I | awk '{print $1}'):$PORT/snapshot"
    echo "  Status:        http://$(hostname -I | awk '{print $1}'):$PORT/state"
    echo ""
    echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
    echo ""

    # Execute
    exec $cmd
}

#===============================================================================
# Main
#===============================================================================

case "${1:-}" in
    --help|-h)
        show_help
        ;;
    --list-devices|-l)
        list_devices
        ;;
    --show-config|-c)
        show_config
        ;;
    *)
        start_stream
        ;;
esac
