//! Video capture from V4L2 devices
//!
//! This module handles capturing video frames from USB cameras and other V4L2 devices.
//! It supports multiple pixel formats (YUYV, MJPEG, RGB) and automatically selects
//! the best available format.
//!
//! Performance optimizations:
//! - Uses turbojpeg for hardware-accelerated MJPEG decoding (libjpeg-turbo with SIMD)
//! - Integer-only YUYV to RGB conversion (no floating point)
//! - Pre-allocated buffers to avoid allocations in hot path

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::{Device, FourCC};

use crate::output::VirtualCamera;
use crate::server::AppState;

/// Thread-local turbojpeg decompressor for hardware-accelerated MJPEG decoding
thread_local! {
    static JPEG_DECOMPRESSOR: std::cell::RefCell<Option<turbojpeg::Decompressor>> =
        std::cell::RefCell::new(turbojpeg::Decompressor::new().ok());
}

/// Supported pixel formats in order of preference
/// MJPEG is preferred for high FPS cameras - compressed format uses less USB bandwidth
/// and allows higher frame rates. Modern CPUs decode MJPEG very fast with turbojpeg SIMD.
const PREFERRED_FORMATS: &[&[u8; 4]] = &[
    b"MJPG", // Motion JPEG - compressed, enables high FPS (60/120), fast turbojpeg SIMD decode
    b"YUYV", // YUV 4:2:2 - uncompressed, limited to ~30fps at 640x480 due to USB bandwidth
    b"RGB3", // RGB24 - simple but less common
    b"BGR3", // BGR24 - simple but less common
];

/// Run the video capture and processing pipeline
pub fn run_pipeline(
    input_device: &str,
    output_device: &str,
    width: u32,
    height: u32,
    fps: u32,
    state: Arc<AppState>,
) -> Result<()> {
    info!("=== HyperCalibrate Video Pipeline ===");
    info!("Input device: {}", input_device);
    info!("Output device: {}", output_device);
    info!("Requested resolution: {}x{} @ {} fps", width, height, fps);

    // Open the capture device
    let dev = Device::with_path(input_device)
        .with_context(|| format!("Failed to open input device: {}", input_device))?;

    // Query and log device capabilities
    let caps = dev.query_caps()
        .context("Failed to query device capabilities")?;
    info!("Camera: {} (driver: {})", caps.card, caps.driver);

    // Set the format
    let format = configure_capture_format(&dev, width, height)?;
    info!(
        "Capture format: {}x{} {:?}",
        format.width, format.height,
        String::from_utf8_lossy(&format.fourcc.repr)
    );

    // Set frame rate if supported
    if let Err(e) = set_frame_rate(&dev, fps) {
        warn!("Could not set frame rate to {} fps: {}", fps, e);
    }

    // Open the output virtual camera
    let mut output = VirtualCamera::new(output_device, format.width, format.height)
        .with_context(|| format!("Failed to create output device: {}", output_device))?;

    output.initialize()
        .with_context(|| format!("Failed to initialize output device: {}", output_device))?;

    info!("Output: {}", output.info());

    // Initialize camera controls now that the device is open
    if let Err(e) = state.init_camera_controls() {
        warn!("Failed to initialize camera controls: {}", e);
        warn!("Camera control adjustment will not be available");
    } else {
        info!("Camera controls initialized successfully");
    }

    // Create the capture stream with memory mapping
    // Using 4 buffers for smooth capture pipeline
    let mut stream = Stream::with_buffers(&dev, Type::VideoCapture, 4)
        .context("Failed to create capture stream")?;

    info!("Starting capture loop...");

    // Allocate buffers for transformation
    let actual_width = format.width as usize;
    let actual_height = format.height as usize;
    let rgb_size = actual_width * actual_height * 3;
    let yuyv_size = actual_width * actual_height * 2;

    let mut rgb_buffer = vec![0u8; rgb_size];
    let mut output_rgb_buffer = vec![0u8; rgb_size];
    let mut yuyv_buffer = vec![0u8; yuyv_size];

    // Performance tracking
    let mut frame_count = 0u64;
    let mut last_stats_time = Instant::now();
    let stats_interval = Duration::from_secs(10);

    // Determine input format type
    let fourcc_bytes = format.fourcc.repr;
    let is_mjpeg = &fourcc_bytes == b"MJPG";
    let is_yuyv = &fourcc_bytes == b"YUYV";
    let is_rgb = &fourcc_bytes == b"RGB3";
    let is_bgr = &fourcc_bytes == b"BGR3";

    info!("Input format detection: MJPG={}, YUYV={}, RGB={}, BGR={}",
        is_mjpeg, is_yuyv, is_rgb, is_bgr);

    // Warm up turbojpeg decompressor if using MJPEG
    if is_mjpeg {
        JPEG_DECOMPRESSOR.with(|_| {});
        info!("TurboJPEG decompressor initialized");
    }

    loop {
        // Check if camera release has been requested
        if state.is_camera_release_requested() {
            info!("Camera release requested - stopping capture loop");
            // Drop the stream and device to release /dev/video0
            drop(stream);
            state.set_camera_released(true);
            info!("Camera device released - {} is now available for other applications", input_device);

            // Wait in a loop until restart is requested or process exits
            loop {
                std::thread::sleep(Duration::from_millis(500));
                // If restart is requested, exit the function to allow process restart
                if state.is_restart_requested() {
                    info!("Restart requested while camera released - exiting capture loop");
                    return Ok(());
                }
            }
        }

        // Timing: Frame wait (waiting for camera hardware to deliver frame)
        let frame_wait_start = Instant::now();

        // Capture a frame - this blocks until camera delivers a frame
        let (buf, _meta) = stream.next()
            .context("Failed to capture frame")?;

        let frame_wait_us = frame_wait_start.elapsed().as_micros() as u64;

        // Get current transform from shared state
        let transform = state.get_transform();

        // Check if calibration is enabled
        let calibration_enabled = {
            let config = state.config.read();
            config.calibration.enabled
        };

        // Timing: Decode/conversion (MJPEG decode or YUYV→RGB conversion)
        let decode_start = Instant::now();

        // Convert input to RGB for processing
        let working_rgb = if is_mjpeg {
            // Decode MJPEG to RGB
            match decode_mjpeg(buf, &mut rgb_buffer, actual_width, actual_height) {
                Ok(rgb) => rgb,
                Err(_) => {
                    if frame_count % 100 == 0 {
                        warn!("Failed to decode MJPEG frame");
                    }
                    continue;
                }
            }
        } else if is_yuyv {
            // Convert YUYV to RGB
            yuyv_to_rgb(buf, &mut rgb_buffer, actual_width, actual_height);
            &rgb_buffer[..]
        } else if is_bgr {
            // Convert BGR to RGB
            bgr_to_rgb(buf, &mut rgb_buffer);
            &rgb_buffer[..]
        } else {
            // Assume RGB or copy raw
            let copy_len = buf.len().min(rgb_buffer.len());
            rgb_buffer[..copy_len].copy_from_slice(&buf[..copy_len]);
            &rgb_buffer[..]
        };

        let decode_us = decode_start.elapsed().as_micros() as u64;

        // Timing: Transform (perspective warp for calibration)
        let transform_start = Instant::now();

        // Process the frame (apply calibration if enabled)
        let output_rgb = if calibration_enabled {
            transform.warp_rgb(working_rgb, &mut output_rgb_buffer);
            &output_rgb_buffer[..]
        } else {
            working_rgb
        };

        let transform_us = transform_start.elapsed().as_micros() as u64;

        // Timing: Output write
        let output_start = Instant::now();

        // Write to virtual camera (the output module handles format conversion)
        if let Err(e) = output.write_frame_rgb(output_rgb) {
            if frame_count % 100 == 0 {
                warn!("Failed to write to output: {}", e);
            }
        }

        let output_us = output_start.elapsed().as_micros() as u64;

        // Update preview for web UI (only if clients are connected)
        let preview_encode_us = if state.should_encode_preview() && frame_count % 3 == 0 {
            let preview_start = Instant::now();
            state.update_preview(output_rgb, format.width, format.height);
            state.update_raw_preview(working_rgb, format.width, format.height);
            Some(preview_start.elapsed().as_micros() as u64)
        } else {
            None
        };

        // Record stats with separated timings
        state.record_frame_stats(frame_wait_us, decode_us, transform_us, output_us, preview_encode_us);

        frame_count += 1;

        // Log performance stats periodically
        if last_stats_time.elapsed() >= stats_interval {
            let elapsed = last_stats_time.elapsed().as_secs_f64();
            let fps_actual = frame_count as f64 / elapsed;
            let preview_status = if state.should_encode_preview() { "active" } else { "inactive" };
            let dropped = output.dropped_count();
            if dropped > 0 {
                info!("Performance: {:.1} fps ({} frames in {:.1}s, {} dropped, preview {})",
                    fps_actual, frame_count, elapsed, dropped, preview_status);
            } else {
                info!("Performance: {:.1} fps ({} frames in {:.1}s, preview {})",
                    fps_actual, frame_count, elapsed, preview_status);
            }
            frame_count = 0;
            last_stats_time = Instant::now();
        }
    }
}

/// Configure the capture format, trying preferred formats in order
fn configure_capture_format(dev: &Device, width: u32, height: u32) -> Result<v4l::Format> {
    // First, try to enumerate available formats
    let formats = dev.enum_formats()
        .context("Failed to enumerate formats")?;

    info!("Available formats:");
    for fmt in &formats {
        info!("  {:?}: {}", String::from_utf8_lossy(&fmt.fourcc.repr), fmt.description);
    }

    // Try preferred formats in order
    for preferred in PREFERRED_FORMATS {
        let fourcc = FourCC::new(preferred);
        if formats.iter().any(|f| f.fourcc == fourcc) {
            info!("Trying format: {:?} at {}x{}", String::from_utf8_lossy(*preferred), width, height);

            let mut format = dev.format()
                .context("Failed to get current format")?;

            format.width = width;
            format.height = height;
            format.fourcc = fourcc;

            match dev.set_format(&format) {
                Ok(actual) => {
                    info!("Successfully set format to {}x{} {:?}",
                        actual.width, actual.height,
                        String::from_utf8_lossy(&actual.fourcc.repr));
                    return Ok(actual);
                }
                Err(e) => {
                    warn!("Could not set format {:?} at {}x{}: {}",
                        String::from_utf8_lossy(*preferred), width, height, e);
                }
            }
        }
    }

    // Fall back to whatever the device gives us
    info!("Trying device default format at {}x{}", width, height);
    let mut format = dev.format()
        .context("Failed to get current format")?;

    info!("Current device format: {}x{} {:?}", format.width, format.height,
        String::from_utf8_lossy(&format.fourcc.repr));

    format.width = width;
    format.height = height;

    match dev.set_format(&format) {
        Ok(actual) => {
            info!("Using format: {}x{} {:?}", actual.width, actual.height,
                String::from_utf8_lossy(&actual.fourcc.repr));
            Ok(actual)
        }
        Err(e) => {
            // If we can't set the requested resolution, try to use whatever the device supports
            warn!("Could not set resolution {}x{}: {}", width, height, e);
            let current = dev.format().context("Failed to get device format")?;
            info!("Using device's current format: {}x{} {:?}",
                current.width, current.height,
                String::from_utf8_lossy(&current.fourcc.repr));
            Ok(current)
        }
    }
}

/// Set the frame rate on the capture device
fn set_frame_rate(dev: &Device, fps: u32) -> Result<()> {
    // Log current parameters before change
    if let Ok(params) = dev.params() {
        info!("Current frame interval: {}/{} ({:.1} fps)",
            params.interval.numerator,
            params.interval.denominator,
            params.interval.denominator as f64 / params.interval.numerator as f64);
    }

    let mut params = dev.params()
        .context("Failed to get parameters")?;

    params.interval = v4l::Fraction::new(1, fps);

    dev.set_params(&params)
        .context("Failed to set parameters")?;

    // Log actual frame rate after setting
    let actual_params = dev.params().context("Failed to read back parameters")?;
    info!("Set frame interval to: {}/{} ({:.1} fps requested: {})",
        actual_params.interval.numerator,
        actual_params.interval.denominator,
        actual_params.interval.denominator as f64 / actual_params.interval.numerator.max(1) as f64,
        fps);

    Ok(())
}

/// Decode MJPEG frame to RGB using turbojpeg (hardware-accelerated via libjpeg-turbo)
/// Falls back to software jpeg-decoder if turbojpeg fails
fn decode_mjpeg<'a>(mjpeg_data: &[u8], rgb_buffer: &'a mut [u8], width: usize, height: usize) -> Result<&'a [u8], ()> {
    let expected_size = width * height * 3;

    // Try hardware-accelerated turbojpeg first
    let turbo_result = JPEG_DECOMPRESSOR.with(|decomp| {
        if let Some(ref mut decompressor) = *decomp.borrow_mut() {
            // Read JPEG header to get actual dimensions
            if let Ok(header) = decompressor.read_header(mjpeg_data) {
                let jpeg_width = header.width;
                let jpeg_height = header.height;

                // Decompress directly to RGB
                let image = turbojpeg::Image {
                    pixels: &mut rgb_buffer[..expected_size],
                    width: jpeg_width,
                    pitch: jpeg_width * 3,
                    height: jpeg_height,
                    format: turbojpeg::PixelFormat::RGB,
                };

                if decompressor.decompress(mjpeg_data, image).is_ok() {
                    return Some(());
                }
            }
        }
        None
    });

    if turbo_result.is_some() {
        return Ok(&rgb_buffer[..expected_size]);
    }

    // Fallback to software decoder
    decode_mjpeg_software(mjpeg_data, rgb_buffer, width, height)
}

/// Software fallback MJPEG decoder using jpeg-decoder crate
fn decode_mjpeg_software<'a>(mjpeg_data: &[u8], rgb_buffer: &'a mut [u8], width: usize, height: usize) -> Result<&'a [u8], ()> {
    use std::io::Cursor;

    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(mjpeg_data));

    let pixels = decoder.decode().map_err(|_| ())?;
    let info = decoder.info().ok_or(())?;

    let expected_size = width * height * 3;

    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            let copy_len = pixels.len().min(rgb_buffer.len()).min(expected_size);
            rgb_buffer[..copy_len].copy_from_slice(&pixels[..copy_len]);
        }
        jpeg_decoder::PixelFormat::L8 => {
            // Grayscale - convert to RGB
            for (i, &gray) in pixels.iter().enumerate().take(width * height) {
                let rgb_offset = i * 3;
                if rgb_offset + 2 < rgb_buffer.len() {
                    rgb_buffer[rgb_offset] = gray;
                    rgb_buffer[rgb_offset + 1] = gray;
                    rgb_buffer[rgb_offset + 2] = gray;
                }
            }
        }
        _ => {
            // Try to handle as RGB anyway
            let copy_len = pixels.len().min(rgb_buffer.len()).min(expected_size);
            rgb_buffer[..copy_len].copy_from_slice(&pixels[..copy_len]);
        }
    }

    Ok(&rgb_buffer[..expected_size.min(rgb_buffer.len())])
}

/// Convert YUYV to RGB using fast integer math (no floating point)
/// Uses fixed-point arithmetic with 8-bit shift for BT.601 color conversion
/// This is significantly faster than floating-point on ARM processors
#[inline]
pub fn yuyv_to_rgb(yuyv: &[u8], rgb: &mut [u8], width: usize, height: usize) {
    let pixels = width * height;

    // Process 2 pixels at a time (4 bytes YUYV -> 6 bytes RGB)
    for i in 0..(pixels / 2) {
        let yuyv_offset = i * 4;
        let rgb_offset = i * 6;

        if yuyv_offset + 3 >= yuyv.len() || rgb_offset + 5 >= rgb.len() {
            break;
        }

        // Read YUYV values
        let y0 = yuyv[yuyv_offset] as i32;
        let u = yuyv[yuyv_offset + 1] as i32 - 128;
        let y1 = yuyv[yuyv_offset + 2] as i32;
        let v = yuyv[yuyv_offset + 3] as i32 - 128;

        // BT.601 conversion using fixed-point (scaled by 256)
        // R = Y + 1.402 * V           → Y + (359 * V) >> 8
        // G = Y - 0.344 * U - 0.714 * V → Y - (88 * U + 183 * V) >> 8
        // B = Y + 1.772 * U           → Y + (454 * U) >> 8

        // Pre-compute shared UV terms
        let v_r = (359 * v) >> 8;
        let uv_g = (88 * u + 183 * v) >> 8;
        let u_b = (454 * u) >> 8;

        // First pixel
        rgb[rgb_offset] = (y0 + v_r).clamp(0, 255) as u8;
        rgb[rgb_offset + 1] = (y0 - uv_g).clamp(0, 255) as u8;
        rgb[rgb_offset + 2] = (y0 + u_b).clamp(0, 255) as u8;

        // Second pixel (shares U and V)
        rgb[rgb_offset + 3] = (y1 + v_r).clamp(0, 255) as u8;
        rgb[rgb_offset + 4] = (y1 - uv_g).clamp(0, 255) as u8;
        rgb[rgb_offset + 5] = (y1 + u_b).clamp(0, 255) as u8;
    }
}

/// Convert BGR to RGB (swap R and B channels) using chunks for better optimization
#[inline]
pub fn bgr_to_rgb(bgr: &[u8], rgb: &mut [u8]) {
    // Use chunks_exact for better auto-vectorization
    for (bgr_chunk, rgb_chunk) in bgr.chunks_exact(3).zip(rgb.chunks_exact_mut(3)) {
        rgb_chunk[0] = bgr_chunk[2]; // R <- B
        rgb_chunk[1] = bgr_chunk[1]; // G <- G
        rgb_chunk[2] = bgr_chunk[0]; // B <- R
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yuyv_to_rgb() {
        let width = 4;
        let height = 2;
        let yuyv_size = width * height * 2;
        let rgb_size = width * height * 3;

        let yuyv_input: Vec<u8> = (0..yuyv_size).map(|i| (i % 256) as u8).collect();
        let mut rgb = vec![0u8; rgb_size];

        yuyv_to_rgb(&yuyv_input, &mut rgb, width, height);

        // Just verify it doesn't panic and produces output
        assert!(rgb.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_bgr_to_rgb() {
        let bgr = vec![0u8, 128u8, 255u8]; // B=0, G=128, R=255
        let mut rgb = vec![0u8; 3];

        bgr_to_rgb(&bgr, &mut rgb);

        assert_eq!(rgb[0], 255); // R
        assert_eq!(rgb[1], 128); // G
        assert_eq!(rgb[2], 0);   // B
    }
}
