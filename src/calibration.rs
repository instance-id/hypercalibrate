//! Calibration point management and UI state

use crate::config::{Calibration, Point};
use serde::{Deserialize, Serialize};

/// The type of calibration point
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PointType {
    Corner,
    Edge,
}

/// A labeled calibration point for the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationPoint {
    pub id: usize,
    pub point_type: PointType,
    pub label: String,
    pub x: f64,
    pub y: f64,
    /// For edge points, which edge (0-3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
}

impl CalibrationPoint {
    pub fn corner(id: usize, label: &str, x: f64, y: f64) -> Self {
        Self {
            id,
            point_type: PointType::Corner,
            label: label.to_string(),
            x,
            y,
            edge: None,
        }
    }

    pub fn edge(id: usize, label: &str, x: f64, y: f64, edge_idx: usize) -> Self {
        Self {
            id,
            point_type: PointType::Edge,
            label: label.to_string(),
            x,
            y,
            edge: Some(edge_idx),
        }
    }
}

/// Get all calibration points as labeled UI points
pub fn calibration_to_ui_points(cal: &Calibration) -> Vec<CalibrationPoint> {
    let mut points = vec![
        // Corners (IDs 0-3)
        CalibrationPoint::corner(0, "Top Left", cal.corners[0].x, cal.corners[0].y),
        CalibrationPoint::corner(1, "Top Right", cal.corners[1].x, cal.corners[1].y),
        CalibrationPoint::corner(2, "Bottom Right", cal.corners[2].x, cal.corners[2].y),
        CalibrationPoint::corner(3, "Bottom Left", cal.corners[3].x, cal.corners[3].y),
    ];

    // Add dynamic edge points
    for ep in &cal.edge_points {
        let edge_name = match ep.edge {
            0 => "Top",
            1 => "Right",
            2 => "Bottom",
            3 => "Left",
            _ => "Edge",
        };
        points.push(CalibrationPoint::edge(
            ep.id,
            &format!("{} Edge", edge_name),
            ep.x,
            ep.y,
            ep.edge,
        ));
    }

    points
}

/// Update a calibration point by ID (corners only - edge points use Calibration methods)
pub fn update_calibration_point(cal: &mut Calibration, id: usize, x: f64, y: f64) {
    // Clamp values to valid range
    let x = x.clamp(0.0, 1.0);
    let y = y.clamp(0.0, 1.0);

    match id {
        0..=3 => {
            cal.corners[id] = Point::new(x, y);
        }
        _ => {
            // Try to update as edge point
            if !cal.update_edge_point(id, x, y) {
                tracing::warn!("Invalid calibration point ID: {}", id);
            }
        }
    }
}

/// Reset calibration to default values
pub fn reset_calibration(cal: &mut Calibration) {
    *cal = Calibration::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_points_count() {
        let cal = Calibration::default();
        let points = calibration_to_ui_points(&cal);
        // Default has only 4 corners, no edge points
        assert_eq!(points.len(), 4);
    }

    #[test]
    fn test_update_corner() {
        let mut cal = Calibration::default();
        update_calibration_point(&mut cal, 0, 0.2, 0.3);
        assert_eq!(cal.corners[0].x, 0.2);
        assert_eq!(cal.corners[0].y, 0.3);
    }

    #[test]
    fn test_add_edge_point() {
        let mut cal = Calibration::default();
        let id = cal.add_edge_point(0, 0.5, 0.1);
        assert!(id >= 100);
        assert_eq!(cal.edge_points.len(), 1);

        let points = calibration_to_ui_points(&cal);
        assert_eq!(points.len(), 5); // 4 corners + 1 edge
    }

    #[test]
    fn test_remove_edge_point() {
        let mut cal = Calibration::default();
        let id = cal.add_edge_point(0, 0.5, 0.1);
        assert_eq!(cal.edge_points.len(), 1);

        cal.remove_edge_point(id);
        assert_eq!(cal.edge_points.len(), 0);
    }

    #[test]
    fn test_clamping() {
        let mut cal = Calibration::default();
        update_calibration_point(&mut cal, 0, 1.5, -0.5);
        assert_eq!(cal.corners[0].x, 1.0);
        assert_eq!(cal.corners[0].y, 0.0);
    }
}
