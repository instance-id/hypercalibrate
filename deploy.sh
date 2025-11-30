#!/bin/bash
#===============================================================================
# HyperCalibrate - Deploy to Raspberry Pi (HyperBian)
#
# This script:
# 1. Cross-compiles the binary for Raspberry Pi (if not already built)
# 2. Copies the binary and install script to the Pi
# 3. Runs the installation on the Pi
#
# Usage: ./deploy.sh <raspberry-pi-ip> [options]
#===============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

#-------------------------------------------------------------------------------
# Load configuration file if exists
#-------------------------------------------------------------------------------
if [ -f "$SCRIPT_DIR/deploy.conf" ]; then
    echo "📋 Loading configuration from deploy.conf..."
    source "$SCRIPT_DIR/deploy.conf"
fi

#-------------------------------------------------------------------------------
# Configuration (HyperBian defaults, can be overridden by deploy.conf or env)
#-------------------------------------------------------------------------------
PI_USER="${PI_USER:-${HYPERPI_USER:-hyperion}}"
PI_PASS="${PI_PASS:-${HYPERPI_PASS:-ambientlight}}"
PI_PORT="${PI_PORT:-${HYPERPI_PORT:-22}}"

# Video device configuration
INPUT_DEVICE="${INPUT_DEVICE:-/dev/video0}"
OUTPUT_DEVICE="${OUTPUT_DEVICE:-/dev/video10}"
CAPTURE_WIDTH="${CAPTURE_WIDTH:-640}"
CAPTURE_HEIGHT="${CAPTURE_HEIGHT:-480}"
CAPTURE_FPS="${CAPTURE_FPS:-30}"
WEB_PORT="${WEB_PORT:-8091}"
WEB_HOST="${WEB_HOST:-0.0.0.0}"

# Build configuration
TARGET_ARCH="${TARGET_ARCH:-aarch64}"

#-------------------------------------------------------------------------------
# Parse arguments
#-------------------------------------------------------------------------------
show_help() {
    cat << EOF
HyperCalibrate - Deploy to Raspberry Pi (HyperBian)

Usage: $0 [raspberry-pi-ip] [options]

Arguments:
  <raspberry-pi-ip>    IP address of your Raspberry Pi (or set PI_IP in deploy.conf)

Options:
  -u, --user USER      SSH username (default: hyperion)
  -p, --password PASS  SSH password (default: ambientlight)
  -P, --port PORT      SSH port (default: 22)
  -c, --config FILE    Load configuration from file (default: deploy.conf)
  --skip-build         Skip building, use existing binary in dist/
  --build-only         Only build, don't deploy
  --uninstall          Remove HyperCalibrate from the Pi
  -h, --help           Show this help message

Configuration File (deploy.conf):
  Copy deploy.conf.example to deploy.conf and customize.
  Command line options override config file values.

Examples:
  # Deploy using deploy.conf settings
  $0

  # Deploy with IP override
  $0 192.168.1.100

  # Deploy with custom credentials
  $0 192.168.1.100 -u pi -p raspberry

  # Just build, don't deploy
  $0 --build-only

  # Deploy existing build
  $0 192.168.1.100 --skip-build

  # Uninstall from Pi
  $0 192.168.1.100 --uninstall
EOF
}

SKIP_BUILD=false
BUILD_ONLY=false
UNINSTALL=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -u|--user)
            PI_USER="$2"
            shift 2
            ;;
        -p|--password)
            PI_PASS="$2"
            shift 2
            ;;
        -P|--port)
            PI_PORT="$2"
            shift 2
            ;;
        -c|--config)
            if [ -f "$2" ]; then
                source "$2"
            else
                echo "❌ Config file not found: $2"
                exit 1
            fi
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --build-only)
            BUILD_ONLY=true
            shift
            ;;
        --uninstall)
            UNINSTALL=true
            shift
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        -*)
            echo "Unknown option: $1"
            show_help
            exit 1
            ;;
        *)
            PI_IP="$1"
            shift
            ;;
    esac
done

#-------------------------------------------------------------------------------
# Validation
#-------------------------------------------------------------------------------
if [ -z "$PI_IP" ] && [ "$BUILD_ONLY" = false ]; then
    echo "❌ Error: Raspberry Pi IP address is required"
    echo "   Either pass it as an argument or set PI_IP in deploy.conf"
    echo ""
    show_help
    exit 1
fi

#-------------------------------------------------------------------------------
# Check for sshpass (needed for password auth)
#-------------------------------------------------------------------------------
check_sshpass() {
    if ! command -v sshpass &> /dev/null; then
        echo "⚠️  sshpass not found. Installing..."
        if command -v apt-get &> /dev/null; then
            sudo apt-get update && sudo apt-get install -y sshpass
        elif command -v brew &> /dev/null; then
            brew install hudochenkov/sshpass/sshpass
        elif command -v pacman &> /dev/null; then
            sudo pacman -S sshpass
        else
            echo "❌ Please install sshpass manually"
            exit 1
        fi
    fi
}

#-------------------------------------------------------------------------------
# SSH helper function
#-------------------------------------------------------------------------------
ssh_cmd() {
    sshpass -p "$PI_PASS" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -p "$PI_PORT" "$PI_USER@$PI_IP" "$@"
}

scp_cmd() {
    sshpass -p "$PI_PASS" scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -P "$PI_PORT" "$@"
}

#-------------------------------------------------------------------------------
# Build
#-------------------------------------------------------------------------------
build() {
    echo "🔨 Building HyperCalibrate for Raspberry Pi..."

    # Use Docker build
    if [ -f "$SCRIPT_DIR/docker-build.sh" ]; then
        bash "$SCRIPT_DIR/docker-build.sh" "$TARGET_ARCH"
    else
        echo "❌ docker-build.sh not found"
        exit 1
    fi
}

#-------------------------------------------------------------------------------
# Deploy
#-------------------------------------------------------------------------------
deploy() {
    local BINARY="$SCRIPT_DIR/dist/hypercalibrate"

    if [ ! -f "$BINARY" ]; then
        echo "❌ Binary not found at $BINARY"
        echo "   Run with --skip-build removed, or build first with:"
        echo "   ./docker-build.sh"
        exit 1
    fi

    echo ""
    echo "🚀 Deploying to $PI_USER@$PI_IP..."
    echo "   Configuration:"
    echo "   - Input:  $INPUT_DEVICE"
    echo "   - Output: $OUTPUT_DEVICE"
    echo "   - Web UI: http://$PI_IP:$WEB_PORT"

    check_sshpass

    # Test connection
    echo ""
    echo "   Testing SSH connection..."
    if ! ssh_cmd "echo 'Connected!'" 2>/dev/null; then
        echo "❌ Failed to connect to $PI_IP"
        echo "   Check IP address and credentials"
        exit 1
    fi

    # Create temp directory on Pi
    echo "   Creating installation directory..."
    ssh_cmd "mkdir -p /tmp/hypercalibrate"

    # Copy files
    echo "   Copying binary ($(du -h "$BINARY" | cut -f1))..."
    scp_cmd "$BINARY" "$PI_USER@$PI_IP:/tmp/hypercalibrate/"

    echo "   Copying configuration..."
    scp_cmd "$SCRIPT_DIR/config.example.toml" "$PI_USER@$PI_IP:/tmp/hypercalibrate/"

    # Create and copy the remote install script
    create_remote_install_script
    scp_cmd "/tmp/hypercalibrate-install.sh" "$PI_USER@$PI_IP:/tmp/hypercalibrate/install.sh"

    # Run installation
    echo ""
    echo "📦 Running installation on Raspberry Pi..."
    ssh_cmd "chmod +x /tmp/hypercalibrate/install.sh && sudo /tmp/hypercalibrate/install.sh"

    # Get Pi's IP for display
    PI_ACTUAL_IP=$(ssh_cmd "hostname -I | awk '{print \$1}'" 2>/dev/null)

    echo ""
    echo "✅ Deployment complete!"
    echo ""
    echo "🎯 HyperCalibrate is now running on your Raspberry Pi"
    echo ""
    echo "   📱 Calibration UI: http://$PI_ACTUAL_IP:$WEB_PORT"
    echo "   📹 Virtual Camera: $OUTPUT_DEVICE"
    echo ""
    echo "   Configure in Hyperion:"
    echo "   1. Go to Configuration → LED Hardware → LED Controller"
    echo "   2. Add/edit your LED instance"
    echo "   3. Under 'Grabber', select 'Platform Capture'"
    echo "   4. Choose 'HyperCalibrate' ($OUTPUT_DEVICE) as input"
    echo ""
    echo "🔧 Manage the service:"
    echo "   ssh $PI_USER@$PI_IP"
    echo "   sudo systemctl status hypercalibrate"
    echo "   sudo systemctl restart hypercalibrate"
    echo "   sudo journalctl -u hypercalibrate -f"
}

#-------------------------------------------------------------------------------
# Uninstall
#-------------------------------------------------------------------------------
uninstall() {
    echo "🗑️  Uninstalling HyperCalibrate from $PI_IP..."

    check_sshpass

    ssh_cmd "sudo systemctl stop hypercalibrate 2>/dev/null || true"
    ssh_cmd "sudo systemctl disable hypercalibrate 2>/dev/null || true"
    ssh_cmd "sudo rm -f /etc/systemd/system/hypercalibrate.service"
    ssh_cmd "sudo rm -f /usr/local/bin/hypercalibrate"
    ssh_cmd "sudo rm -rf /etc/hypercalibrate"
    ssh_cmd "sudo systemctl daemon-reload"

    echo "✅ HyperCalibrate has been removed"
}

#-------------------------------------------------------------------------------
# Create remote installation script with embedded configuration
#-------------------------------------------------------------------------------
create_remote_install_script() {
    # Extract video device number from OUTPUT_DEVICE (e.g., /dev/video10 -> 10)
    local VIDEO_NR="${OUTPUT_DEVICE##*/video}"

    cat > /tmp/hypercalibrate-install.sh << INSTALL_SCRIPT
#!/bin/bash
#===============================================================================
# HyperCalibrate - Remote Installation Script
# This runs on the Raspberry Pi
# Generated by deploy.sh with embedded configuration
#===============================================================================

set -e

# Configuration (embedded from deploy script)
INPUT_DEVICE="$INPUT_DEVICE"
OUTPUT_DEVICE="$OUTPUT_DEVICE"
VIDEO_NR="$VIDEO_NR"
CAPTURE_WIDTH="$CAPTURE_WIDTH"
CAPTURE_HEIGHT="$CAPTURE_HEIGHT"
CAPTURE_FPS="$CAPTURE_FPS"
WEB_PORT="$WEB_PORT"
WEB_HOST="$WEB_HOST"

echo "🍓 Installing HyperCalibrate on Raspberry Pi..."
echo "   Input:  \$INPUT_DEVICE"
echo "   Output: \$OUTPUT_DEVICE"
echo "   Web UI: http://\$(hostname -I | awk '{print \$1}'):\$WEB_PORT"
echo ""

# Install v4l2loopback if not present
echo "📦 Checking v4l2loopback..."
if ! dpkg -l | grep -q v4l2loopback-dkms; then
    echo "   Installing v4l2loopback..."
    apt-get update
    apt-get install -y v4l2loopback-dkms v4l-utils
fi

# Load v4l2loopback module
# NOTE: exclusive_caps=0 is required to prevent Hyperion from crashing
# when it enumerates video devices at startup
echo "🔧 Setting up virtual camera device..."
modprobe -r v4l2loopback 2>/dev/null || true
modprobe v4l2loopback devices=1 video_nr=\$VIDEO_NR card_label="HyperCalibrate" exclusive_caps=0

# Pre-configure the virtual camera format and enable keep_format
# This allows our app to set the format and write frames successfully
echo "📹 Configuring virtual camera format..."
v4l2-ctl -d /dev/video\$VIDEO_NR --set-fmt-video-out="width=\$CAPTURE_WIDTH,height=\$CAPTURE_HEIGHT,pixelformat=YUYV" 2>/dev/null || true
v4l2-ctl -d /dev/video\$VIDEO_NR --set-ctrl keep_format=1 2>/dev/null || true

# Make persistent
echo "v4l2loopback" > /etc/modules-load.d/v4l2loopback.conf
echo "options v4l2loopback devices=1 video_nr=\$VIDEO_NR card_label=HyperCalibrate exclusive_caps=0" > /etc/modprobe.d/v4l2loopback.conf

# Install binary
echo "📋 Installing binary..."
install -m 755 /tmp/hypercalibrate/hypercalibrate /usr/local/bin/

# Create config directory and config file
echo "📁 Setting up configuration..."
mkdir -p /etc/hypercalibrate

# Generate config file with proper values
cat > /etc/hypercalibrate/config.toml << TOML_CONFIG
# HyperCalibrate Configuration
# Generated by deploy script

[video]
input_device = "\$INPUT_DEVICE"
output_device = "\$OUTPUT_DEVICE"
width = \$CAPTURE_WIDTH
height = \$CAPTURE_HEIGHT
fps = \$CAPTURE_FPS

[server]
host = "\$WEB_HOST"
port = \$WEB_PORT

[calibration]
enabled = true

[[calibration.corners]]
x = 0.1
y = 0.1

[[calibration.corners]]
x = 0.9
y = 0.1

[[calibration.corners]]
x = 0.9
y = 0.9

[[calibration.corners]]
x = 0.1
y = 0.9

# Edge points are dynamic - start with none, add via UI with Shift+Click
# Remove points with Ctrl+Click
edge_points = []
TOML_CONFIG

# Create systemd service
# NOTE: Service starts AFTER Hyperion to prevent v4l2loopback from crashing Hyperion
echo "⚙️  Creating systemd service..."
cat > /etc/systemd/system/hypercalibrate.service << 'SERVICE_FILE'
[Unit]
Description=HyperCalibrate - TV Screen Calibration for Hyperion
After=network.target hyperion@hyperion.service
Wants=hyperion@hyperion.service

[Service]
Type=simple
User=root
ExecStartPre=/sbin/modprobe v4l2loopback devices=1 video_nr=VIDEO_NR_PLACEHOLDER card_label=HyperCalibrate exclusive_caps=0
ExecStartPre=/bin/sh -c 'v4l2-ctl -d /dev/videoVIDEO_NR_PLACEHOLDER --set-fmt-video-out="width=WIDTH_PLACEHOLDER,height=HEIGHT_PLACEHOLDER,pixelformat=YUYV" && v4l2-ctl -d /dev/videoVIDEO_NR_PLACEHOLDER --set-ctrl keep_format=1'
ExecStart=/usr/local/bin/hypercalibrate --config /etc/hypercalibrate/config.toml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
SERVICE_FILE

# Replace placeholder in service file
sed -i "s/VIDEO_NR_PLACEHOLDER/\$VIDEO_NR/g" /etc/systemd/system/hypercalibrate.service
sed -i "s/WIDTH_PLACEHOLDER/\$CAPTURE_WIDTH/g" /etc/systemd/system/hypercalibrate.service
sed -i "s/HEIGHT_PLACEHOLDER/\$CAPTURE_HEIGHT/g" /etc/systemd/system/hypercalibrate.service

# Enable and start service
echo "🚀 Starting service..."
systemctl daemon-reload
systemctl enable hypercalibrate
systemctl restart hypercalibrate

# Wait a moment for startup
sleep 2

# Check status
if systemctl is-active --quiet hypercalibrate; then
    echo ""
    echo "✅ HyperCalibrate is running!"
    echo ""
    echo "🎯 Access the calibration UI at:"
    echo "   http://\$(hostname -I | awk '{print \$1}'):\$WEB_PORT"
else
    echo ""
    echo "⚠️  Service may not have started correctly. Check with:"
    echo "   sudo journalctl -u hypercalibrate -n 50"
fi

# Cleanup
rm -rf /tmp/hypercalibrate

echo ""
echo "✅ Installation complete!"
INSTALL_SCRIPT
}

#-------------------------------------------------------------------------------
# Main
#-------------------------------------------------------------------------------
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║         HyperCalibrate - Deploy to Raspberry Pi               ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

if [ "$UNINSTALL" = true ]; then
    uninstall
    exit 0
fi

if [ "$SKIP_BUILD" = false ]; then
    build
fi

if [ "$BUILD_ONLY" = true ]; then
    echo ""
    echo "✅ Build complete. Binary at: dist/hypercalibrate"
    exit 0
fi

deploy
