//! Configuration management for HyperCalibrate

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::color::ColorCorrection;

/// A 2D point with normalized coordinates (0.0 to 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Convert to pixel coordinates
    pub fn to_pixels(&self, width: u32, height: u32) -> (f64, f64) {
        (self.x * width as f64, self.y * height as f64)
    }

    /// Create from pixel coordinates
    pub fn from_pixels(px: f64, py: f64, width: u32, height: u32) -> Self {
        Self {
            x: px / width as f64,
            y: py / height as f64,
        }
    }
}

/// An edge midpoint with its edge index for proper ordering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgePoint {
    /// Unique ID for this point
    pub id: usize,
    /// Which edge this point belongs to (0=top, 1=right, 2=bottom, 3=left)
    pub edge: usize,
    /// Position along the edge (0.0 = start corner, 1.0 = end corner)
    pub t: f64,
    /// The actual x,y position (normalized 0-1)
    pub x: f64,
    pub y: f64,
}

impl EdgePoint {
    pub fn new(id: usize, edge: usize, t: f64, x: f64, y: f64) -> Self {
        Self { id, edge, t, x, y }
    }
}

/// Calibration data for perspective transformation
/// Uses 4 corner points and dynamic edge midpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calibration {
    /// The four corners of the TV screen in the camera image
    /// Order: top-left, top-right, bottom-right, bottom-left
    pub corners: [Point; 4],

    /// Dynamic edge midpoints - can be added/removed
    /// Each point knows which edge it belongs to
    #[serde(default)]
    pub edge_points: Vec<EdgePoint>,

    /// Next ID to assign to new edge points
    #[serde(default = "default_next_id")]
    pub next_edge_point_id: usize,

    /// Whether calibration is active
    pub enabled: bool,
}

fn default_next_id() -> usize {
    100
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            // Default to a slightly inset frame
            corners: [
                Point::new(0.1, 0.1),   // Top-left
                Point::new(0.9, 0.1),   // Top-right
                Point::new(0.9, 0.9),   // Bottom-right
                Point::new(0.1, 0.9),   // Bottom-left
            ],
            // Start with no edge points - user can add them as needed
            edge_points: Vec::new(),
            next_edge_point_id: 100,
            enabled: true,
        }
    }
}

impl Calibration {
    /// Add a new edge point
    pub fn add_edge_point(&mut self, edge: usize, x: f64, y: f64) -> usize {
        let id = self.next_edge_point_id;
        self.next_edge_point_id += 1;

        // Calculate t based on position along edge
        let t = self.calculate_t_for_edge(edge, x, y);

        self.edge_points.push(EdgePoint::new(id, edge, t, x, y));

        // Sort points on this edge by t value
        self.edge_points.sort_by(|a, b| {
            if a.edge != b.edge {
                a.edge.cmp(&b.edge)
            } else {
                a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        id
    }

    /// Remove an edge point by ID
    pub fn remove_edge_point(&mut self, id: usize) -> bool {
        let len_before = self.edge_points.len();
        self.edge_points.retain(|p| p.id != id);
        self.edge_points.len() < len_before
    }

    /// Update an edge point position
    pub fn update_edge_point(&mut self, id: usize, x: f64, y: f64) -> bool {
        // First calculate t with corners borrowed immutably
        let edge_and_t = self.edge_points.iter()
            .find(|p| p.id == id)
            .map(|p| (p.edge, Self::calculate_t_for_edge_static(p.edge, x, y, &self.corners)));

        if let Some((edge, t)) = edge_and_t {
            if let Some(point) = self.edge_points.iter_mut().find(|p| p.id == id) {
                point.x = x.clamp(0.0, 1.0);
                point.y = y.clamp(0.0, 1.0);
                point.t = t;
                return true;
            }
        }
        false
    }

    /// Calculate t (position along edge) for a point
    fn calculate_t_for_edge(&self, edge: usize, x: f64, y: f64) -> f64 {
        Self::calculate_t_for_edge_static(edge, x, y, &self.corners)
    }

    fn calculate_t_for_edge_static(edge: usize, x: f64, y: f64, corners: &[Point; 4]) -> f64 {
        let (from_idx, to_idx) = match edge {
            0 => (0, 1), // Top: TL -> TR
            1 => (1, 2), // Right: TR -> BR
            2 => (2, 3), // Bottom: BR -> BL
            3 => (3, 0), // Left: BL -> TL
            _ => return 0.5,
        };

        let from = &corners[from_idx];
        let to = &corners[to_idx];

        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let len_sq = dx * dx + dy * dy;

        if len_sq < 0.0001 {
            return 0.5;
        }

        let t = ((x - from.x) * dx + (y - from.y) * dy) / len_sq;
        t.clamp(0.0, 1.0)
    }

    /// Get edge points for a specific edge, sorted by t
    pub fn get_edge_points(&self, edge: usize) -> Vec<&EdgePoint> {
        let mut points: Vec<_> = self.edge_points.iter().filter(|p| p.edge == edge).collect();
        points.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        points
    }

    /// Simple 4-corner perspective transform points
    /// Returns source corners in pixel coordinates
    pub fn get_source_corners(&self, width: u32, height: u32) -> [(f64, f64); 4] {
        [
            self.corners[0].to_pixels(width, height),
            self.corners[1].to_pixels(width, height),
            self.corners[2].to_pixels(width, height),
            self.corners[3].to_pixels(width, height),
        ]
    }

    /// Destination corners (full frame)
    pub fn get_dest_corners(width: u32, height: u32) -> [(f64, f64); 4] {
        [
            (0.0, 0.0),                           // Top-left
            (width as f64, 0.0),                  // Top-right
            (width as f64, height as f64),        // Bottom-right
            (0.0, height as f64),                 // Bottom-left
        ]
    }
}

/// Preferred capture format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureFormat {
    /// Prefer MJPEG (lower bandwidth, but color conversion done by hardware)
    #[default]
    Mjpeg,
    /// Prefer YUYV (higher bandwidth, but we control color conversion)
    Yuyv,
}

/// Video configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub input_device: String,
    pub output_device: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Preferred capture format (MJPEG or YUYV)
    #[serde(default)]
    pub format: CaptureFormat,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            input_device: "/dev/video0".to_string(),
            output_device: "/dev/video10".to_string(),
            width: 640,
            height: 480,
            fps: 30,
            format: CaptureFormat::default(),
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8091,
        }
    }
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub video: VideoConfig,

    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub calibration: Calibration,

    #[serde(default)]
    pub camera: CameraConfig,

    /// Color correction settings for HDMI capture
    #[serde(default)]
    pub color: ColorCorrection,
}

/// Camera hardware control settings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraConfig {
    /// Stored control values by normalized control name
    /// e.g., "brightness" -> 128, "contrast" -> 32
    #[serde(default)]
    pub controls: HashMap<String, i64>,
}

impl Config {
    /// Load configuration from a file, or create default if it doesn't exist
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config from {:?}", path))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {:?}", path))?;
            tracing::info!("Loaded configuration from {:?}", path);
            Ok(config)
        } else {
            let config = Config::default();
            config.save(path)?;
            tracing::info!("Created default configuration at {:?}", path);
            Ok(config)
        }
    }

    /// Save configuration to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize configuration")?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {:?}", parent))?;
        }

        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {:?}", path))?;

        tracing::info!("Saved configuration to {:?}", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_conversion() {
        let p = Point::new(0.5, 0.5);
        let (px, py) = p.to_pixels(640, 480);
        assert_eq!(px, 320.0);
        assert_eq!(py, 240.0);

        let p2 = Point::from_pixels(320.0, 240.0, 640, 480);
        assert_eq!(p2.x, 0.5);
        assert_eq!(p2.y, 0.5);
    }

    #[test]
    fn test_default_calibration() {
        let cal = Calibration::default();
        assert_eq!(cal.corners.len(), 4);
        assert_eq!(cal.edge_points.len(), 0); // No edge points by default
        assert!(cal.enabled);
    }
}
