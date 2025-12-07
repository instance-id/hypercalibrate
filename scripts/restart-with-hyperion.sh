#!/bin/bash
#===============================================================================
# HyperCalibrate - Coordinated Restart Script
#
# This script properly restarts HyperCalibrate along with Hyperion to ensure
# the v4l2loopback virtual camera can be reconfigured without "device busy"
# errors.
#
# Order of operations:
#   1. Stop Hyperion (releases /dev/video10)
#   2. Stop HyperCalibrate
#   3. Start HyperCalibrate (ExecStartPre reconfigures v4l2loopback)
#   4. Start Hyperion (opens /dev/video10 for reading)
#
# Note: v4l2loopback reconfiguration happens automatically via the systemd
# service's ExecStartPre=/usr/local/bin/hypercalibrate-setup, which reads
# the config file and sets up the virtual camera with the correct resolution.
#
# Usage: sudo hypercalibrate-restart
#        (Installed to /usr/local/bin/hypercalibrate-restart)
#===============================================================================

set -e

# Detect Hyperion service name (could be hyperion or hyperion@hyperion)
detect_hyperion_service() {
    if systemctl list-units --type=service --all | grep -q "hyperion@"; then
        # Template service - find the active instance
        local instance
        instance=$(systemctl list-units --type=service --all | grep "hyperion@" | head -1 | awk '{print $1}')
        if [ -n "$instance" ]; then
            echo "$instance"
            return
        fi
    fi

    if systemctl list-units --type=service --all | grep -q "hyperiond"; then
        echo "hyperiond"
        return
    fi

    if systemctl list-units --type=service --all | grep -q "hyperion.service"; then
        echo "hyperion"
        return
    fi

    echo ""
}

HYPERION_SERVICE=$(detect_hyperion_service)

echo "🔄 HyperCalibrate Coordinated Restart"
echo "   Hyperion service: ${HYPERION_SERVICE:-not found}"
echo ""

# Step 1: Stop Hyperion if running (releases the v4l2loopback device)
if [ -n "$HYPERION_SERVICE" ]; then
    echo "⏹️  Stopping Hyperion ($HYPERION_SERVICE)..."
    systemctl stop "$HYPERION_SERVICE" 2>/dev/null || true
    sleep 1
else
    echo "ℹ️  Hyperion service not found, skipping..."
fi

# Step 2: Stop HyperCalibrate
echo "⏹️  Stopping HyperCalibrate..."
systemctl stop hypercalibrate 2>/dev/null || true
sleep 1

# Step 3: Start HyperCalibrate
# Note: ExecStartPre=/usr/local/bin/hypercalibrate-setup will automatically
# reconfigure v4l2loopback with the current settings from config.toml
echo "▶️  Starting HyperCalibrate..."
echo "   (ExecStartPre will reconfigure v4l2loopback from config)"
systemctl start hypercalibrate

# Wait for HyperCalibrate to initialize and open the device
echo "   Waiting for HyperCalibrate to initialize..."
sleep 3

# Verify HyperCalibrate is running
if ! systemctl is-active --quiet hypercalibrate; then
    echo "❌ HyperCalibrate failed to start!"
    journalctl -u hypercalibrate -n 20 --no-pager
    exit 1
fi

# Step 4: Start Hyperion (it will open the device for reading)
if [ -n "$HYPERION_SERVICE" ]; then
    echo "▶️  Starting Hyperion..."
    systemctl start "$HYPERION_SERVICE"

    sleep 2

    if systemctl is-active --quiet "$HYPERION_SERVICE"; then
        echo "✅ Hyperion started successfully"
    else
        echo "⚠️  Hyperion may have failed to start"
        journalctl -u "$HYPERION_SERVICE" -n 10 --no-pager
    fi
fi

echo ""
echo "✅ Coordinated restart complete!"
echo ""
echo "   HyperCalibrate: $(systemctl is-active hypercalibrate)"
if [ -n "$HYPERION_SERVICE" ]; then
    echo "   Hyperion: $(systemctl is-active "$HYPERION_SERVICE")"
fi
