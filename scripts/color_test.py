#!/usr/bin/env python3
"""
Color Correction Test Script for HyperCalibrate

This script captures frames from the HyperCalibrate preview with different
color correction presets and analyzes the RGB values to verify the color
matrix is having an effect.
"""

import requests
import time
import io
import sys
from collections import defaultdict

# Try to import PIL, fall back to basic analysis if not available
try:
    from PIL import Image
    import numpy as np
    HAS_PIL = True
except ImportError:
    HAS_PIL = False
    print("Note: PIL/numpy not available, using basic frame size analysis")

BASE_URL = "http://192.168.50.146:8091"

PRESETS = [
    "passthrough",
    "hd_standard",    # BT.709, Limited
    "sd_legacy",      # BT.601, Limited
    "pc_gaming",      # BT.709, Full
]

def get_color_settings():
    """Get current color correction settings"""
    resp = requests.get(f"{BASE_URL}/api/color")
    return resp.json()

def apply_preset(preset_name):
    """Apply a color correction preset"""
    resp = requests.post(f"{BASE_URL}/api/color/preset/{preset_name}")
    if resp.status_code == 200:
        return {"success": True}
    return {"success": False, "error": resp.text}

def activate_preview():
    """Activate preview encoding"""
    requests.post(f"{BASE_URL}/api/preview/activate")
    time.sleep(0.5)  # Wait for preview to start

def deactivate_preview():
    """Deactivate preview encoding"""
    requests.post(f"{BASE_URL}/api/preview/deactivate")

def capture_frame():
    """Capture a single frame from the preview endpoint"""
    resp = requests.get(f"{BASE_URL}/api/preview", stream=True)
    if resp.status_code != 200:
        return None
    return resp.content

def analyze_frame_basic(frame_data):
    """Basic analysis - just return frame size"""
    return {
        "size": len(frame_data),
    }

def analyze_frame_pil(frame_data):
    """Analyze frame using PIL/numpy"""
    try:
        img = Image.open(io.BytesIO(frame_data))
        arr = np.array(img)

        # Calculate statistics
        r_mean = arr[:,:,0].mean()
        g_mean = arr[:,:,1].mean()
        b_mean = arr[:,:,2].mean()

        r_std = arr[:,:,0].std()
        g_std = arr[:,:,1].std()
        b_std = arr[:,:,2].std()

        # Sample center region (middle 50%)
        h, w = arr.shape[:2]
        center = arr[h//4:3*h//4, w//4:3*w//4]

        r_center = center[:,:,0].mean()
        g_center = center[:,:,1].mean()
        b_center = center[:,:,2].mean()

        return {
            "size": len(frame_data),
            "dimensions": f"{w}x{h}",
            "r_mean": round(r_mean, 2),
            "g_mean": round(g_mean, 2),
            "b_mean": round(b_mean, 2),
            "r_std": round(r_std, 2),
            "g_std": round(g_std, 2),
            "b_std": round(b_std, 2),
            "r_center": round(r_center, 2),
            "g_center": round(g_center, 2),
            "b_center": round(b_center, 2),
            "luminance": round(0.299*r_mean + 0.587*g_mean + 0.114*b_mean, 2),
        }
    except Exception as e:
        return {"error": str(e)}

def analyze_frame(frame_data):
    """Analyze a frame"""
    if HAS_PIL:
        return analyze_frame_pil(frame_data)
    return analyze_frame_basic(frame_data)

def run_test():
    """Run the color correction test"""
    print("=" * 60)
    print("HyperCalibrate Color Correction Test")
    print("=" * 60)

    # Check current format
    resp = requests.get(f"{BASE_URL}/api/video/format")
    format_info = resp.json()
    print(f"\nCapture Format: {format_info['format'].upper()}")
    print(f"Description: {format_info['description']}")

    if format_info['format'] == 'mjpeg':
        print("\n⚠️  WARNING: Using MJPEG format - color matrix may not have effect!")
        print("   Switch to YUYV format for proper color matrix control.")

    print("\n" + "-" * 60)
    print("Testing color presets...")
    print("-" * 60)

    # Activate preview encoding
    print("\nActivating preview...")
    activate_preview()

    results = {}

    for preset in PRESETS:
        print(f"\n📊 Testing preset: {preset}")

        # Apply preset
        result = apply_preset(preset)
        if not result.get("success"):
            print(f"   ❌ Failed to apply preset: {result}")
            continue

        # Wait for settings to take effect (need longer for capture loop to pick up changes)
        time.sleep(1.5)

        # Capture multiple frames and average
        frames_data = []
        for i in range(3):
            frame = capture_frame()
            if frame:
                analysis = analyze_frame(frame)
                if "error" not in analysis:
                    frames_data.append(analysis)
            time.sleep(0.1)

        if not frames_data:
            print(f"   ❌ Failed to capture frames")
            continue

        # Average the results
        if HAS_PIL and frames_data:
            avg = {}
            for key in frames_data[0]:
                if isinstance(frames_data[0][key], (int, float)):
                    avg[key] = round(sum(f[key] for f in frames_data) / len(frames_data), 2)
                else:
                    avg[key] = frames_data[0][key]
            results[preset] = avg

            print(f"   R: {avg['r_mean']:.1f} (±{avg['r_std']:.1f})  center: {avg['r_center']:.1f}")
            print(f"   G: {avg['g_mean']:.1f} (±{avg['g_std']:.1f})  center: {avg['g_center']:.1f}")
            print(f"   B: {avg['b_mean']:.1f} (±{avg['b_std']:.1f})  center: {avg['b_center']:.1f}")
            print(f"   Luminance: {avg['luminance']:.1f}")
        else:
            results[preset] = frames_data[0] if frames_data else {}
            print(f"   Frame size: {results[preset].get('size', 'N/A')} bytes")

    # Compare results
    print("\n" + "=" * 60)
    print("COMPARISON RESULTS")
    print("=" * 60)

    if len(results) >= 2 and HAS_PIL:
        # Calculate differences between presets
        preset_names = list(results.keys())

        print("\nColor differences between presets:")
        print("-" * 40)

        for i, p1 in enumerate(preset_names):
            for p2 in preset_names[i+1:]:
                r1, r2 = results[p1], results[p2]

                r_diff = abs(r1['r_mean'] - r2['r_mean'])
                g_diff = abs(r1['g_mean'] - r2['g_mean'])
                b_diff = abs(r1['b_mean'] - r2['b_mean'])
                lum_diff = abs(r1['luminance'] - r2['luminance'])

                total_diff = r_diff + g_diff + b_diff

                print(f"\n{p1} vs {p2}:")
                print(f"  ΔR: {r_diff:.1f}  ΔG: {g_diff:.1f}  ΔB: {b_diff:.1f}")
                print(f"  ΔLuminance: {lum_diff:.1f}")
                print(f"  Total color shift: {total_diff:.1f}")

                if total_diff < 5:
                    print(f"  ⚠️  Very small difference - presets may not be working!")
                elif total_diff < 15:
                    print(f"  ℹ️  Moderate difference detected")
                else:
                    print(f"  ✅ Significant color difference - presets are working!")

        # Summary
        print("\n" + "=" * 60)
        print("SUMMARY")
        print("=" * 60)

        # Check if hd_standard vs sd_legacy shows difference (BT.709 vs BT.601)
        if "hd_standard" in results and "sd_legacy" in results:
            hd = results["hd_standard"]
            sd = results["sd_legacy"]
            matrix_diff = abs(hd['r_mean'] - sd['r_mean']) + abs(hd['g_mean'] - sd['g_mean']) + abs(hd['b_mean'] - sd['b_mean'])

            print(f"\nBT.709 vs BT.601 matrix difference: {matrix_diff:.1f}")
            if matrix_diff > 5:
                print("✅ Color matrices ARE having an effect!")
            else:
                print("⚠️  Color matrices show minimal difference")

        # Check if pc_gaming vs hd_standard shows difference (Full vs Limited range)
        if "pc_gaming" in results and "hd_standard" in results:
            pc = results["pc_gaming"]
            hd = results["hd_standard"]
            range_diff = abs(pc['luminance'] - hd['luminance'])

            print(f"\nFull vs Limited range luminance difference: {range_diff:.1f}")
            if range_diff > 3:
                print("✅ Range expansion IS working!")
            else:
                print("⚠️  Range expansion shows minimal difference")

    print("\n" + "=" * 60)
    print("Test complete!")
    print("=" * 60)

    return results

if __name__ == "__main__":
    try:
        results = run_test()
    except requests.exceptions.ConnectionError:
        print("❌ Could not connect to HyperCalibrate at", BASE_URL)
        print("   Make sure the service is running.")
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nTest cancelled.")
        sys.exit(0)
