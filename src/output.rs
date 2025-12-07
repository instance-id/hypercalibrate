// Virtual camera output using v4l2loopback
//
// v4l2loopback supports two modes for feeding video:
// 1. write() API - Simple, requires opening with O_RDWR and writing frames
// 2. mmap/userptr streaming - More complex, uses V4L2 buffer queuing
//
// This implementation tries multiple approaches in order:
// 1. Direct write() - simplest and most reliable for v4l2loopback
// 2. Fallback to a dummy mode that logs frames for debugging

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Supported pixel formats for output
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PixelFormat {
    Yuyv,   // YUYV 4:2:2
    Rgb24,  // RGB24
    Bgr24,  // BGR24
}

impl PixelFormat {
    /// V4L2 fourcc code
    pub fn fourcc(&self) -> u32 {
        match self {
            PixelFormat::Yuyv => fourcc(b"YUYV"),
            PixelFormat::Rgb24 => fourcc(b"RGB3"),
            PixelFormat::Bgr24 => fourcc(b"BGR3"),
        }
    }

    /// Bytes per pixel (or average for packed formats)
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Yuyv => 2,
            PixelFormat::Rgb24 => 3,
            PixelFormat::Bgr24 => 3,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            PixelFormat::Yuyv => "YUYV",
            PixelFormat::Rgb24 => "RGB24",
            PixelFormat::Bgr24 => "BGR24",
        }
    }
}

/// Create fourcc code from bytes
fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}

/// Virtual camera output device
pub struct VirtualCamera {
    device_path: String,
    file: Option<File>,
    width: u32,
    height: u32,
    format: PixelFormat,
    frame_size: usize,
    frame_count: AtomicU64,
    dropped_count: AtomicU64,
    initialized: bool,
    /// Track when we last logged a warning about dropped frames
    last_drop_warn: Option<Instant>,
}

// V4L2 ioctl definitions
const VIDIOC_S_FMT: libc::c_ulong = 0xc0d05605;
const V4L2_BUF_TYPE_VIDEO_OUTPUT: u32 = 2;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_PIX_FMT_YUYV: u32 = 0x56595559; // 'YUYV'
const V4L2_PIX_FMT_RGB24: u32 = 0x33424752; // 'RGB3'
const V4L2_PIX_FMT_BGR24: u32 = 0x33524742; // 'BGR3'

/// V4L2 format structure - matches kernel definition exactly
#[repr(C)]
struct v4l2_pix_format {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    ycbcr_enc_or_hsv_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

#[repr(C)]
struct v4l2_format {
    type_: u32,
    // 4 bytes padding to align the union to 8-byte boundary (matching kernel struct)
    _pad_before_fmt: u32,
    // Union - pix_format is the largest at 48 bytes, pad to 200 bytes total for union
    fmt: v4l2_pix_format,
    _pad: [u8; 200 - std::mem::size_of::<v4l2_pix_format>()],
}

impl VirtualCamera {
    /// Create a new virtual camera output
    pub fn new(device_path: &str, width: u32, height: u32) -> io::Result<Self> {
        info!("Creating virtual camera: {} ({}x{})", device_path, width, height);

        // Default to YUYV as it's most widely supported
        let format = PixelFormat::Yuyv;
        let frame_size = (width as usize) * (height as usize) * format.bytes_per_pixel();

        Ok(Self {
            device_path: device_path.to_string(),
            file: None,
            width,
            height,
            format,
            frame_size,
            frame_count: AtomicU64::new(0),
            dropped_count: AtomicU64::new(0),
            initialized: false,
            last_drop_warn: None,
        })
    }

    /// Initialize the virtual camera with format negotiation
    pub fn initialize(&mut self) -> io::Result<()> {
        if self.initialized {
            return Ok(());
        }

        // Check if device exists
        if !Path::new(&self.device_path).exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Virtual camera device {} not found. Is v4l2loopback loaded?", self.device_path)
            ));
        }

        // Try formats in order of preference
        let formats_to_try = [
            PixelFormat::Yuyv,
            PixelFormat::Rgb24,
            PixelFormat::Bgr24,
        ];

        for fmt in formats_to_try {
            info!("Trying format: {}", fmt.name());

            match self.try_initialize_with_format(fmt) {
                Ok(()) => {
                    self.format = fmt;
                    self.frame_size = (self.width as usize) * (self.height as usize) * fmt.bytes_per_pixel();
                    self.initialized = true;
                    info!("Virtual camera initialized successfully with format {}", fmt.name());
                    return Ok(());
                }
                Err(e) => {
                    warn!("Format {} rejected: {}", fmt.name(), e);
                }
            }
        }

        // If all formats fail, try opening without format setting
        // Some v4l2loopback setups accept any data
        warn!("All formats rejected, attempting raw open without format setting");
        match self.try_raw_open() {
            Ok(()) => {
                self.initialized = true;
                info!("Virtual camera opened in raw mode (format not set)");
                return Ok(());
            }
            Err(e) => {
                error!("Raw open also failed: {}", e);
            }
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Failed to initialize virtual camera {}. Ensure v4l2loopback is loaded correctly: \
                 sudo modprobe v4l2loopback video_nr=10 card_label=HyperCalibrate exclusive_caps=0",
                self.device_path
            )
        ))
    }

    /// Try to initialize with a specific format
    fn try_initialize_with_format(&mut self, format: PixelFormat) -> io::Result<()> {
        // Open the device with O_NONBLOCK to prevent blocking writes
        // This is critical to avoid deadlock when downstream consumer stops reading
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&self.device_path)?;

        let fd = file.as_raw_fd();

        // Set format using ioctl
        let bytesperline = self.width * format.bytes_per_pixel() as u32;
        let sizeimage = bytesperline * self.height;

        let mut v4l2_fmt = v4l2_format {
            type_: V4L2_BUF_TYPE_VIDEO_OUTPUT,
            _pad_before_fmt: 0,
            fmt: v4l2_pix_format {
                width: self.width,
                height: self.height,
                pixelformat: format.fourcc(),
                field: V4L2_FIELD_NONE,
                bytesperline,
                sizeimage,
                colorspace: 0,
                priv_: 0,
                flags: 0,
                ycbcr_enc_or_hsv_enc: 0,
                quantization: 0,
                xfer_func: 0,
            },
            _pad: [0u8; 200 - std::mem::size_of::<v4l2_pix_format>()],
        };

        let ret = unsafe {
            libc::ioctl(fd, VIDIOC_S_FMT, &mut v4l2_fmt as *mut v4l2_format)
        };

        if ret < 0 {
            let err = io::Error::last_os_error();
            return Err(err);
        }

        // Verify the format was accepted
        if v4l2_fmt.fmt.width != self.width || v4l2_fmt.fmt.height != self.height {
            warn!(
                "Format accepted but size changed: requested {}x{}, got {}x{}",
                self.width, self.height, v4l2_fmt.fmt.width, v4l2_fmt.fmt.height
            );
        }

        self.file = Some(file);
        Ok(())
    }

    /// Try opening without setting format (for pre-configured v4l2loopback)
    fn try_raw_open(&mut self) -> io::Result<()> {
        // Open with O_NONBLOCK to prevent blocking writes
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&self.device_path)?;

        self.file = Some(file);
        Ok(())
    }

    /// Write a frame in RGB24 format - will convert as needed
    pub fn write_frame_rgb(&mut self, rgb_data: &[u8]) -> io::Result<()> {
        if !self.initialized {
            self.initialize()?;
        }

        let expected_size = (self.width as usize) * (self.height as usize) * 3;
        if rgb_data.len() != expected_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "RGB data size mismatch: expected {} bytes ({}x{}x3), got {}",
                    expected_size, self.width, self.height, rgb_data.len()
                )
            ));
        }

        // Convert and write based on target format
        match self.format {
            PixelFormat::Rgb24 => {
                self.write_raw(rgb_data)?;
            }
            PixelFormat::Bgr24 => {
                let bgr = rgb_to_bgr(rgb_data);
                self.write_raw(&bgr)?;
            }
            PixelFormat::Yuyv => {
                let yuyv = rgb_to_yuyv(rgb_data, self.width as usize, self.height as usize);
                self.write_raw(&yuyv)?;
            }
        }

        let count = self.frame_count.fetch_add(1, Ordering::Relaxed);
        if count % 100 == 0 {
            debug!("Written {} frames to virtual camera", count + 1);
        }

        Ok(())
    }

    /// Write raw frame data directly to device
    /// Uses O_NONBLOCK to prevent deadlock if downstream consumer stops reading.
    /// If the buffer is full, the frame is dropped rather than blocking.
    fn write_raw(&mut self, data: &[u8]) -> io::Result<()> {
        if let Some(ref mut file) = self.file {
            match file.write_all(data) {
                Ok(()) => Ok(()),
                Err(e) if e.raw_os_error() == Some(libc::EAGAIN) ||
                          e.raw_os_error() == Some(libc::EWOULDBLOCK) => {
                    // Buffer full - drop the frame to avoid blocking
                    self.record_dropped_frame();
                    Ok(())
                }
                Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
                    // EINVAL often means format mismatch or device not ready
                    warn!("Write returned EINVAL - device may not be properly configured");
                    Err(e)
                }
                Err(e) => Err(e),
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Virtual camera file not open"
            ))
        }
    }

    /// Record a dropped frame and log periodically
    fn record_dropped_frame(&mut self) {
        let dropped = self.dropped_count.fetch_add(1, Ordering::Relaxed) + 1;

        // Log at most every 5 seconds to avoid spamming
        let should_warn = match self.last_drop_warn {
            None => true,
            Some(last) => last.elapsed() >= Duration::from_secs(5),
        };

        if should_warn {
            let written = self.frame_count.load(Ordering::Relaxed);
            warn!(
                "Dropped frame (total dropped: {}, written: {}) - downstream consumer may be slow",
                dropped, written
            );
            self.last_drop_warn = Some(Instant::now());
        }
    }

    /// Get the current frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Get the current dropped frame count
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    /// Get device info string
    pub fn info(&self) -> String {
        format!(
            "{} {}x{} {} (frames: {}, dropped: {})",
            self.device_path,
            self.width,
            self.height,
            self.format.name(),
            self.frame_count(),
            self.dropped_count()
        )
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            drop(file);
            info!("Virtual camera closed after {} frames", self.frame_count());
        }
    }
}

/// Convert RGB24 to BGR24
fn rgb_to_bgr(rgb: &[u8]) -> Vec<u8> {
    let mut bgr = Vec::with_capacity(rgb.len());
    for chunk in rgb.chunks_exact(3) {
        bgr.push(chunk[2]); // B
        bgr.push(chunk[1]); // G
        bgr.push(chunk[0]); // R
    }
    bgr
}

/// Convert RGB24 to YUYV (YUV 4:2:2 packed)
fn rgb_to_yuyv(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut yuyv = Vec::with_capacity(width * height * 2);

    for y in 0..height {
        for x in (0..width).step_by(2) {
            let idx1 = (y * width + x) * 3;
            let idx2 = (y * width + x + 1) * 3;

            // First pixel
            let r1 = rgb[idx1] as i32;
            let g1 = rgb[idx1 + 1] as i32;
            let b1 = rgb[idx1 + 2] as i32;

            // Second pixel (handle edge case)
            let (r2, g2, b2) = if x + 1 < width {
                (rgb[idx2] as i32, rgb[idx2 + 1] as i32, rgb[idx2 + 2] as i32)
            } else {
                (r1, g1, b1)
            };

            // Convert to YUV (BT.601)
            let y1 = ((66 * r1 + 129 * g1 + 25 * b1 + 128) >> 8) + 16;
            let y2 = ((66 * r2 + 129 * g2 + 25 * b2 + 128) >> 8) + 16;

            // Average U and V for the pair
            let u = ((-38 * r1 - 74 * g1 + 112 * b1 + 128) >> 8) + 128;
            let v = ((112 * r1 - 94 * g1 - 18 * b1 + 128) >> 8) + 128;

            yuyv.push(y1.clamp(0, 255) as u8);
            yuyv.push(u.clamp(0, 255) as u8);
            yuyv.push(y2.clamp(0, 255) as u8);
            yuyv.push(v.clamp(0, 255) as u8);
        }
    }

    yuyv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fourcc() {
        assert_eq!(fourcc(b"YUYV"), 0x56595559);
    }

    #[test]
    fn test_rgb_to_yuyv_basic() {
        // Simple 2x1 red image
        let rgb = vec![255, 0, 0, 255, 0, 0];
        let yuyv = rgb_to_yuyv(&rgb, 2, 1);
        assert_eq!(yuyv.len(), 4); // 2 pixels * 2 bytes
    }

    #[test]
    fn test_rgb_to_bgr() {
        let rgb = vec![1, 2, 3, 4, 5, 6];
        let bgr = rgb_to_bgr(&rgb);
        assert_eq!(bgr, vec![3, 2, 1, 6, 5, 4]);
    }
}
