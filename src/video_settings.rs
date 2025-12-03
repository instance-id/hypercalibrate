//! Video settings discovery and management
//!
//! This module provides functionality to query camera capabilities (supported
//! resolutions, framerates, formats) and manage video settings that require
//! a service restart to apply.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use v4l::video::Capture;
use v4l::{Device, FourCC};

/// Supported pixel format information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatInfo {
    /// FourCC code (e.g., "YUYV", "MJPG")
    pub fourcc: String,
    /// Human-readable description
    pub description: String,
}

/// Discrete frame size
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Display string like "1920x1080"
    pub fn display(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// Frame rate information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrameRate {
    /// Frames per second
    pub fps: f64,
    /// Numerator of the fraction (for V4L2)
    pub numerator: u32,
    /// Denominator of the fraction (for V4L2)
    pub denominator: u32,
}

impl FrameRate {
    pub fn from_fraction(numerator: u32, denominator: u32) -> Self {
        let fps = if numerator > 0 {
            denominator as f64 / numerator as f64
        } else {
            0.0
        };
        Self {
            fps,
            numerator,
            denominator,
        }
    }

    pub fn from_fps(fps: u32) -> Self {
        Self {
            fps: fps as f64,
            numerator: 1,
            denominator: fps,
        }
    }
}

/// Resolution with its supported framerates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionInfo {
    pub width: u32,
    pub height: u32,
    pub framerates: Vec<FrameRate>,
}

/// Complete camera capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCapabilities {
    /// Available pixel formats
    pub formats: Vec<FormatInfo>,
    /// Available resolutions with their framerates
    pub resolutions: Vec<ResolutionInfo>,
    /// Current settings
    pub current: CurrentVideoSettings,
    /// Whether capabilities could be fully queried
    pub complete: bool,
}

/// Current video settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentVideoSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: String,
}

/// Pending video settings change (requires restart)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PendingVideoSettings {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    /// Whether there are pending changes that need a restart
    pub needs_restart: bool,
}

impl PendingVideoSettings {
    pub fn has_changes(&self) -> bool {
        self.width.is_some() || self.height.is_some() || self.fps.is_some()
    }

    pub fn clear(&mut self) {
        self.width = None;
        self.height = None;
        self.fps = None;
        self.needs_restart = false;
    }
}

/// Query camera capabilities from a V4L2 device
pub fn query_camera_capabilities(device_path: &str, current_width: u32, current_height: u32, current_fps: u32) -> Result<CameraCapabilities> {
    let device = Device::with_path(device_path)
        .with_context(|| format!("Failed to open device: {}", device_path))?;

    let mut formats = Vec::new();
    let mut resolutions_map: HashMap<Resolution, Vec<FrameRate>> = HashMap::new();
    let mut complete = true;

    // Query available formats
    let format_descriptions = match device.enum_formats() {
        Ok(fmts) => fmts,
        Err(e) => {
            warn!("Failed to enumerate formats: {}", e);
            complete = false;
            Vec::new()
        }
    };

    // Get current format for reference
    let current_format = device.format().ok();
    let current_format_str = current_format
        .as_ref()
        .map(|f| String::from_utf8_lossy(&f.fourcc.repr).to_string())
        .unwrap_or_else(|| "YUYV".to_string());

    for fmt_desc in &format_descriptions {
        let fourcc_str = String::from_utf8_lossy(&fmt_desc.fourcc.repr).to_string();

        formats.push(FormatInfo {
            fourcc: fourcc_str.clone(),
            description: fmt_desc.description.clone(),
        });

        // Query frame sizes for this format
        match device.enum_framesizes(fmt_desc.fourcc) {
            Ok(frame_sizes) => {
                for frame_size in frame_sizes {
                    match &frame_size.size {
                        v4l::framesize::FrameSizeEnum::Discrete(discrete) => {
                            let res = Resolution::new(discrete.width, discrete.height);

                            // Query framerates for this resolution
                            let framerates = query_framerates(&device, fmt_desc.fourcc, discrete.width, discrete.height);

                            resolutions_map
                                .entry(res)
                                .or_insert_with(Vec::new)
                                .extend(framerates);
                        }
                        v4l::framesize::FrameSizeEnum::Stepwise(stepwise) => {
                            // For stepwise, add common resolutions within the range
                            let common_resolutions = [
                                (320, 240),
                                (640, 480),
                                (800, 600),
                                (1024, 768),
                                (1280, 720),
                                (1280, 960),
                                (1920, 1080),
                            ];

                            for (w, h) in common_resolutions {
                                if w >= stepwise.min_width
                                    && w <= stepwise.max_width
                                    && h >= stepwise.min_height
                                    && h <= stepwise.max_height
                                {
                                    let res = Resolution::new(w, h);
                                    let framerates = query_framerates(&device, fmt_desc.fourcc, w, h);
                                    resolutions_map
                                        .entry(res)
                                        .or_insert_with(Vec::new)
                                        .extend(framerates);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                debug!("Failed to enumerate frame sizes for {}: {}", fourcc_str, e);
                complete = false;
            }
        }
    }

    // Convert map to sorted vector
    let mut resolutions: Vec<ResolutionInfo> = resolutions_map
        .into_iter()
        .map(|(res, mut framerates)| {
            // Deduplicate and sort framerates
            framerates.sort_by(|a, b| b.fps.partial_cmp(&a.fps).unwrap_or(std::cmp::Ordering::Equal));
            framerates.dedup_by(|a, b| (a.fps - b.fps).abs() < 0.5);

            ResolutionInfo {
                width: res.width,
                height: res.height,
                framerates,
            }
        })
        .collect();

    // Sort resolutions by area (largest first for typical use case)
    resolutions.sort_by(|a, b| {
        let area_a = a.width * a.height;
        let area_b = b.width * b.height;
        area_b.cmp(&area_a)
    });

    // If no resolutions found, add some defaults
    if resolutions.is_empty() {
        warn!("No resolutions enumerated, adding defaults");
        complete = false;
        resolutions = vec![
            ResolutionInfo {
                width: 640,
                height: 480,
                framerates: vec![FrameRate::from_fps(30), FrameRate::from_fps(15)],
            },
            ResolutionInfo {
                width: 320,
                height: 240,
                framerates: vec![FrameRate::from_fps(30), FrameRate::from_fps(15)],
            },
        ];
    }

    info!(
        "Discovered {} formats, {} resolutions for {}",
        formats.len(),
        resolutions.len(),
        device_path
    );

    Ok(CameraCapabilities {
        formats,
        resolutions,
        current: CurrentVideoSettings {
            width: current_width,
            height: current_height,
            fps: current_fps,
            format: current_format_str,
        },
        complete,
    })
}

/// Query available framerates for a specific format and resolution
fn query_framerates(device: &Device, fourcc: FourCC, width: u32, height: u32) -> Vec<FrameRate> {
    match device.enum_frameintervals(fourcc, width, height) {
        Ok(intervals) => {
            intervals
                .into_iter()
                .filter_map(|interval| {
                    match interval.interval {
                        v4l::frameinterval::FrameIntervalEnum::Discrete(frac) => {
                            Some(FrameRate::from_fraction(frac.numerator, frac.denominator))
                        }
                        v4l::frameinterval::FrameIntervalEnum::Stepwise(stepwise) => {
                            // For stepwise, return common framerates
                            // Just return the max fps as a single option
                            if stepwise.min.numerator > 0 {
                                Some(FrameRate::from_fraction(
                                    stepwise.min.numerator,
                                    stepwise.min.denominator,
                                ))
                            } else {
                                None
                            }
                        }
                    }
                })
                .collect()
        }
        Err(e) => {
            debug!(
                "Failed to enumerate frame intervals for {}x{}: {}",
                width, height, e
            );
            // Return common defaults
            vec![FrameRate::from_fps(30), FrameRate::from_fps(15)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_display() {
        let res = Resolution::new(1920, 1080);
        assert_eq!(res.display(), "1920x1080");
    }

    #[test]
    fn test_framerate_from_fps() {
        let fr = FrameRate::from_fps(30);
        assert_eq!(fr.fps, 30.0);
        assert_eq!(fr.numerator, 1);
        assert_eq!(fr.denominator, 30);
    }

    #[test]
    fn test_framerate_from_fraction() {
        let fr = FrameRate::from_fraction(1, 30);
        assert!((fr.fps - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_pending_settings() {
        let mut pending = PendingVideoSettings::default();
        assert!(!pending.has_changes());

        pending.width = Some(1280);
        assert!(pending.has_changes());

        pending.clear();
        assert!(!pending.has_changes());
    }
}
