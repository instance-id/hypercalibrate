#!/bin/bash
#
# Setup udev rules for persistent video device symlinks
# This ensures your HDMI capture device is always at a predictable path
#
# Usage:
#   ./setup-udev-rules.sh                    # Interactive mode - detects devices
#   ./setup-udev-rules.sh --list             # List current video devices
#   ./setup-udev-rules.sh --remove           # Remove hypercalibrate udev rules
#

set -e

RULES_FILE="/etc/udev/rules.d/99-hypercalibrate-video.rules"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

list_video_devices() {
    print_header "Available Video Devices"
    echo ""

    for dev in /dev/video*; do
        if [ -e "$dev" ]; then
            # Get device info
            info=$(udevadm info -q property -n "$dev" 2>/dev/null || true)
            vendor=$(echo "$info" | grep "^ID_VENDOR=" | cut -d= -f2)
            model=$(echo "$info" | grep "^ID_MODEL=" | cut -d= -f2)
            vendor_id=$(echo "$info" | grep "^ID_VENDOR_ID=" | cut -d= -f2)
            product_id=$(echo "$info" | grep "^ID_MODEL_ID=" | cut -d= -f2)

            # Get V4L2 device name
            v4l2_name=$(v4l2-ctl -d "$dev" --info 2>/dev/null | grep "Card type" | cut -d: -f2 | xargs || echo "Unknown")

            echo -e "${GREEN}$dev${NC}"
            echo "  Name:      $v4l2_name"
            echo "  Vendor:    ${vendor:-Unknown} (${vendor_id:-????})"
            echo "  Model:     ${model:-Unknown} (${product_id:-????})"
            echo ""
        fi
    done
}

remove_rules() {
    print_header "Removing HyperCalibrate udev Rules"

    if [ -f "$RULES_FILE" ]; then
        sudo rm -f "$RULES_FILE"
        sudo udevadm control --reload-rules
        sudo udevadm trigger
        echo -e "${GREEN}✓ Removed $RULES_FILE${NC}"
        echo "  Reboot or replug devices for changes to take effect."
    else
        echo -e "${YELLOW}No HyperCalibrate udev rules found.${NC}"
    fi
}

create_rule_for_device() {
    local dev=$1
    local symlink_name=$2

    # Get device info
    info=$(udevadm info -q property -n "$dev" 2>/dev/null)
    vendor_id=$(echo "$info" | grep "^ID_VENDOR_ID=" | cut -d= -f2)
    product_id=$(echo "$info" | grep "^ID_MODEL_ID=" | cut -d= -f2)

    if [ -z "$vendor_id" ] || [ -z "$product_id" ]; then
        echo -e "${RED}Could not get vendor/product ID for $dev${NC}"
        return 1
    fi

    # Create the rule
    # SUBSYSTEM=="video4linux" matches video devices
    # ATTR{index}=="0" ensures we only match the main video node (not metadata nodes)
    local rule="SUBSYSTEM==\"video4linux\", ATTR{index}==\"0\", ATTRS{idVendor}==\"$vendor_id\", ATTRS{idProduct}==\"$product_id\", SYMLINK+=\"$symlink_name\", TAG+=\"hypercalibrate\""

    echo "$rule"
}

interactive_setup() {
    print_header "HyperCalibrate Video Device Setup"
    echo ""
    echo "This script will create udev rules to give your video devices"
    echo "persistent symlinks that don't change between reboots."
    echo ""

    # List devices first
    list_video_devices

    # Check if we have any video devices
    if ! ls /dev/video* 1>/dev/null 2>&1; then
        echo -e "${RED}No video devices found!${NC}"
        exit 1
    fi

    echo -e "${YELLOW}Which device is your HDMI capture card?${NC}"
    echo "Enter the device path (e.g., /dev/video0) or 'skip' to skip:"
    read -r capture_dev

    echo ""
    echo -e "${YELLOW}Which device is your USB camera? (optional)${NC}"
    echo "Enter the device path (e.g., /dev/video2) or 'skip' to skip:"
    read -r camera_dev

    # Build rules
    rules=""

    if [ "$capture_dev" != "skip" ] && [ -n "$capture_dev" ]; then
        rule=$(create_rule_for_device "$capture_dev" "hdmi_capture")
        if [ -n "$rule" ]; then
            rules+="# HDMI Capture Device\n$rule\n\n"
            echo -e "${GREEN}✓ Will create /dev/hdmi_capture → $capture_dev${NC}"
        fi
    fi

    if [ "$camera_dev" != "skip" ] && [ -n "$camera_dev" ]; then
        rule=$(create_rule_for_device "$camera_dev" "usb_camera")
        if [ -n "$rule" ]; then
            rules+="# USB Camera\n$rule\n\n"
            echo -e "${GREEN}✓ Will create /dev/usb_camera → $camera_dev${NC}"
        fi
    fi

    if [ -z "$rules" ]; then
        echo -e "${YELLOW}No rules to create.${NC}"
        exit 0
    fi

    echo ""
    echo -e "${YELLOW}The following rules will be written to $RULES_FILE:${NC}"
    echo ""
    echo -e "$rules"
    echo ""
    echo "Proceed? (y/n)"
    read -r confirm

    if [ "$confirm" = "y" ] || [ "$confirm" = "Y" ]; then
        echo -e "$rules" | sudo tee "$RULES_FILE" > /dev/null
        sudo udevadm control --reload-rules
        sudo udevadm trigger

        echo ""
        echo -e "${GREEN}✓ udev rules installed!${NC}"
        echo ""
        echo "The symlinks will be created when you replug the devices or reboot."
        echo ""
        echo -e "${YELLOW}To use with HyperCalibrate, update your config:${NC}"
        echo "  INPUT_DEVICE=/dev/hdmi_capture"
        echo ""
        echo "Or edit /opt/hypercalibrate/config.toml:"
        echo '  input_device = "/dev/hdmi_capture"'
    else
        echo "Cancelled."
    fi
}

# Main
case "${1:-}" in
    --list|-l)
        list_video_devices
        ;;
    --remove|-r)
        remove_rules
        ;;
    --help|-h)
        echo "Usage: $0 [--list|--remove|--help]"
        echo ""
        echo "  (no args)   Interactive setup - detect devices and create rules"
        echo "  --list      List current video devices with their info"
        echo "  --remove    Remove HyperCalibrate udev rules"
        echo "  --help      Show this help"
        ;;
    *)
        interactive_setup
        ;;
esac
