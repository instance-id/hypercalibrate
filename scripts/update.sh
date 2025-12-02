#!/bin/bash
#===============================================================================
# HyperCalibrate - Quick Update Script
#
# A lightweight script that only:
# 1. Copies the pre-built binary to the Pi
# 2. Restarts the service
#
# Use this for rapid iteration. Use deploy.sh for full installation.
#
# Usage: ./update.sh [raspberry-pi-ip]
#===============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

#-------------------------------------------------------------------------------
# Load configuration
#-------------------------------------------------------------------------------
if [ -f "$PROJECT_ROOT/deploy.conf" ]; then
    source "$PROJECT_ROOT/deploy.conf"
fi

PI_USER="${PI_USER:-hyperion}"
PI_PASS="${PI_PASS:-ambientlight}"
PI_PORT="${PI_PORT:-22}"

# Allow IP override from command line
if [ -n "$1" ]; then
    PI_IP="$1"
fi

#-------------------------------------------------------------------------------
# Validation
#-------------------------------------------------------------------------------
if [ -z "$PI_IP" ]; then
    echo "❌ Error: Raspberry Pi IP address required"
    echo "   Usage: $0 <raspberry-pi-ip>"
    echo "   Or set PI_IP in deploy.conf"
    exit 1
fi

BINARY="$PROJECT_ROOT/dist/hypercalibrate"
if [ ! -f "$BINARY" ]; then
    echo "❌ Binary not found at $BINARY"
    echo "   Run ./docker-build.sh first"
    exit 1
fi

#-------------------------------------------------------------------------------
# Check sshpass
#-------------------------------------------------------------------------------
if ! command -v sshpass &> /dev/null; then
    echo "❌ sshpass not found. Install it or use deploy.sh"
    exit 1
fi

#-------------------------------------------------------------------------------
# SSH helpers
#-------------------------------------------------------------------------------
ssh_cmd() {
    sshpass -p "$PI_PASS" ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -q -p "$PI_PORT" "$PI_USER@$PI_IP" "$@"
}

scp_cmd() {
    sshpass -p "$PI_PASS" scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
        -q -P "$PI_PORT" "$@"
}

#-------------------------------------------------------------------------------
# Update
#-------------------------------------------------------------------------------
echo "🔄 Quick update to $PI_USER@$PI_IP"
echo ""

# Stop service
echo "⏹️  Stopping service..."
ssh_cmd "sudo systemctl stop hypercalibrate" 2>/dev/null || true

# Copy binary
echo "📦 Copying binary ($(du -h "$BINARY" | cut -f1))..."
scp_cmd "$BINARY" "$PI_USER@$PI_IP:/tmp/hypercalibrate-new"

# Install and restart
echo "📋 Installing..."
ssh_cmd "sudo mv /tmp/hypercalibrate-new /usr/local/bin/hypercalibrate && sudo chmod +x /usr/local/bin/hypercalibrate"

echo "🚀 Starting service..."
ssh_cmd "sudo systemctl start hypercalibrate"

# Quick status check
sleep 1
if ssh_cmd "systemctl is-active --quiet hypercalibrate"; then
    PI_ACTUAL_IP=$(ssh_cmd "hostname -I | awk '{print \$1}'" 2>/dev/null)
    WEB_PORT="${WEB_PORT:-8091}"
    echo ""
    echo "✅ Update complete!"
    echo "   📱 http://$PI_ACTUAL_IP:$WEB_PORT"
else
    echo ""
    echo "⚠️  Service may not have started. Check logs:"
    echo "   ssh $PI_USER@$PI_IP 'sudo journalctl -u hypercalibrate -n 20'"
fi
