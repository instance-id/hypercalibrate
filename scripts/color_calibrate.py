#!/usr/bin/env python3
"""
Color Calibration Helper for HyperCalibrate

This script helps find the correct color settings by:
1. Displaying a reference color on screen (via browser)
2. Capturing what the camera sees
3. Comparing the captured color to the expected color
4. Suggesting adjustments

For best results, display a solid color test pattern on your TV.
"""

import requests
import time
import io
import sys
import json

try:
    from PIL import Image
    import numpy as np
    HAS_PIL = True
except ImportError:
    print("Error: PIL and numpy are required. Install with: pip install pillow numpy")
    sys.exit(1)

BASE_URL = "http://192.168.50.146:8091"

# Standard test colors (sRGB values)
TEST_COLORS = {
    "red": (255, 0, 0),
    "green": (0, 255, 0),
    "blue": (0, 0, 255),
    "white": (255, 255, 255),
    "gray50": (128, 128, 128),
    "yellow": (255, 255, 0),
    "cyan": (0, 255, 255),
    "magenta": (255, 0, 255),
}

def activate_preview():
    requests.post(f"{BASE_URL}/api/preview/activate")
    time.sleep(0.5)

def get_color_settings():
    resp = requests.get(f"{BASE_URL}/api/color")
    return resp.json()

def set_color_settings(color_space, input_range):
    """Set color correction settings"""
    settings = get_color_settings()['settings']
    settings['color_space'] = color_space
    settings['input_range'] = input_range
    settings['enabled'] = True

    resp = requests.post(f"{BASE_URL}/api/color", json=settings)
    time.sleep(0.5)
    return resp.status_code == 200

def capture_center_color():
    """Capture a frame and return the average color of the center region"""
    resp = requests.get(f"{BASE_URL}/api/preview")
    if resp.status_code != 200:
        return None

    img = Image.open(io.BytesIO(resp.content))
    arr = np.array(img)

    h, w = arr.shape[:2]
    # Sample center 20% of image
    margin_h = int(h * 0.4)
    margin_w = int(w * 0.4)
    center = arr[margin_h:h-margin_h, margin_w:w-margin_w]

    r = int(center[:,:,0].mean())
    g = int(center[:,:,1].mean())
    b = int(center[:,:,2].mean())

    return (r, g, b)

def color_distance(c1, c2):
    """Calculate Euclidean distance between two RGB colors"""
    return ((c1[0]-c2[0])**2 + (c1[1]-c2[1])**2 + (c1[2]-c2[2])**2) ** 0.5

def analyze_color_error(expected, captured):
    """Analyze the color error and suggest corrections"""
    r_err = captured[0] - expected[0]
    g_err = captured[1] - expected[1]
    b_err = captured[2] - expected[2]

    # Check for common issues
    issues = []

    # Brightness/range issue
    avg_expected = sum(expected) / 3
    avg_captured = sum(captured) / 3
    brightness_ratio = avg_captured / max(avg_expected, 1)

    if brightness_ratio < 0.85:
        issues.append("Image appears darker than expected - try 'Limited' input range")
    elif brightness_ratio > 1.15:
        issues.append("Image appears brighter than expected - try 'Full' input range")

    # Color tint
    if abs(r_err - g_err) > 15 or abs(r_err - b_err) > 15 or abs(g_err - b_err) > 15:
        if r_err > g_err and r_err > b_err:
            issues.append("Red tint detected")
        elif g_err > r_err and g_err > b_err:
            issues.append("Green tint detected")
        elif b_err > r_err and b_err > g_err:
            issues.append("Blue tint detected")

        # Matrix mismatch often shows as color tint
        issues.append("Color tint may indicate wrong color matrix - try switching between BT.601 and BT.709")

    return issues

def run_calibration():
    print("=" * 60)
    print("HyperCalibrate Color Calibration Helper")
    print("=" * 60)

    print("\nInstructions:")
    print("1. Display a SOLID COLOR on your TV (e.g., full-screen red, white, etc.)")
    print("2. This tool will capture what the camera sees")
    print("3. Compare to find the best color settings")
    print()

    activate_preview()

    # Test all combinations
    color_spaces = ["bt601", "bt709"]
    ranges = ["limited", "full"]

    print("Testing all color space / range combinations...")
    print("-" * 60)

    results = []

    for cs in color_spaces:
        for rng in ranges:
            set_color_settings(cs, rng)
            time.sleep(0.3)

            # Capture multiple samples
            colors = []
            for _ in range(3):
                c = capture_center_color()
                if c:
                    colors.append(c)
                time.sleep(0.1)

            if colors:
                avg_r = int(sum(c[0] for c in colors) / len(colors))
                avg_g = int(sum(c[1] for c in colors) / len(colors))
                avg_b = int(sum(c[2] for c in colors) / len(colors))

                result = {
                    "color_space": cs,
                    "input_range": rng,
                    "captured": (avg_r, avg_g, avg_b),
                }
                results.append(result)

                print(f"{cs.upper():6} + {rng.upper():7}: RGB({avg_r:3}, {avg_g:3}, {avg_b:3})")

    print("\n" + "=" * 60)
    print("ANALYSIS")
    print("=" * 60)

    # Ask user what color they're displaying
    print("\nWhat color is currently displayed on your TV?")
    print("Options: red, green, blue, white, gray50, yellow, cyan, magenta")
    print("Or enter custom RGB as 'R,G,B' (e.g., '255,128,0')")

    try:
        user_input = input("\nEnter color name or RGB: ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        print("\nUsing white as reference...")
        user_input = "white"

    if user_input in TEST_COLORS:
        expected = TEST_COLORS[user_input]
    elif ',' in user_input:
        try:
            parts = user_input.split(',')
            expected = (int(parts[0]), int(parts[1]), int(parts[2]))
        except:
            print("Invalid RGB format, using white")
            expected = TEST_COLORS["white"]
    else:
        print("Unknown color, using white")
        expected = TEST_COLORS["white"]

    print(f"\nExpected color: RGB{expected}")
    print("-" * 40)

    # Find best match
    best_result = None
    best_distance = float('inf')

    for result in results:
        dist = color_distance(expected, result["captured"])
        result["distance"] = dist
        result["error"] = (
            result["captured"][0] - expected[0],
            result["captured"][1] - expected[1],
            result["captured"][2] - expected[2],
        )

        print(f"{result['color_space'].upper():6} + {result['input_range'].upper():7}: "
              f"RGB{result['captured']} - Error: {result['error']} - Distance: {dist:.1f}")

        if dist < best_distance:
            best_distance = dist
            best_result = result

    print("\n" + "=" * 60)
    print("RECOMMENDATION")
    print("=" * 60)

    if best_result:
        print(f"\n✅ Best match: {best_result['color_space'].upper()} + {best_result['input_range'].upper()}")
        print(f"   Color distance: {best_distance:.1f}")

        if best_distance < 20:
            print("   This is a good match!")
        elif best_distance < 40:
            print("   Acceptable match - may need fine-tuning with brightness/contrast")
        else:
            print("   ⚠️  Large color error - check your source device settings")

        # Apply best settings
        print(f"\nApplying recommended settings...")
        set_color_settings(best_result['color_space'], best_result['input_range'])
        print("Done!")

        # Analyze remaining error
        issues = analyze_color_error(expected, best_result['captured'])
        if issues:
            print("\nPotential issues detected:")
            for issue in issues:
                print(f"  - {issue}")

    print("\n" + "=" * 60)
    print("Tips for better color accuracy:")
    print("-" * 60)
    print("1. Check your source device (Fire TV, etc.) color output settings")
    print("2. Some devices output in different color spaces depending on content")
    print("3. HDR content may need different settings than SDR")
    print("4. The capture card hardware may have its own color processing")
    print("=" * 60)

if __name__ == "__main__":
    try:
        run_calibration()
    except requests.exceptions.ConnectionError:
        print("❌ Could not connect to HyperCalibrate at", BASE_URL)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nCalibration cancelled.")
        sys.exit(0)
