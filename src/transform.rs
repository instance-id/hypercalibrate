//! Perspective transformation for screen calibration
//!
//! This module implements the perspective (homography) transformation
//! that maps the distorted camera view of the TV screen to a rectangular output.
//!
//! Supports both simple 4-corner homography and grid-based warping with
//! edge midpoints for more accurate correction of lens distortion.
//!
//! Performance optimizations:
//! - Pre-computed lookup table (LUT) for source coordinates
//! - f32 instead of f64 for faster computation on ARM
//! - Parallel processing using rayon
//! - Optional nearest-neighbor sampling for maximum speed

use crate::config::{Calibration, EdgePoint};
use rayon::prelude::*;

/// Perspective transformation matrix (3x3 homography)
///
/// Uses pre-computed lookup tables for fast warping and supports
/// parallel processing via rayon.
#[derive(Debug, Clone)]
pub struct PerspectiveTransform {
    /// The 3x3 transformation matrix stored in row-major order
    matrix: [f64; 9],
    /// Inverse matrix for reverse mapping (used for warping)
    inverse: [f64; 9],
    /// Source image dimensions
    src_width: u32,
    src_height: u32,
    /// Destination image dimensions
    dst_width: u32,
    dst_height: u32,
    /// Pre-computed source coordinates for each destination pixel (f32 for speed)
    /// Layout: [dst_y * dst_width + dst_x] = (src_x, src_y)
    lut: Vec<(f32, f32)>,
}

impl PerspectiveTransform {
    /// Create a new perspective transform from calibration data
    ///
    /// If edge points are present, uses grid-based warping for more accurate
    /// correction. Otherwise, falls back to simple 4-corner homography.
    pub fn from_calibration(calibration: &Calibration, width: u32, height: u32) -> Self {
        if calibration.edge_points.is_empty() {
            // Simple 4-corner homography
            let src = calibration.get_source_corners(width, height);
            let dst = Calibration::get_dest_corners(width, height);
            Self::compute(src, dst, width, height, width, height)
        } else {
            // Grid-based warping using edge points
            Self::from_calibration_with_grid(calibration, width, height)
        }
    }

    /// Create a grid-based transform that uses edge midpoints for more accurate warping.
    ///
    /// This divides the quadrilateral into a grid of smaller quads, allowing for
    /// local corrections that can handle lens distortion better than a single homography.
    fn from_calibration_with_grid(calibration: &Calibration, width: u32, height: u32) -> Self {
        let dst_w = width as usize;
        let dst_h = height as usize;

        // Build the source mesh from corners and edge points
        let mesh = SourceMesh::from_calibration(calibration, width, height);

        // Pre-compute lookup table for all destination pixels
        let mut lut = Vec::with_capacity(dst_w * dst_h);

        for dst_y in 0..dst_h {
            for dst_x in 0..dst_w {
                // Normalize destination coordinates to [0, 1]
                let u = dst_x as f64 / (width - 1).max(1) as f64;
                let v = dst_y as f64 / (height - 1).max(1) as f64;

                // Find the source position using the mesh
                let (src_x, src_y) = mesh.sample(u, v);
                lut.push((src_x as f32, src_y as f32));
            }
        }

        // For the matrix fields, compute a simple homography (used for point transforms)
        let src_corners = calibration.get_source_corners(width, height);
        let dst_corners = Calibration::get_dest_corners(width, height);
        let matrix = compute_homography(src_corners, dst_corners);
        let inverse = compute_homography(dst_corners, src_corners);

        Self {
            matrix,
            inverse,
            src_width: width,
            src_height: height,
            dst_width: width,
            dst_height: height,
            lut,
        }
    }

    /// Compute the perspective transform from 4 source points to 4 destination points
    /// Uses the Direct Linear Transform (DLT) algorithm
    /// Pre-computes a lookup table for fast warping
    pub fn compute(
        src: [(f64, f64); 4],
        dst: [(f64, f64); 4],
        src_width: u32,
        src_height: u32,
        dst_width: u32,
        dst_height: u32,
    ) -> Self {
        let matrix = compute_homography(src, dst);
        let inverse = compute_homography(dst, src);

        // Pre-compute lookup table for all destination pixels
        let dst_w = dst_width as usize;
        let dst_h = dst_height as usize;
        let mut lut = Vec::with_capacity(dst_w * dst_h);

        for dst_y in 0..dst_h {
            for dst_x in 0..dst_w {
                let (src_x, src_y) = apply_homography(&inverse, dst_x as f64, dst_y as f64);
                lut.push((src_x as f32, src_y as f32));
            }
        }

        Self {
            matrix,
            inverse,
            src_width,
            src_height,
            dst_width,
            dst_height,
            lut,
        }
    }

    /// Transform a point from source to destination coordinates
    #[inline]
    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        apply_homography(&self.matrix, x, y)
    }

    /// Transform a point from destination to source coordinates (inverse)
    #[inline]
    pub fn inverse_transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        apply_homography(&self.inverse, x, y)
    }

    /// Apply the perspective transform to an image buffer
    /// Uses pre-computed LUT and bilinear interpolation
    /// Parallelized across rows using rayon for multi-core performance
    pub fn warp_image(
        &self,
        src: &[u8],
        src_stride: usize,
        dst: &mut [u8],
        dst_stride: usize,
        channels: usize,
    ) {
        let dst_w = self.dst_width as usize;
        let dst_h = self.dst_height as usize;
        let src_w = self.src_width as usize;
        let src_h = self.src_height as usize;

        // Process rows in parallel using rayon
        dst.par_chunks_mut(dst_stride)
            .enumerate()
            .take(dst_h)
            .for_each(|(dst_y, row)| {
                self.warp_row(src, src_stride, src_w, src_h, channels, dst_w, dst_y, row);
            });
    }

    /// Warp a single row (used by both parallel and sequential paths)
    #[inline]
    fn warp_row(
        &self,
        src: &[u8],
        src_stride: usize,
        src_w: usize,
        src_h: usize,
        channels: usize,
        dst_w: usize,
        dst_y: usize,
        row: &mut [u8],
    ) {
        let lut_row_offset = dst_y * dst_w;

        for dst_x in 0..dst_w {
            let (src_x, src_y) = self.lut[lut_row_offset + dst_x];

            // Bilinear interpolation with f32 for speed
            let pixel = bilinear_sample_f32(
                src, src_stride, src_w, src_h, channels,
                src_x, src_y
            );

            // Write to destination
            let dst_offset = dst_x * channels;
            for c in 0..channels {
                if dst_offset + c < row.len() {
                    row[dst_offset + c] = pixel[c];
                }
            }
        }
    }

    /// Fast warp using nearest-neighbor sampling (no interpolation)
    /// Use this for maximum speed when quality can be sacrificed
    #[allow(dead_code)]
    pub fn warp_image_fast(
        &self,
        src: &[u8],
        src_stride: usize,
        dst: &mut [u8],
        dst_stride: usize,
        channels: usize,
    ) {
        let dst_w = self.dst_width as usize;
        let dst_h = self.dst_height as usize;
        let src_w = self.src_width as usize;
        let src_h = self.src_height as usize;

        // Parallel processing with nearest neighbor
        dst.par_chunks_mut(dst_stride)
            .enumerate()
            .take(dst_h)
            .for_each(|(dst_y, row)| {
                let lut_row_offset = dst_y * dst_w;

                for dst_x in 0..dst_w {
                    let (src_x, src_y) = self.lut[lut_row_offset + dst_x];

                    // Nearest neighbor - just round to nearest pixel
                    let sx = (src_x.round() as usize).min(src_w - 1);
                    let sy = (src_y.round() as usize).min(src_h - 1);
                    let src_offset = sy * src_stride + sx * channels;

                    // Write to destination
                    let dst_offset = dst_x * channels;
                    for c in 0..channels {
                        if dst_offset + c < row.len() && src_offset + c < src.len() {
                            row[dst_offset + c] = src[src_offset + c];
                        }
                    }
                }
            });
    }

    /// Apply the perspective transform to a YUYV image buffer
    /// YUYV is a common format for USB cameras (4 bytes = 2 pixels)
    #[allow(dead_code)]
    pub fn warp_yuyv(
        &self,
        src: &[u8],
        dst: &mut [u8],
    ) {
        let dst_w = self.dst_width as usize;
        let dst_h = self.dst_height as usize;
        let src_w = self.src_width as usize;
        let src_h = self.src_height as usize;
        let src_stride = src_w * 2; // YUYV: 2 bytes per pixel
        let dst_stride = dst_w * 2;

        for dst_y in 0..dst_h {
            for dst_x in 0..dst_w {
                // Map destination pixel to source coordinates
                let (src_x, src_y) = self.inverse_transform_point(dst_x as f64, dst_y as f64);

                // Sample YUYV with nearest neighbor (for speed)
                let sx = (src_x.round() as usize).min(src_w - 1);
                let sy = (src_y.round() as usize).min(src_h - 1);

                // Calculate source offset
                let src_offset = sy * src_stride + (sx / 2) * 4;
                let dst_offset = dst_y * dst_stride + (dst_x / 2) * 4;

                if src_offset + 3 < src.len() && dst_offset + 3 < dst.len() {
                    // For YUYV, we need to handle the paired pixels carefully
                    // Each 4 bytes contains: Y0 U Y1 V (two pixels sharing U and V)
                    let y = if sx % 2 == 0 {
                        src[src_offset]     // Y0
                    } else {
                        src[src_offset + 2] // Y1
                    };
                    let u = src[src_offset + 1];
                    let v = src[src_offset + 3];

                    // Write to destination
                    if dst_x % 2 == 0 {
                        dst[dst_offset] = y;     // Y0
                        dst[dst_offset + 1] = u; // U
                    } else {
                        dst[dst_offset + 2] = y; // Y1
                        dst[dst_offset + 3] = v; // V
                    }
                }
            }
        }
    }

    /// Apply the perspective transform to an RGB image buffer
    pub fn warp_rgb(
        &self,
        src: &[u8],
        dst: &mut [u8],
    ) {
        self.warp_image(
            src,
            self.src_width as usize * 3,
            dst,
            self.dst_width as usize * 3,
            3,
        );
    }

    /// Apply the perspective transform to an RGBA image buffer
    pub fn warp_rgba(
        &self,
        src: &[u8],
        dst: &mut [u8],
    ) {
        self.warp_image(
            src,
            self.src_width as usize * 4,
            dst,
            self.dst_width as usize * 4,
            4,
        );
    }
}

/// Compute a 3x3 homography matrix from 4 point correspondences
/// using the Direct Linear Transform (DLT) algorithm
fn compute_homography(src: [(f64, f64); 4], dst: [(f64, f64); 4]) -> [f64; 9] {
    // Build the 8x9 matrix A for the DLT algorithm
    // For each point correspondence (x,y) -> (x',y'), we have two equations:
    // -x*h1 - y*h2 - h3 + x'*x*h7 + x'*y*h8 + x'*h9 = 0
    // -x*h4 - y*h5 - h6 + y'*x*h7 + y'*y*h8 + y'*h9 = 0

    // We solve using a simplified method for exactly 4 points
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];

    for i in 0..4 {
        let (x, y) = src[i];
        let (xp, yp) = dst[i];

        let row1 = i * 2;
        let row2 = i * 2 + 1;

        a[row1][0] = x;
        a[row1][1] = y;
        a[row1][2] = 1.0;
        a[row1][3] = 0.0;
        a[row1][4] = 0.0;
        a[row1][5] = 0.0;
        a[row1][6] = -xp * x;
        a[row1][7] = -xp * y;
        b[row1] = xp;

        a[row2][0] = 0.0;
        a[row2][1] = 0.0;
        a[row2][2] = 0.0;
        a[row2][3] = x;
        a[row2][4] = y;
        a[row2][5] = 1.0;
        a[row2][6] = -yp * x;
        a[row2][7] = -yp * y;
        b[row2] = yp;
    }

    // Solve using Gaussian elimination
    let h = solve_linear_system(&mut a, &mut b);

    [h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], 1.0]
}

/// Solve an 8x8 linear system using Gaussian elimination with partial pivoting
fn solve_linear_system(a: &mut [[f64; 8]; 8], b: &mut [f64; 8]) -> [f64; 8] {
    let n = 8;

    // Forward elimination with partial pivoting
    for col in 0..n {
        // Find pivot
        let mut max_row = col;
        let mut max_val = a[col][col].abs();
        for row in (col + 1)..n {
            if a[row][col].abs() > max_val {
                max_val = a[row][col].abs();
                max_row = row;
            }
        }

        // Swap rows
        if max_row != col {
            a.swap(col, max_row);
            b.swap(col, max_row);
        }

        // Eliminate column
        let pivot = a[col][col];
        if pivot.abs() < 1e-10 {
            // Singular matrix, return identity-ish
            return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        }

        for row in (col + 1)..n {
            let factor = a[row][col] / pivot;
            for j in col..n {
                a[row][j] -= factor * a[col][j];
            }
            b[row] -= factor * b[col];
        }
    }

    // Back substitution
    let mut x = [0.0f64; 8];
    for i in (0..n).rev() {
        let mut sum = b[i];
        for j in (i + 1)..n {
            sum -= a[i][j] * x[j];
        }
        x[i] = sum / a[i][i];
    }

    x
}

/// Apply a homography matrix to a point
#[inline]
fn apply_homography(h: &[f64; 9], x: f64, y: f64) -> (f64, f64) {
    let w = h[6] * x + h[7] * y + h[8];
    if w.abs() < 1e-10 {
        return (x, y); // Avoid division by zero
    }
    let xp = (h[0] * x + h[1] * y + h[2]) / w;
    let yp = (h[3] * x + h[4] * y + h[5]) / w;
    (xp, yp)
}

/// Bilinear interpolation sampling using f32 for speed on ARM
/// This is the hot path - optimized for performance
#[inline]
fn bilinear_sample_f32(
    src: &[u8],
    stride: usize,
    width: usize,
    height: usize,
    channels: usize,
    x: f32,
    y: f32,
) -> [u8; 4] {
    // Clamp coordinates
    let x = x.max(0.0).min((width - 1) as f32);
    let y = y.max(0.0).min((height - 1) as f32);

    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    // Pre-compute weights
    let w00 = (1.0 - fx) * (1.0 - fy);
    let w10 = fx * (1.0 - fy);
    let w01 = (1.0 - fx) * fy;
    let w11 = fx * fy;

    // Pre-compute offsets
    let off00 = y0 * stride + x0 * channels;
    let off10 = y0 * stride + x1 * channels;
    let off01 = y1 * stride + x0 * channels;
    let off11 = y1 * stride + x1 * channels;

    let mut result = [0u8; 4];

    for c in 0..channels.min(4) {
        // Use get() to avoid bounds checks in the inner loop
        let p00 = src.get(off00 + c).copied().unwrap_or(0) as f32;
        let p10 = src.get(off10 + c).copied().unwrap_or(0) as f32;
        let p01 = src.get(off01 + c).copied().unwrap_or(0) as f32;
        let p11 = src.get(off11 + c).copied().unwrap_or(0) as f32;

        let value = p00 * w00 + p10 * w10 + p01 * w01 + p11 * w11;
        result[c] = value.round().clamp(0.0, 255.0) as u8;
    }

    result
}

/// Bilinear interpolation sampling (f64 version for precision when needed)
#[inline]
fn bilinear_sample(
    src: &[u8],
    stride: usize,
    width: usize,
    height: usize,
    channels: usize,
    x: f64,
    y: f64,
) -> [u8; 4] {
    // Clamp coordinates
    let x = x.max(0.0).min((width - 1) as f64);
    let y = y.max(0.0).min((height - 1) as f64);

    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);

    let fx = x - x0 as f64;
    let fy = y - y0 as f64;

    let mut result = [0u8; 4];

    for c in 0..channels.min(4) {
        let p00 = src.get(y0 * stride + x0 * channels + c).copied().unwrap_or(0) as f64;
        let p10 = src.get(y0 * stride + x1 * channels + c).copied().unwrap_or(0) as f64;
        let p01 = src.get(y1 * stride + x0 * channels + c).copied().unwrap_or(0) as f64;
        let p11 = src.get(y1 * stride + x1 * channels + c).copied().unwrap_or(0) as f64;

        let value = p00 * (1.0 - fx) * (1.0 - fy)
            + p10 * fx * (1.0 - fy)
            + p01 * (1.0 - fx) * fy
            + p11 * fx * fy;

        result[c] = value.round().clamp(0.0, 255.0) as u8;
    }

    result
}

/// Fast nearest-neighbor sampling (for performance-critical paths)
#[inline]
pub fn nearest_sample(
    src: &[u8],
    stride: usize,
    width: usize,
    height: usize,
    channels: usize,
    x: f64,
    y: f64,
) -> [u8; 4] {
    let x = (x.round() as usize).min(width - 1);
    let y = (y.round() as usize).min(height - 1);

    let offset = y * stride + x * channels;
    let mut result = [0u8; 4];

    for c in 0..channels.min(4) {
        result[c] = src.get(offset + c).copied().unwrap_or(0);
    }

    result
}

/// A mesh of source points for grid-based warping.
///
/// This structure represents the source quadrilateral divided into a grid,
/// where edge midpoints allow for local corrections. The mesh uses bilinear
/// interpolation between grid cells to map destination coordinates to source.
struct SourceMesh {
    /// Grid of source points in pixel coordinates.
    /// Organized as rows (top to bottom), each row contains points (left to right).
    /// The grid dimensions are (num_horizontal_points, num_vertical_points).
    grid: Vec<Vec<(f64, f64)>>,
    /// Number of columns in the grid (points along horizontal edges)
    cols: usize,
    /// Number of rows in the grid (points along vertical edges)
    rows: usize,
}

impl SourceMesh {
    /// Build a source mesh from calibration data.
    ///
    /// The mesh is constructed by:
    /// 1. Building the top and bottom edge point sequences (including corners and edge points)
    /// 2. Building the left and right edge point sequences
    /// 3. Interpolating interior points using bilinear blending between edges
    fn from_calibration(calibration: &Calibration, width: u32, height: u32) -> Self {
        // Get corners in pixel coordinates
        let corners = calibration.get_source_corners(width, height);
        let tl = corners[0];
        let tr = corners[1];
        let br = corners[2];
        let bl = corners[3];

        // Build edge sequences with corners and edge points
        // Edge 0: Top (TL -> TR)
        let top_edge = Self::build_edge_sequence(
            tl, tr,
            calibration.get_edge_points(0),
            width, height,
        );

        // Edge 1: Right (TR -> BR)
        let right_edge = Self::build_edge_sequence(
            tr, br,
            calibration.get_edge_points(1),
            width, height,
        );

        // Edge 2: Bottom (BR -> BL) - note: we'll reverse this for grid construction
        let bottom_edge_raw = Self::build_edge_sequence(
            br, bl,
            calibration.get_edge_points(2),
            width, height,
        );
        // Reverse so it goes BL -> BR (left to right)
        let bottom_edge: Vec<_> = bottom_edge_raw.into_iter().rev().collect();

        // Edge 3: Left (BL -> TL) - note: we'll reverse this for grid construction
        let left_edge_raw = Self::build_edge_sequence(
            bl, tl,
            calibration.get_edge_points(3),
            width, height,
        );
        // Reverse so it goes TL -> BL (top to bottom)
        let left_edge: Vec<_> = left_edge_raw.into_iter().rev().collect();

        // The grid dimensions are determined by the number of points on each edge
        // For a proper grid, top/bottom should have same count, left/right should have same count
        // We'll use the maximum and interpolate if needed
        let cols = top_edge.len().max(bottom_edge.len());
        let rows = left_edge.len().max(right_edge.len());

        // Resample edges to have uniform counts
        let top = Self::resample_edge(&top_edge, cols);
        let bottom = Self::resample_edge(&bottom_edge, cols);
        let left = Self::resample_edge(&left_edge, rows);
        let right = Self::resample_edge(&right_edge, rows);

        // Build the grid by interpolating between edges
        let mut grid = Vec::with_capacity(rows);

        for row in 0..rows {
            let v = if rows > 1 { row as f64 / (rows - 1) as f64 } else { 0.5 };
            let mut grid_row = Vec::with_capacity(cols);

            for col in 0..cols {
                let u = if cols > 1 { col as f64 / (cols - 1) as f64 } else { 0.5 };

                // Bilinear interpolation using all four edges
                // Top-bottom interpolation at this column
                let top_pt = top[col];
                let bottom_pt = bottom[col];
                let tb_x = top_pt.0 * (1.0 - v) + bottom_pt.0 * v;
                let tb_y = top_pt.1 * (1.0 - v) + bottom_pt.1 * v;

                // Left-right interpolation at this row
                let left_pt = left[row];
                let right_pt = right[row];
                let lr_x = left_pt.0 * (1.0 - u) + right_pt.0 * u;
                let lr_y = left_pt.1 * (1.0 - u) + right_pt.1 * u;

                // Combine using transfinite interpolation (Coons patch)
                // This properly blends the edge constraints
                let corner_blend =
                    top[0].0 * (1.0 - u) * (1.0 - v) +
                    top[cols - 1].0 * u * (1.0 - v) +
                    bottom[0].0 * (1.0 - u) * v +
                    bottom[cols - 1].0 * u * v;
                let corner_blend_y =
                    top[0].1 * (1.0 - u) * (1.0 - v) +
                    top[cols - 1].1 * u * (1.0 - v) +
                    bottom[0].1 * (1.0 - u) * v +
                    bottom[cols - 1].1 * u * v;

                let x = tb_x + lr_x - corner_blend;
                let y = tb_y + lr_y - corner_blend_y;

                grid_row.push((x, y));
            }

            grid.push(grid_row);
        }

        Self { grid, cols, rows }
    }

    /// Build a sequence of points along an edge, including the start/end corners
    /// and any edge points in between, sorted by t value.
    fn build_edge_sequence(
        start: (f64, f64),
        end: (f64, f64),
        edge_points: Vec<&EdgePoint>,
        width: u32,
        height: u32,
    ) -> Vec<(f64, f64)> {
        let mut seq = vec![(0.0, start)];

        for ep in edge_points {
            let px = ep.x * width as f64;
            let py = ep.y * height as f64;
            seq.push((ep.t, (px, py)));
        }

        seq.push((1.0, end));

        // Sort by t
        seq.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // Extract just the points
        seq.into_iter().map(|(_, pt)| pt).collect()
    }

    /// Resample an edge to have exactly `count` points using linear interpolation.
    fn resample_edge(edge: &[(f64, f64)], count: usize) -> Vec<(f64, f64)> {
        if edge.len() == count {
            return edge.to_vec();
        }

        if count == 0 {
            return vec![];
        }

        if count == 1 {
            // Return the midpoint
            let mid_idx = edge.len() / 2;
            return vec![edge[mid_idx]];
        }

        let mut result = Vec::with_capacity(count);

        for i in 0..count {
            let t = i as f64 / (count - 1) as f64;
            let pos = t * (edge.len() - 1) as f64;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f64;

            if idx >= edge.len() - 1 {
                result.push(edge[edge.len() - 1]);
            } else {
                let p0 = edge[idx];
                let p1 = edge[idx + 1];
                result.push((
                    p0.0 * (1.0 - frac) + p1.0 * frac,
                    p0.1 * (1.0 - frac) + p1.1 * frac,
                ));
            }
        }

        result
    }

    /// Sample the mesh at normalized coordinates (u, v) in [0, 1].
    /// Returns the corresponding source pixel coordinates.
    fn sample(&self, u: f64, v: f64) -> (f64, f64) {
        if self.cols < 2 || self.rows < 2 {
            // Degenerate mesh, return corner or center
            if !self.grid.is_empty() && !self.grid[0].is_empty() {
                return self.grid[0][0];
            }
            return (0.0, 0.0);
        }

        // Find the cell in the grid
        let col_f = u * (self.cols - 1) as f64;
        let row_f = v * (self.rows - 1) as f64;

        let col0 = (col_f.floor() as usize).min(self.cols - 2);
        let row0 = (row_f.floor() as usize).min(self.rows - 2);
        let col1 = col0 + 1;
        let row1 = row0 + 1;

        let cu = col_f - col0 as f64;
        let cv = row_f - row0 as f64;

        // Bilinear interpolation within the cell
        let p00 = self.grid[row0][col0];
        let p10 = self.grid[row0][col1];
        let p01 = self.grid[row1][col0];
        let p11 = self.grid[row1][col1];

        let x = p00.0 * (1.0 - cu) * (1.0 - cv)
            + p10.0 * cu * (1.0 - cv)
            + p01.0 * (1.0 - cu) * cv
            + p11.0 * cu * cv;

        let y = p00.1 * (1.0 - cu) * (1.0 - cv)
            + p10.1 * cu * (1.0 - cv)
            + p01.1 * (1.0 - cu) * cv
            + p11.1 * cu * cv;

        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Point;

    #[test]
    fn test_identity_transform() {
        let src = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ];
        let dst = src;

        let transform = PerspectiveTransform::compute(src, dst, 100, 100, 100, 100);

        // Test that identity-ish transform works
        let (x, y) = transform.transform_point(50.0, 50.0);
        assert!((x - 50.0).abs() < 0.01);
        assert!((y - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_simple_transform() {
        let src = [
            (10.0, 10.0),
            (90.0, 10.0),
            (90.0, 90.0),
            (10.0, 90.0),
        ];
        let dst = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ];

        let transform = PerspectiveTransform::compute(src, dst, 100, 100, 100, 100);

        // Test corner mapping
        let (x, y) = transform.transform_point(10.0, 10.0);
        assert!((x - 0.0).abs() < 1.0);
        assert!((y - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_grid_transform_with_edge_points() {
        use crate::config::EdgePoint;

        // Create a calibration with edge points
        let mut cal = Calibration::default();

        // Add an edge point on the top edge that's pushed inward
        // This simulates barrel distortion correction
        cal.edge_points.push(EdgePoint::new(100, 0, 0.5, 0.5, 0.15)); // Top edge midpoint, pushed down

        let transform = PerspectiveTransform::from_calibration(&cal, 100, 100);

        // The LUT should exist and have the right size
        assert_eq!(transform.lut.len(), 100 * 100);

        // Sample the center-top of the destination - it should map to a point
        // that's influenced by the edge point we added
        let (src_x, src_y) = transform.lut[5 * 100 + 50]; // y=5, x=50 (near top center)

        // The source y should be pulled toward the edge point (which is at y=0.15*100=15)
        // compared to a simple linear interpolation
        assert!(src_y > 5.0, "Edge point should influence the mapping, got src_y={}", src_y);
    }

    #[test]
    fn test_grid_transform_corners_preserved() {
        use crate::config::EdgePoint;

        // Create a calibration with edge points
        let mut cal = Calibration::default();
        cal.corners = [
            Point::new(0.1, 0.1),
            Point::new(0.9, 0.1),
            Point::new(0.9, 0.9),
            Point::new(0.1, 0.9),
        ];

        // Add edge points
        cal.edge_points.push(EdgePoint::new(100, 0, 0.5, 0.5, 0.12)); // Top edge
        cal.edge_points.push(EdgePoint::new(101, 2, 0.5, 0.5, 0.88)); // Bottom edge

        let transform = PerspectiveTransform::from_calibration(&cal, 100, 100);

        // Check that corners map correctly
        // Top-left corner of destination (0,0) should map to source (10, 10)
        let (src_x, src_y) = transform.lut[0];
        assert!((src_x - 10.0).abs() < 1.0, "Top-left x: expected ~10, got {}", src_x);
        assert!((src_y - 10.0).abs() < 1.0, "Top-left y: expected ~10, got {}", src_y);

        // Bottom-right corner of destination (99,99) should map to source (90, 90)
        let (src_x, src_y) = transform.lut[99 * 100 + 99];
        assert!((src_x - 90.0).abs() < 1.0, "Bottom-right x: expected ~90, got {}", src_x);
        assert!((src_y - 90.0).abs() < 1.0, "Bottom-right y: expected ~90, got {}", src_y);
    }

    #[test]
    fn test_source_mesh_basic() {
        // Test the SourceMesh directly
        let cal = Calibration::default();
        let mesh = SourceMesh::from_calibration(&cal, 100, 100);

        // With no edge points, should have 2x2 grid (just corners)
        assert_eq!(mesh.cols, 2);
        assert_eq!(mesh.rows, 2);

        // Corners should be at the calibration corner positions
        let (x, y) = mesh.sample(0.0, 0.0);
        assert!((x - 10.0).abs() < 0.1, "TL x: expected 10, got {}", x);
        assert!((y - 10.0).abs() < 0.1, "TL y: expected 10, got {}", y);

        let (x, y) = mesh.sample(1.0, 1.0);
        assert!((x - 90.0).abs() < 0.1, "BR x: expected 90, got {}", x);
        assert!((y - 90.0).abs() < 0.1, "BR y: expected 90, got {}", y);
    }

    #[test]
    fn test_source_mesh_with_edge_point() {
        use crate::config::EdgePoint;

        let mut cal = Calibration::default();
        // Add a point on the top edge at t=0.5
        cal.edge_points.push(EdgePoint::new(100, 0, 0.5, 0.5, 0.15));

        let mesh = SourceMesh::from_calibration(&cal, 100, 100);

        // Should now have 3 columns (TL, mid, TR) and 2 rows
        assert_eq!(mesh.cols, 3);
        assert_eq!(mesh.rows, 2);

        // The top-center point should be at the edge point position
        let top_mid = mesh.grid[0][1];
        assert!((top_mid.0 - 50.0).abs() < 1.0, "Top mid x: expected 50, got {}", top_mid.0);
        assert!((top_mid.1 - 15.0).abs() < 1.0, "Top mid y: expected 15, got {}", top_mid.1);
    }
}
