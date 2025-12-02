#!/usr/bin/env -S uv run

# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "opencv-python",
# ]
# ///

import argparse
import json
import time
from pathlib import Path

import cv2


PROFILE_FILE_DEFAULT = "camera_profiles.json"

# Common resolutions / FPS / formats to cycle through.
RESOLUTIONS = [
    (640, 480),
    (800, 600),
    (1280, 720),
    (1920, 1080),
]

FPS_OPTIONS = [15, 30, 60, 120]

# Typical USB camera pixel formats. Support is device-dependent.
FOURCC_OPTIONS = [
    "MJPG",  # Motion-JPEG (often fastest / highest FPS)
    "YUYV",  # Packed YUV 4:2:2
    "YUY2",  # Alias for YUYV on some devices
    "H264",  # Compressed H.264 stream (if supported)
]

AUTO_EXPOSURE_MANUAL = 1.0
AUTO_EXPOSURE_AUTO = 3.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="USB camera benchmarking tool: tweak settings and measure FPS/bitrate.",
    )
    parser.add_argument("--device", "-d", type=int, default=0, help="Camera device index (default: 0)")
    parser.add_argument(
        "--profile",
        "-p",
        type=str,
        default=None,
        help=(
            "Profile name to load/save settings. "
            "Press 's' in the viewer to save current settings to this profile."
        ),
    )
    parser.add_argument(
        "--profiles-file",
        type=str,
        default=None,
        help="Optional path to JSON file for storing profiles (default: camera_profiles.json next to script)",
    )
    parser.add_argument(
        "--backend",
        type=str,
        choices=["any", "v4l2"],
        default="v4l2",
        help="Preferred capture backend (default: v4l2 on Linux, falls back to any if unavailable)",
    )
    parser.add_argument(
        "--headless",
        action="store_true",
        help="Run in headless mode (no window) to benchmark raw capture FPS.",
    )
    parser.add_argument(
        "--sweep",
        action="store_true",
        help="In headless mode, sweep over resolution/FPS/format combinations and print a summary table.",
    )
    parser.add_argument(
        "--duration",
        type=float,
        default=10.0,
        help="Duration in seconds for headless benchmark (default: 10.0).",
    )
    return parser.parse_args()


def get_profiles_path(args: argparse.Namespace) -> Path:
    if args.profiles_file:
        return Path(args.profiles_file).expanduser()
    return Path(__file__).resolve().parent / PROFILE_FILE_DEFAULT


def load_profiles(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        with path.open("r", encoding="utf-8") as f:
            return json.load(f)
    except Exception as exc:  # pragma: no cover - defensive
        print(f"[WARN] Failed to load profiles from {path}: {exc}")
        return {}


def save_profiles(path: Path, profiles: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as f:
        json.dump(profiles, f, indent=2, sort_keys=True)
    tmp.replace(path)


def make_default_settings(device_index: int, backend: str) -> dict:
    return {
        "device_index": int(device_index),
        "backend": backend,
        "width": 1280,
        "height": 720,
        "fps": 30.0,
        "fourcc": "MJPG",
    }


def fourcc_to_str(v: int) -> str:
    if v == 0:
        return "????"
    chars = [chr((v >> (8 * i)) & 0xFF) for i in range(4)]
    return "".join(chars)


def open_capture(device_index: int, backend: str) -> cv2.VideoCapture:
    if backend == "v4l2":
        cap = cv2.VideoCapture(device_index, cv2.CAP_V4L2)
        if not cap.isOpened():
            print("[INFO] Failed to open camera with CAP_V4L2, falling back to default backend.")
            cap.release()
            cap = cv2.VideoCapture(device_index)
    else:
        cap = cv2.VideoCapture(device_index)
    return cap


def apply_settings(cap: cv2.VideoCapture, settings: dict) -> dict:
    width = int(settings.get("width", 0))
    height = int(settings.get("height", 0))
    fps = float(settings.get("fps", 0.0))
    fourcc_str = settings.get("fourcc")

    if fourcc_str:
        fourcc = cv2.VideoWriter_fourcc(*fourcc_str)
        cap.set(cv2.CAP_PROP_FOURCC, fourcc)

    if width > 0:
        cap.set(cv2.CAP_PROP_FRAME_WIDTH, width)
    if height > 0:
        cap.set(cv2.CAP_PROP_FRAME_HEIGHT, height)
    if fps > 0:
        cap.set(cv2.CAP_PROP_FPS, fps)

    actual = {
        "width": int(cap.get(cv2.CAP_PROP_FRAME_WIDTH)),
        "height": int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT)),
        "fps": float(cap.get(cv2.CAP_PROP_FPS)),
        "fourcc": fourcc_to_str(int(cap.get(cv2.CAP_PROP_FOURCC))),
    }

    print(
        "[INFO] Applied settings -> requested: %dx%d @ %.1f FPS, %s; actual: %dx%d @ %.1f FPS, %s"
        % (
            width,
            height,
            fps,
            fourcc_str,
            actual["width"],
            actual["height"],
            actual["fps"],
            actual["fourcc"],
        )
    )

    return actual


def find_index(options, value, default_index: int = 0) -> int:
    try:
        return options.index(value)
    except ValueError:
        return default_index


def print_controls() -> None:
    print("Camera benchmark controls:")
    print("  q / ESC : quit")
    print("  r       : cycle resolution")
    print("  f       : cycle target FPS")
    print("  c       : cycle pixel format (FOURCC)")
    print("  a       : toggle auto-exposure")
    print("  z/x     : exposure down/up")
    print("  v/b     : gain down/up")
    print("  s       : save current settings to profile (see --profile)")
    print("  l       : reload current profile from disk")
    print("  h       : print this help text again")


def update_exposure_settings(cap: cv2.VideoCapture, settings: dict) -> None:
    auto_raw = cap.get(cv2.CAP_PROP_AUTO_EXPOSURE)
    if auto_raw == AUTO_EXPOSURE_MANUAL:
        mode = "manual"
    elif auto_raw == AUTO_EXPOSURE_AUTO:
        mode = "auto"
    else:
        mode = f"value:{auto_raw:.2f}"
    settings["auto_exposure"] = mode
    settings["exposure"] = float(cap.get(cv2.CAP_PROP_EXPOSURE))
    settings["gain"] = float(cap.get(cv2.CAP_PROP_GAIN))


def apply_profile_exposure(cap: cv2.VideoCapture, settings: dict) -> None:
    mode = settings.get("auto_exposure")
    if mode == "manual":
        cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, AUTO_EXPOSURE_MANUAL)
    elif mode == "auto":
        cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, AUTO_EXPOSURE_AUTO)

    exposure = settings.get("exposure")
    if exposure is not None:
        try:
            cap.set(cv2.CAP_PROP_EXPOSURE, float(exposure))
        except (TypeError, ValueError):
            pass

    gain = settings.get("gain")
    if gain is not None:
        try:
            cap.set(cv2.CAP_PROP_GAIN, float(gain))
        except (TypeError, ValueError):
            pass


def overlay_info(frame, stats: dict, settings: dict, actual: dict):
    overlay = frame.copy()

    lines = [
        f"Device {settings.get('device_index', '?')}  Backend: {settings.get('backend', 'any')}",
        (
            f"Requested: {settings.get('width')}x{settings.get('height')} @ {settings.get('fps'):.1f} FPS, "
            f"{settings.get('fourcc')}"
        ),
        (
            f"Actual:    {actual.get('width')}x{actual.get('height')} @ {actual.get('fps'):.1f} FPS, "
            f"{actual.get('fourcc')}"
        ),
        (
            f"Frames: {stats['frame_count']}  Inst FPS: {stats['inst_fps']:.1f}  "
            f"EMA FPS: {stats['ema_fps']:.1f}  Avg FPS: {stats['avg_fps']:.1f}"
        ),
        (
            f"Exposure: mode={settings.get('auto_exposure', 'n/a')}  "
            f"exp={settings.get('exposure', 'n/a')}  "
            f"gain={settings.get('gain', 'n/a')}"
        ),
        f"Est data rate: {stats['mbps']:.2f} Mbit/s (approx, assuming 3 bytes/pixel)",
        "Controls: q=quit  r=next res  f=next fps  c=next format  a=auto-exp  z/x=exp-/+  v/b=gain-/+  s=save profile  l=reload profile  h=help",
    ]

    y0 = 20
    dy = 22

    hud_height = y0 + dy * len(lines) + 8
    hud_height = min(hud_height, overlay.shape[0])
    dark = overlay.copy()
    cv2.rectangle(dark, (0, 0), (overlay.shape[1], hud_height), (0, 0, 0), -1)
    cv2.addWeighted(dark, 0.7, overlay, 0.3, 0, overlay)

    for i, text in enumerate(lines):
        y = y0 + i * dy
        cv2.putText(
            overlay,
            text,
            (10, y),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.5,
            (220, 220, 220),
            1,
            cv2.LINE_AA,
        )

    return overlay


def run_headless_benchmark(cap: cv2.VideoCapture, settings: dict, actual: dict, duration: float, quiet: bool = False):
    frame_count = 0
    session_start = time.monotonic()
    last_time = session_start
    ema_fps = 0.0
    if not quiet:
        print(f"[INFO] Starting headless benchmark for {duration:.1f} seconds")
    while True:
        now = time.monotonic()
        if now - session_start >= duration:
            break
        ret, frame = cap.read()
        if not ret:
            print("[WARN] Failed to read frame from camera.")
            break
        frame_count += 1
        dt = now - last_time
        last_time = now
        inst_fps = 1.0 / dt if dt > 0 else 0.0
        alpha = 0.1
        ema_fps = inst_fps if ema_fps == 0.0 else (alpha * inst_fps + (1.0 - alpha) * ema_fps)
    elapsed = time.monotonic() - session_start
    avg_fps = frame_count / elapsed if elapsed > 0 else 0.0
    width = actual.get("width", 0)
    height = actual.get("height", 0)
    mbps = 0.0
    if width > 0 and height > 0 and avg_fps > 0:
        bytes_per_frame = width * height * 3
        mbps = (bytes_per_frame * avg_fps * 8.0) / 1e6

    if quiet:
        return {
            "elapsed": elapsed,
            "frame_count": frame_count,
            "avg_fps": avg_fps,
            "ema_fps": ema_fps,
            "mbps": mbps,
        }

    print("[INFO] Headless benchmark results:")
    print(f"  Duration: {elapsed:.3f} s (requested {duration:.3f} s)")
    print(f"  Frames:   {frame_count}")
    print(f"  Avg FPS:  {avg_fps:.2f}")
    print(f"  EMA FPS:  {ema_fps:.2f}")
    if mbps > 0:
        print(f"  Approx data rate: {mbps:.2f} Mbit/s (assuming 3 bytes/pixel)")
    return 0


def run_headless_sweep(cap: cv2.VideoCapture, base_settings: dict, duration: float) -> int:
    total = len(RESOLUTIONS) * len(FPS_OPTIONS) * len(FOURCC_OPTIONS)
    print(
        f"[INFO] Starting headless sweep over {total} combinations "
        f"(~{total * duration:.1f} seconds, duration={duration:.1f}s each)",
    )
    header = (
        "req_res        req_fps  req_fmt  "
        "act_res        act_fps  act_fmt  "
        "avg_fps  ema_fps  mbps"
    )
    print(header)
    print("-" * len(header))

    for width_req, height_req in RESOLUTIONS:
        for fourcc_req in FOURCC_OPTIONS:
            if fourcc_req not in ["MJPG"]:
                continue

            for fps_req in FPS_OPTIONS:

                if fps_req not in [30.0, 60.0, 120.0]:
                    continue

                settings = dict(base_settings)
                settings["width"] = width_req
                settings["height"] = height_req
                settings["fps"] = float(fps_req)
                settings["fourcc"] = fourcc_req

                actual = apply_settings(cap, settings)
                update_exposure_settings(cap, settings)

                stats = run_headless_benchmark(cap, settings, actual, duration, quiet=True)
                elapsed = stats["elapsed"]
                frame_count = stats["frame_count"]
                avg_fps = stats["avg_fps"]
                ema_fps = stats["ema_fps"]
                mbps = stats["mbps"]

                width_act = actual.get("width", 0)
                height_act = actual.get("height", 0)
                fps_act = actual.get("fps", 0.0)
                fourcc_act = actual.get("fourcc", "????")

                row = (
                    f"{width_req}x{height_req:<9} "
                    f"{fps_req:7.1f} "
                    f"{fourcc_req:7} "
                    f"{width_act}x{height_act:<9} "
                    f"{fps_act:7.1f} "
                    f"{fourcc_act:7} "
                    f"{avg_fps:7.2f} "
                    f"{ema_fps:7.2f} "
                    f"{mbps:7.2f}"
                )
                print(row)

    return 0


def main() -> int:
    args = parse_args()
    profiles_path = get_profiles_path(args)
    profiles = load_profiles(profiles_path)

    profile_name = args.profile

    if profile_name and profile_name in profiles:
        settings = profiles[profile_name]
        print(f"[INFO] Loaded profile '{profile_name}' from {profiles_path}")
    else:
        if profile_name and profile_name not in profiles:
            print(
                f"[INFO] Profile '{profile_name}' not found in {profiles_path}, "
                "starting from default settings.",
            )
        settings = make_default_settings(args.device, args.backend)

    # Ensure required fields exist / are typed correctly.
    settings.setdefault("device_index", int(args.device))
    settings.setdefault("backend", args.backend)
    settings.setdefault("width", 1280)
    settings.setdefault("height", 720)
    settings.setdefault("fps", 30.0)
    settings.setdefault("fourcc", "MJPG")
    settings.setdefault("auto_exposure", None)
    settings.setdefault("exposure", None)
    settings.setdefault("gain", None)

    # Open the camera.
    cap = open_capture(int(settings.get("device_index", args.device)), settings.get("backend", args.backend))
    if not cap.isOpened():
        print("[ERROR] Failed to open camera device.")
        return 1

    actual = apply_settings(cap, settings)
    apply_profile_exposure(cap, settings)
    update_exposure_settings(cap, settings)

    if args.headless and args.sweep:
        try:
            return run_headless_sweep(cap, settings, args.duration)
        finally:
            cap.release()
            cv2.destroyAllWindows()

    if args.headless:
        try:
            return run_headless_benchmark(cap, settings, actual, args.duration)
        finally:
            cap.release()
            cv2.destroyAllWindows()

    res_index = find_index(RESOLUTIONS, (int(settings["width"]), int(settings["height"])))
    fps_index = find_index(FPS_OPTIONS, int(round(float(settings["fps"]))))
    fmt_index = find_index(FOURCC_OPTIONS, settings.get("fourcc", FOURCC_OPTIONS[0]))

    frame_count = 0
    session_start = time.monotonic()
    last_time = session_start
    ema_fps = 0.0

    window_name = "Camera Benchmark"
    cv2.namedWindow(window_name, cv2.WINDOW_NORMAL)

    print_controls()

    try:
        while True:
            now = time.monotonic()
            ret, frame = cap.read()
            if not ret:
                print("[WARN] Failed to read frame from camera.")
                break

            frame_count += 1
            dt = now - last_time
            last_time = now

            inst_fps = 1.0 / dt if dt > 0 else 0.0
            alpha = 0.1
            ema_fps = inst_fps if ema_fps == 0.0 else (alpha * inst_fps + (1.0 - alpha) * ema_fps)
            avg_fps = frame_count / (now - session_start) if now > session_start else 0.0

            height, width = frame.shape[:2]
            channels = frame.shape[2] if len(frame.shape) == 3 else 1
            bytes_per_frame = width * height * channels
            mbps = (bytes_per_frame * ema_fps * 8.0) / 1e6 if ema_fps > 0 else 0.0

            stats = {
                "frame_count": frame_count,
                "inst_fps": inst_fps,
                "ema_fps": ema_fps,
                "avg_fps": avg_fps,
                "mbps": mbps,
            }

            vis = overlay_info(frame, stats, settings, actual)
            cv2.imshow(window_name, vis)

            key = cv2.waitKey(1) & 0xFF
            if key == 27 or key == ord("q"):
                # ESC or 'q' -> quit
                break
            elif key == ord("h"):
                print_controls()
            elif key == ord("r"):
                # Cycle resolution.
                res_index = (res_index + 1) % len(RESOLUTIONS)
                new_w, new_h = RESOLUTIONS[res_index]
                settings["width"] = new_w
                settings["height"] = new_h
                actual = apply_settings(cap, settings)
                update_exposure_settings(cap, settings)
                frame_count = 0
                session_start = time.monotonic()
                last_time = session_start
                ema_fps = 0.0
            elif key == ord("f"):
                # Cycle target FPS.
                fps_index = (fps_index + 1) % len(FPS_OPTIONS)
                new_fps = FPS_OPTIONS[fps_index]
                settings["fps"] = float(new_fps)
                actual = apply_settings(cap, settings)
                update_exposure_settings(cap, settings)
                frame_count = 0
                session_start = time.monotonic()
                last_time = session_start
                ema_fps = 0.0
            elif key == ord("c"):
                # Cycle pixel format.
                fmt_index = (fmt_index + 1) % len(FOURCC_OPTIONS)
                new_fmt = FOURCC_OPTIONS[fmt_index]
                settings["fourcc"] = new_fmt
                actual = apply_settings(cap, settings)
                update_exposure_settings(cap, settings)
                frame_count = 0
                session_start = time.monotonic()
                last_time = session_start
                ema_fps = 0.0
            elif key == ord("a"):
                mode = settings.get("auto_exposure")
                if mode == "auto":
                    cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, AUTO_EXPOSURE_MANUAL)
                else:
                    cap.set(cv2.CAP_PROP_AUTO_EXPOSURE, AUTO_EXPOSURE_AUTO)
                update_exposure_settings(cap, settings)
            elif key == ord("z"):
                value = cap.get(cv2.CAP_PROP_EXPOSURE)
                cap.set(cv2.CAP_PROP_EXPOSURE, value - 1.0)
                update_exposure_settings(cap, settings)
            elif key == ord("x"):
                value = cap.get(cv2.CAP_PROP_EXPOSURE)
                cap.set(cv2.CAP_PROP_EXPOSURE, value + 1.0)
                update_exposure_settings(cap, settings)
            elif key == ord("v"):
                value = cap.get(cv2.CAP_PROP_GAIN)
                cap.set(cv2.CAP_PROP_GAIN, max(0.0, value - 1.0))
                update_exposure_settings(cap, settings)
            elif key == ord("b"):
                value = cap.get(cv2.CAP_PROP_GAIN)
                cap.set(cv2.CAP_PROP_GAIN, value + 1.0)
                update_exposure_settings(cap, settings)
            elif key == ord("s"):
                # Save current settings to profile.
                if not profile_name:
                    # Default profile name if none provided.
                    profile_name = f"device_{settings.get('device_index', args.device)}"
                    print(
                        f"[INFO] No --profile given, saving under default profile name '{profile_name}'.",
                    )

                profiles[profile_name] = {
                    "device_index": int(settings.get("device_index", args.device)),
                    "backend": settings.get("backend", args.backend),
                    "width": int(settings.get("width", actual.get("width", 0))),
                    "height": int(settings.get("height", actual.get("height", 0))),
                    "fps": float(settings.get("fps", actual.get("fps", 0.0))),
                    "fourcc": settings.get("fourcc", actual.get("fourcc", "MJPG")),
                    "auto_exposure": settings.get("auto_exposure"),
                    "exposure": settings.get("exposure"),
                    "gain": settings.get("gain"),
                }
                save_profiles(profiles_path, profiles)
                print(f"[INFO] Saved profile '{profile_name}' to {profiles_path}")
            elif key == ord("l"):
                # Reload current profile from disk.
                if not profile_name:
                    print("[INFO] No profile name associated with this run; nothing to reload.")
                else:
                    profiles = load_profiles(profiles_path)
                    if profile_name not in profiles:
                        print(
                            f"[WARN] Profile '{profile_name}' not found in {profiles_path}; "
                            "nothing to reload.",
                        )
                    else:
                        settings = profiles[profile_name]
                        print(f"[INFO] Reloaded profile '{profile_name}' from {profiles_path}")
                        # Re-apply settings and reset stats.
                        actual = apply_settings(cap, settings)
                        apply_profile_exposure(cap, settings)
                        update_exposure_settings(cap, settings)
                        res_index = find_index(
                            RESOLUTIONS,
                            (int(settings["width"]), int(settings["height"])),
                        )
                        fps_index = find_index(
                            FPS_OPTIONS,
                            int(round(float(settings["fps"]))),
                        )
                        fmt_index = find_index(
                            FOURCC_OPTIONS,
                            settings.get("fourcc", FOURCC_OPTIONS[0]),
                        )
                        frame_count = 0
                        session_start = time.monotonic()
                        last_time = session_start
                        ema_fps = 0.0
    finally:
        cap.release()
        cv2.destroyAllWindows()

    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
