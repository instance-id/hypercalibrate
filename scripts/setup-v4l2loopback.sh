#!/bin/bash
#===============================================================================
# HyperCalibrate - v4l2loopback Setup Script
#
# This script reads video settings from the config file and configures
# the v4l2loopback device accordingly. Called by systemd before starting
# HyperCalibrate.
#
# Usage: sudo ./setup-v4l2loopback.sh [config_file]
#===============================================================================

set -e

# Configuration
CONFIG_FILE="${1:-/etc/hypercalibrate/config.toml}"

# Default values (used if config parsing fails)
DEFAULT_WIDTH=640
DEFAULT_HEIGHT=480
DEFAULT_VIDEO_NR=10

# Parse config file for video settings
parse_config() {
    local key="$1"
    local default="$2"
    
    if [ -f "$CONFIG_FILE" ]; then
        # Simple TOML parsing - look for key = value in [video] or [camera] section
        local value
        value=$(grep -A 20 '^\[video\]' "$CONFIG_FILE" 2>/dev/null | grep "^$key\s*=" | head -1 | cut -d'=' -f2 | tr -d ' "' || true)
        
        # Also check [camera] section for backwards compatibility
        if [ -z "$value" ]; then
            value=$(grep -A 20 '^\[camera\]' "$CONFIG_FILE" 2>/dev/null | grep "^$key\s*=" | head -1 | cut -d'=' -f2 | tr -d ' "' || true)
        fi
        
        if [ -n "$value" ]; then
            echo "$value"
            return
        fi
    fi
    
    echo "$default"
}

# Get settings from config
WIDTH=$(parse_config "width" "$DEFAULT_WIDTH")
HEIGHT=$(parse_config "height" "$DEFAULT_HEIGHT")
OUTPUT_DEVICE=$(parse_config "output_device" "/dev/video$DEFAULT_VIDEO_NR")
VIDEO_NR="${OUTPUT_DEVICE##*/video}"

echo "🎥 Setting up v4l2loopback for HyperCalibrate"
echo "   Resolution: ${WIDTH}x${HEIGHT}"
echo "   Device: /dev/video${VIDEO_NR}"

# Ensure v4l2loopback module is loaded with correct parameters
# First, try to unload if already loaded (ignore errors)
modprobe -r v4l2loopback 2>/dev/null || true

# Load with our parameters
# exclusive_caps=0 is REQUIRED to prevent Hyperion from crashing on startup
modprobe v4l2loopback \
    devices=1 \
    video_nr="$VIDEO_NR" \
    card_label="HyperCalibrate" \
    exclusive_caps=0

# Wait a moment for device to appear
sleep 0.5

# Verify device exists
if [ ! -e "/dev/video${VIDEO_NR}" ]; then
    echo "❌ Error: /dev/video${VIDEO_NR} not found after loading module"
    exit 1
fi

# Set the video format
# This pre-configures the virtual camera so our app can write to it
echo "   Setting format: YUYV ${WIDTH}x${HEIGHT}"
v4l2-ctl -d "/dev/video${VIDEO_NR}" \
    --set-fmt-video-out="width=${WIDTH},height=${HEIGHT},pixelformat=YUYV" 2>/dev/null || true

# Enable keep_format to preserve settings
v4l2-ctl -d "/dev/video${VIDEO_NR}" --set-ctrl keep_format=1 2>/dev/null || true

echo "✅ v4l2loopback configured successfully"
