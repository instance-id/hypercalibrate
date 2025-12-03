#!/bin/bash
#===============================================================================
# HyperCalibrate - Log Collection Script
# Collects logs from Raspberry Pi and saves them locally for debugging
#===============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Load configuration
if [ -f "$PROJECT_DIR/deploy.conf" ]; then
    source "$PROJECT_DIR/deploy.conf"
else
    echo "❌ deploy.conf not found. Please copy deploy.conf.example to deploy.conf"
    exit 1
fi

# Configuration
PI_IP="${PI_IP:-192.168.50.97}"
PI_USER="${PI_USER:-hyperion}"
PI_PASS="${PI_PASS:-ambientlight}"
PI_PORT="${PI_PORT:-22}"

# Output configuration
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
OUTPUT_DIR="${PROJECT_DIR}/logs"
OUTPUT_FILE="${OUTPUT_DIR}/pi_logs_${TIMESTAMP}.txt"

# Number of log lines to collect (default: 500)
LOG_LINES="${1:-500}"

echo "📋 Collecting logs from Raspberry Pi"
echo "   Host: ${PI_USER}@${PI_IP}"
echo "   Lines: ${LOG_LINES} per service"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check for sshpass
if ! command -v sshpass &> /dev/null; then
    echo "❌ sshpass not found. Install with: sudo apt install sshpass"
    exit 1
fi

SSH_CMD="sshpass -p '$PI_PASS' ssh -o StrictHostKeyChecking=no -p $PI_PORT ${PI_USER}@${PI_IP}"

# Function to run SSH command
run_ssh() {
    eval "$SSH_CMD \"$1\"" 2>/dev/null
}

echo "🔍 Gathering system information..."

{
    echo "==============================================================================="
    echo "HyperCalibrate Log Collection"
    echo "Collected: $(date)"
    echo "Host: ${PI_USER}@${PI_IP}"
    echo "==============================================================================="
    echo ""

    echo "==============================================================================="
    echo "SYSTEM INFORMATION"
    echo "==============================================================================="
    echo ""
    echo "--- Hostname & OS ---"
    run_ssh "hostname; cat /etc/os-release | head -5"
    echo ""
    echo "--- Kernel ---"
    run_ssh "uname -a"
    echo ""
    echo "--- Uptime & Load ---"
    run_ssh "uptime"
    echo ""
    echo "--- Memory ---"
    run_ssh "free -h"
    echo ""
    echo "--- Disk Usage ---"
    run_ssh "df -h / /boot 2>/dev/null || df -h /"
    echo ""
    echo "--- CPU Temperature ---"
    run_ssh "vcgencmd measure_temp 2>/dev/null || echo 'vcgencmd not available'"
    echo ""
    echo "--- Throttle Status ---"
    run_ssh "vcgencmd get_throttled 2>/dev/null || echo 'vcgencmd not available'"
    echo ""

    echo "==============================================================================="
    echo "VIDEO DEVICES"
    echo "==============================================================================="
    echo ""
    echo "--- V4L2 Devices ---"
    run_ssh "v4l2-ctl --list-devices 2>/dev/null || echo 'v4l2-ctl not available'"
    echo ""
    echo "--- /dev/video* ---"
    run_ssh "ls -la /dev/video* 2>/dev/null || echo 'No video devices'"
    echo ""
    echo "--- v4l2loopback Module ---"
    run_ssh "lsmod | grep v4l2loopback || echo 'v4l2loopback not loaded'"
    echo ""

    echo "==============================================================================="
    echo "SERVICE STATUS"
    echo "==============================================================================="
    echo ""
    echo "--- HyperCalibrate Service ---"
    run_ssh "sudo systemctl status hypercalibrate --no-pager 2>/dev/null || echo 'Service not found'"
    echo ""
    echo "--- Hyperion Service ---"
    run_ssh "sudo systemctl status hyperion --no-pager 2>/dev/null || systemctl status hyperiond --no-pager 2>/dev/null || echo 'Service not found'"
    echo ""

    echo "==============================================================================="
    echo "HYPERCALIBRATE LOGS (last ${LOG_LINES} lines)"
    echo "==============================================================================="
    echo ""
    run_ssh "sudo journalctl -u hypercalibrate -n ${LOG_LINES} --no-pager 2>/dev/null || echo 'No logs available'"
    echo ""

    echo "==============================================================================="
    echo "HYPERION LOGS (last ${LOG_LINES} lines)"
    echo "==============================================================================="
    echo ""
    run_ssh "sudo journalctl -u hyperion -n ${LOG_LINES} --no-pager 2>/dev/null || sudo journalctl -u hyperiond -n ${LOG_LINES} --no-pager 2>/dev/null || echo 'No logs available'"
    echo ""

    echo "==============================================================================="
    echo "KERNEL MESSAGES - VIDEO/USB (last 100 lines)"
    echo "==============================================================================="
    echo ""
    run_ssh "dmesg | grep -iE 'video|usb|v4l|uvc|loopback' | tail -100 || echo 'No relevant kernel messages'"
    echo ""

    echo "==============================================================================="
    echo "SYSTEM LOGS (last 200 lines)"
    echo "==============================================================================="
    echo ""
    run_ssh "sudo journalctl -p err -n 200 --no-pager 2>/dev/null || echo 'No system logs available'"
    echo ""

    echo "==============================================================================="
    echo "HYPERCALIBRATE CONFIG"
    echo "==============================================================================="
    echo ""
    run_ssh "cat /etc/hypercalibrate/config.toml 2>/dev/null || echo 'Config not found'"
    echo ""

    echo "==============================================================================="
    echo "END OF LOG COLLECTION"
    echo "==============================================================================="

} > "$OUTPUT_FILE"

echo ""
echo "✅ Logs saved to: $OUTPUT_FILE"
echo "   Size: $(du -h "$OUTPUT_FILE" | cut -f1)"
echo ""
echo "📝 Quick view of recent errors:"
grep -iE "error|fail|panic|crash" "$OUTPUT_FILE" | tail -10 || echo "   No obvious errors found"
