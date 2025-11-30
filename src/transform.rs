//! Perspective transformation for screen calibration
//!
//! This module implements the perspective (homography) transformation
//! that maps the distorted camera view of the TV screen to a rectangular output.

use crate::config::Calibration;

/// Perspective transformation matrix (3x3 homography)
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
}

impl PerspectiveTransform {
    /// Create a new perspective transform from calibration data
    pub fn from_calibration(calibration: &Calibration, width: u32, height: u32) -> Self {
        let src = calibration.get_source_corners(width, height);
        let dst = Calibration::get_dest_corners(width, height);

        Self::compute(src, dst, width, height, width, height)
    }

    /// Compute the perspective transform from 4 source points to 4 destination points
    /// Uses the Direct Linear Transform (DLT) algorithm
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

        Self {
            matrix,
            inverse,
            src_width,
            src_height,
            dst_width,
            dst_height,
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
    /// Uses bilinear interpolation for smooth output
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

        for dst_y in 0..dst_h {
            for dst_x in 0..dst_w {
                // Map destination pixel to source coordinates
                let (src_x, src_y) = self.inverse_transform_point(dst_x as f64, dst_y as f64);

                // Bilinear interpolation
                let pixel = bilinear_sample(src, src_stride, src_w, src_h, channels, src_x, src_y);

                // Write to destination
                let dst_offset = dst_y * dst_stride + dst_x * channels;
                for c in 0..channels {
                    if dst_offset + c < dst.len() {
                        dst[dst_offset + c] = pixel[c];
                    }
                }
            }
        }
    }

    /// Apply the perspective transform to a YUYV image buffer
    /// YUYV is a common format for USB cameras (4 bytes = 2 pixels)
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

/// Bilinear interpolation sampling
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
