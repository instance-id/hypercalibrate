//! Video capture from V4L2 devices
//!
//! This module handles capturing video frames from USB cameras and other V4L2 devices.
//! It supports multiple pixel formats (YUYV, MJPEG, RGB) and automatically selects
//! the best available format.

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

/// Supported pixel formats in order of preference
/// YUYV is preferred as it's widely supported and doesn't need decompression
const PREFERRED_FORMATS: &[&[u8; 4]] = &[
    b"YUYV", // YUV 4:2:2 - very common for USB cameras, no decompression needed
    b"MJPG", // Motion JPEG - compressed, needs decoding but widely supported
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

    // Create the capture stream with memory mapping
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

    loop {
        // Capture a frame
        let (buf, _meta) = stream.next()
            .context("Failed to capture frame")?;

        // Get current transform from shared state
        let transform = state.get_transform();

        // Check if calibration is enabled
        let calibration_enabled = {
            let config = state.config.read();
            config.calibration.enabled
        };

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

        // Process the frame (apply calibration if enabled)
        let output_rgb = if calibration_enabled {
            transform.warp_rgb(working_rgb, &mut output_rgb_buffer);
            &output_rgb_buffer[..]
        } else {
            working_rgb
        };

        // Write to virtual camera (the output module handles format conversion)
        if let Err(e) = output.write_frame_rgb(output_rgb) {
            if frame_count % 100 == 0 {
                warn!("Failed to write to output: {}", e);
            }
        }

        // Update preview for web UI (at reduced rate to save bandwidth)
        if frame_count % 3 == 0 {
            state.update_preview(output_rgb, format.width, format.height);
            state.update_raw_preview(working_rgb, format.width, format.height);
        }

        frame_count += 1;

        // Log performance stats periodically
        if last_stats_time.elapsed() >= stats_interval {
            let elapsed = last_stats_time.elapsed().as_secs_f64();
            let fps_actual = frame_count as f64 / elapsed;
            info!("Performance: {:.1} fps ({} frames in {:.1}s)", fps_actual, frame_count, elapsed);
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
    let mut params = dev.params()
        .context("Failed to get parameters")?;

    params.interval = v4l::Fraction::new(1, fps);

    dev.set_params(&params)
        .context("Failed to set parameters")?;

    Ok(())
}

/// Decode MJPEG frame to RGB
fn decode_mjpeg<'a>(mjpeg_data: &[u8], rgb_buffer: &'a mut [u8], width: usize, height: usize) -> Result<&'a [u8], ()> {
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

/// Convert YUYV to RGB
#[inline]
pub fn yuyv_to_rgb(yuyv: &[u8], rgb: &mut [u8], width: usize, height: usize) {
    let pixels = width * height;

    for i in 0..(pixels / 2) {
        let yuyv_offset = i * 4;
        let rgb_offset = i * 6;

        if yuyv_offset + 3 >= yuyv.len() || rgb_offset + 5 >= rgb.len() {
            break;
        }

        let y0 = yuyv[yuyv_offset] as f32;
        let u = yuyv[yuyv_offset + 1] as f32 - 128.0;
        let y1 = yuyv[yuyv_offset + 2] as f32;
        let v = yuyv[yuyv_offset + 3] as f32 - 128.0;

        // First pixel
        rgb[rgb_offset] = clamp_u8(y0 + 1.402 * v);
        rgb[rgb_offset + 1] = clamp_u8(y0 - 0.344 * u - 0.714 * v);
        rgb[rgb_offset + 2] = clamp_u8(y0 + 1.772 * u);

        // Second pixel
        rgb[rgb_offset + 3] = clamp_u8(y1 + 1.402 * v);
        rgb[rgb_offset + 4] = clamp_u8(y1 - 0.344 * u - 0.714 * v);
        rgb[rgb_offset + 5] = clamp_u8(y1 + 1.772 * u);
    }
}

/// Convert BGR to RGB (swap R and B channels)
#[inline]
pub fn bgr_to_rgb(bgr: &[u8], rgb: &mut [u8]) {
    for i in 0..(bgr.len() / 3) {
        let offset = i * 3;
        if offset + 2 < bgr.len() && offset + 2 < rgb.len() {
            rgb[offset] = bgr[offset + 2];     // R <- B
            rgb[offset + 1] = bgr[offset + 1]; // G <- G
            rgb[offset + 2] = bgr[offset];     // B <- R
        }
    }
}

#[inline]
fn clamp_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
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
