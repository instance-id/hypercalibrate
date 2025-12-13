//! Color space conversion and correction for video capture
//!
//! This module handles color space conversions between different standards:
//! - BT.601 (SDTV, older content)
//! - BT.709 (HDTV, most HD content)
//! - BT.2020 (UHDTV, HDR content)
//!
//! It also handles:
//! - Quantization range (Limited 16-235 vs Full 0-255)
//! - Software color adjustments (brightness, contrast, saturation, hue)
//!
//! HDMI capture cards often have issues with:
//! 1. Wrong color matrix being applied (e.g., BT.601 instead of BT.709)
//! 2. Quantization range mismatch (limited vs full)
//! 3. HDR content (BT.2020/PQ) being captured as SDR

use serde::{Deserialize, Serialize};

/// Color space standard for YCbCr to RGB conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorSpace {
    /// BT.601 - Standard Definition (SDTV)
    /// Used by: DVD, older content, some webcams
    Bt601,
    /// BT.709 - High Definition (HDTV)
    /// Used by: Most HD content, Blu-ray, streaming services (SDR)
    #[default]
    Bt709,
    /// BT.2020 - Ultra High Definition (UHDTV)
    /// Used by: 4K HDR content, Netflix HDR, etc.
    /// Note: This only handles the color matrix, not HDR tone mapping
    Bt2020,
}

impl ColorSpace {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorSpace::Bt601 => "BT.601 (SD)",
            ColorSpace::Bt709 => "BT.709 (HD)",
            ColorSpace::Bt2020 => "BT.2020 (UHD)",
        }
    }

    pub fn all() -> &'static [ColorSpace] {
        &[ColorSpace::Bt601, ColorSpace::Bt709, ColorSpace::Bt2020]
    }
}

/// Quantization range for YCbCr values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum QuantizationRange {
    /// Limited range: Y 16-235, CbCr 16-240
    /// Used by: Most broadcast content, HDMI default
    #[default]
    Limited,
    /// Full range: Y 0-255, CbCr 0-255
    /// Used by: PC content, some gaming consoles, JPEG
    Full,
}

impl QuantizationRange {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuantizationRange::Limited => "Limited (16-235)",
            QuantizationRange::Full => "Full (0-255)",
        }
    }
}

/// Color correction settings that can be applied in software
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorCorrection {
    /// Color space for YCbCr to RGB conversion
    #[serde(default)]
    pub color_space: ColorSpace,

    /// Input quantization range (what the capture card sends)
    #[serde(default)]
    pub input_range: QuantizationRange,

    /// Brightness adjustment (-100 to +100, 0 = no change)
    #[serde(default)]
    pub brightness: i32,

    /// Contrast adjustment (0.0 to 2.0, 1.0 = no change)
    #[serde(default = "default_contrast")]
    pub contrast: f32,

    /// Saturation adjustment (0.0 to 2.0, 1.0 = no change)
    #[serde(default = "default_saturation")]
    pub saturation: f32,

    /// Hue rotation in degrees (-180 to +180, 0 = no change)
    #[serde(default)]
    pub hue: f32,

    /// Gamma adjustment (0.1 to 3.0, 1.0 = no change)
    /// Values < 1.0 brighten midtones, > 1.0 darken midtones
    #[serde(default = "default_gamma")]
    pub gamma: f32,

    /// Red channel gain for white balance (0.5 to 2.0, 1.0 = no change)
    #[serde(default = "default_gain")]
    pub red_gain: f32,

    /// Green channel gain for white balance (0.5 to 2.0, 1.0 = no change)
    #[serde(default = "default_gain")]
    pub green_gain: f32,

    /// Blue channel gain for white balance (0.5 to 2.0, 1.0 = no change)
    #[serde(default = "default_gain")]
    pub blue_gain: f32,

    /// Enable color correction processing
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_contrast() -> f32 { 1.0 }
fn default_saturation() -> f32 { 1.0 }
fn default_gamma() -> f32 { 1.0 }
fn default_gain() -> f32 { 1.0 }
fn default_enabled() -> bool { false }

impl Default for ColorCorrection {
    fn default() -> Self {
        Self {
            color_space: ColorSpace::default(),
            input_range: QuantizationRange::default(),
            brightness: 0,
            contrast: 1.0,
            saturation: 1.0,
            hue: 0.0,
            gamma: 1.0,
            red_gain: 1.0,
            green_gain: 1.0,
            blue_gain: 1.0,
            enabled: false,
        }
    }
}

impl ColorCorrection {
    /// Check if any correction is actually needed
    pub fn needs_processing(&self) -> bool {
        if !self.enabled {
            return false;
        }
        // Check if any setting differs from passthrough
        self.brightness != 0
            || (self.contrast - 1.0).abs() > 0.001
            || (self.saturation - 1.0).abs() > 0.001
            || self.hue.abs() > 0.1
            || (self.gamma - 1.0).abs() > 0.001
            || (self.red_gain - 1.0).abs() > 0.001
            || (self.green_gain - 1.0).abs() > 0.001
            || (self.blue_gain - 1.0).abs() > 0.001
    }

    /// Create a preset for standard HD content (most common)
    pub fn preset_hd_standard() -> Self {
        Self {
            color_space: ColorSpace::Bt709,
            input_range: QuantizationRange::Limited,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a preset for HDR content (Netflix, etc.)
    /// Note: This doesn't do proper HDR tone mapping, just color matrix
    pub fn preset_hdr_to_sdr() -> Self {
        Self {
            color_space: ColorSpace::Bt2020,
            input_range: QuantizationRange::Limited,
            // Boost saturation slightly as BT.2020 has wider gamut
            saturation: 1.1,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a preset for PC/Gaming content
    pub fn preset_pc_gaming() -> Self {
        Self {
            color_space: ColorSpace::Bt709,
            input_range: QuantizationRange::Full,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a preset for legacy SD content
    pub fn preset_sd_legacy() -> Self {
        Self {
            color_space: ColorSpace::Bt601,
            input_range: QuantizationRange::Limited,
            enabled: true,
            ..Default::default()
        }
    }
}

/// Color conversion coefficients for different standards
/// These are the coefficients for converting YCbCr to RGB
#[derive(Debug, Clone, Copy)]
pub struct ColorMatrix {
    /// Coefficient for V (Cr) contribution to R
    pub kr: f32,
    /// Coefficient for U (Cb) and V (Cr) contribution to G
    pub kg_u: f32,
    pub kg_v: f32,
    /// Coefficient for U (Cb) contribution to B
    pub kb: f32,
}

impl ColorMatrix {
    /// BT.601 coefficients (SDTV)
    pub const BT601: Self = Self {
        kr: 1.402,
        kg_u: 0.344136,
        kg_v: 0.714136,
        kb: 1.772,
    };

    /// BT.709 coefficients (HDTV)
    pub const BT709: Self = Self {
        kr: 1.5748,
        kg_u: 0.1873,
        kg_v: 0.4681,
        kb: 1.8556,
    };

    /// BT.2020 coefficients (UHDTV)
    pub const BT2020: Self = Self {
        kr: 1.4746,
        kg_u: 0.1646,
        kg_v: 0.5714,
        kb: 1.8814,
    };

    pub fn from_color_space(cs: ColorSpace) -> Self {
        match cs {
            ColorSpace::Bt601 => Self::BT601,
            ColorSpace::Bt709 => Self::BT709,
            ColorSpace::Bt2020 => Self::BT2020,
        }
    }

    /// Convert to fixed-point integer coefficients (scaled by 256)
    /// for fast integer-only conversion
    pub fn to_fixed_point(&self) -> FixedPointMatrix {
        FixedPointMatrix {
            kr: (self.kr * 256.0) as i32,
            kg_u: (self.kg_u * 256.0) as i32,
            kg_v: (self.kg_v * 256.0) as i32,
            kb: (self.kb * 256.0) as i32,
        }
    }
}

/// Fixed-point integer coefficients for fast conversion
#[derive(Debug, Clone, Copy)]
pub struct FixedPointMatrix {
    pub kr: i32,
    pub kg_u: i32,
    pub kg_v: i32,
    pub kb: i32,
}

/// Pre-computed lookup tables for fast color correction
pub struct ColorCorrectionLUT {
    /// Gamma correction LUT (256 entries)
    pub gamma_lut: [u8; 256],
    /// Combined brightness/contrast LUT (256 entries)
    pub bc_lut: [u8; 256],
    /// Whether LUTs are identity (no-op)
    pub is_identity: bool,
}

impl ColorCorrectionLUT {
    /// Create LUTs from color correction settings
    pub fn from_settings(settings: &ColorCorrection) -> Self {
        let mut gamma_lut = [0u8; 256];
        let mut bc_lut = [0u8; 256];

        let is_identity = !settings.needs_processing();

        for i in 0..256 {
            let v = i as f32 / 255.0;

            // Apply gamma
            let gamma_corrected = if (settings.gamma - 1.0).abs() > 0.001 {
                v.powf(1.0 / settings.gamma)
            } else {
                v
            };
            gamma_lut[i] = (gamma_corrected * 255.0).clamp(0.0, 255.0) as u8;

            // Apply brightness and contrast
            // contrast is applied around midpoint (0.5)
            // brightness is added after
            let bc_corrected = ((v - 0.5) * settings.contrast + 0.5)
                + (settings.brightness as f32 / 255.0);
            bc_lut[i] = (bc_corrected * 255.0).clamp(0.0, 255.0) as u8;
        }

        Self {
            gamma_lut,
            bc_lut,
            is_identity,
        }
    }

    /// Apply LUT to a single value
    #[inline]
    pub fn apply(&self, value: u8) -> u8 {
        if self.is_identity {
            value
        } else {
            self.bc_lut[self.gamma_lut[value as usize] as usize]
        }
    }
}

/// Convert YUYV to RGB with configurable color space and range
///
/// This is the main conversion function that handles:
/// - Different color matrices (BT.601, BT.709, BT.2020)
/// - Quantization range expansion (limited to full)
/// - Uses fast integer math for ARM performance
#[inline]
pub fn yuyv_to_rgb_corrected(
    yuyv: &[u8],
    rgb: &mut [u8],
    width: usize,
    height: usize,
    color_space: ColorSpace,
    input_range: QuantizationRange,
) {
    let matrix = ColorMatrix::from_color_space(color_space).to_fixed_point();
    let pixels = width * height;

    // For limited range, we need to expand Y from 16-235 to 0-255
    // and Cb/Cr from 16-240 to 0-255
    let (y_offset, y_scale, c_scale) = match input_range {
        QuantizationRange::Limited => {
            // Y: (Y - 16) * 255 / 219 ≈ (Y - 16) * 298 / 256
            // C: (C - 128) * 255 / 224 ≈ (C - 128) * 291 / 256
            (16i32, 298i32, 291i32)
        }
        QuantizationRange::Full => {
            // No scaling needed
            (0i32, 256i32, 256i32)
        }
    };

    // Process 2 pixels at a time (4 bytes YUYV -> 6 bytes RGB)
    for i in 0..(pixels / 2) {
        let yuyv_offset = i * 4;
        let rgb_offset = i * 6;

        if yuyv_offset + 3 >= yuyv.len() || rgb_offset + 5 >= rgb.len() {
            break;
        }

        // Read YUYV values
        let y0_raw = yuyv[yuyv_offset] as i32;
        let u_raw = yuyv[yuyv_offset + 1] as i32;
        let y1_raw = yuyv[yuyv_offset + 2] as i32;
        let v_raw = yuyv[yuyv_offset + 3] as i32;

        // Apply range expansion
        let y0 = ((y0_raw - y_offset) * y_scale) >> 8;
        let y1 = ((y1_raw - y_offset) * y_scale) >> 8;
        let u = ((u_raw - 128) * c_scale) >> 8;
        let v = ((v_raw - 128) * c_scale) >> 8;

        // Apply color matrix
        // R = Y + Kr * V
        // G = Y - Kg_u * U - Kg_v * V
        // B = Y + Kb * U
        let v_r = (matrix.kr * v) >> 8;
        let uv_g = (matrix.kg_u * u + matrix.kg_v * v) >> 8;
        let u_b = (matrix.kb * u) >> 8;

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

/// Apply limited range expansion to RGB buffer in-place
///
/// For MJPEG sources where the JPEG was encoded with limited range (16-235),
/// this expands the values to full range (0-255).
///
/// Formula: out = (in - 16) * 255 / 219
#[inline]
pub fn apply_range_expansion(rgb: &mut [u8], input_range: QuantizationRange) {
    if matches!(input_range, QuantizationRange::Full) {
        return; // No expansion needed
    }

    // Pre-compute LUT for range expansion
    // (v - 16) * 255 / 219, clamped to 0-255
    static RANGE_LUT: std::sync::OnceLock<[u8; 256]> = std::sync::OnceLock::new();
    let lut = RANGE_LUT.get_or_init(|| {
        let mut table = [0u8; 256];
        for i in 0..256 {
            let expanded = ((i as i32 - 16) * 255) / 219;
            table[i] = expanded.clamp(0, 255) as u8;
        }
        table
    });

    for pixel in rgb.iter_mut() {
        *pixel = lut[*pixel as usize];
    }
}

/// Apply saturation and hue adjustment to RGB buffer in-place
///
/// This converts RGB -> HSV, adjusts S and H, then converts back
/// For performance, we use a simplified approach that works in RGB space
#[inline]
pub fn apply_saturation_hue(rgb: &mut [u8], saturation: f32, hue_degrees: f32) {
    if (saturation - 1.0).abs() < 0.001 && hue_degrees.abs() < 0.1 {
        return; // No adjustment needed
    }

    let hue_rad = hue_degrees * std::f32::consts::PI / 180.0;
    let cos_h = hue_rad.cos();
    let sin_h = hue_rad.sin();

    // Saturation/Hue rotation matrix (simplified, operates on RGB)
    // This is an approximation but fast and good enough for our purposes
    let sat = saturation;

    for chunk in rgb.chunks_exact_mut(3) {
        let r = chunk[0] as f32 / 255.0;
        let g = chunk[1] as f32 / 255.0;
        let b = chunk[2] as f32 / 255.0;

        // Convert to YIQ-like space for hue rotation
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        let i = 0.596 * r - 0.274 * g - 0.322 * b;
        let q = 0.211 * r - 0.523 * g + 0.312 * b;

        // Apply saturation
        let i_sat = i * sat;
        let q_sat = q * sat;

        // Apply hue rotation
        let i_rot = i_sat * cos_h - q_sat * sin_h;
        let q_rot = i_sat * sin_h + q_sat * cos_h;

        // Convert back to RGB
        let r_out = y + 0.956 * i_rot + 0.621 * q_rot;
        let g_out = y - 0.272 * i_rot - 0.647 * q_rot;
        let b_out = y - 1.106 * i_rot + 1.703 * q_rot;

        chunk[0] = (r_out * 255.0).clamp(0.0, 255.0) as u8;
        chunk[1] = (g_out * 255.0).clamp(0.0, 255.0) as u8;
        chunk[2] = (b_out * 255.0).clamp(0.0, 255.0) as u8;
    }
}

/// Apply brightness, contrast, and gamma using pre-computed LUT
#[inline]
pub fn apply_lut(rgb: &mut [u8], lut: &ColorCorrectionLUT) {
    if lut.is_identity {
        return;
    }

    for pixel in rgb.iter_mut() {
        *pixel = lut.apply(*pixel);
    }
}

/// Apply RGB gain (white balance) adjustment
#[inline]
pub fn apply_rgb_gain(rgb: &mut [u8], red_gain: f32, green_gain: f32, blue_gain: f32) {
    // Skip if all gains are 1.0
    if (red_gain - 1.0).abs() < 0.001 && (green_gain - 1.0).abs() < 0.001 && (blue_gain - 1.0).abs() < 0.001 {
        return;
    }

    for chunk in rgb.chunks_exact_mut(3) {
        chunk[0] = ((chunk[0] as f32) * red_gain).clamp(0.0, 255.0) as u8;
        chunk[1] = ((chunk[1] as f32) * green_gain).clamp(0.0, 255.0) as u8;
        chunk[2] = ((chunk[2] as f32) * blue_gain).clamp(0.0, 255.0) as u8;
    }
}

/// Result of auto white balance calculation
#[derive(Debug, Clone, Copy)]
pub struct WhiteBalanceResult {
    pub red_gain: f32,
    pub green_gain: f32,
    pub blue_gain: f32,
    pub confidence: f32,
}

/// Calculate auto white balance gains using the Gray World algorithm
///
/// This assumes the average color of the scene should be neutral gray.
/// Uses sparse sampling (every Nth pixel) for performance on Pi 5.
///
/// Returns gains that would make the average color neutral.
/// Typical execution time: <1ms for 640x480 with step=8
#[inline]
pub fn calculate_auto_white_balance(rgb: &[u8], step: usize) -> WhiteBalanceResult {
    let step = step.max(1);
    let pixels = rgb.len() / 3;

    if pixels == 0 {
        return WhiteBalanceResult {
            red_gain: 1.0,
            green_gain: 1.0,
            blue_gain: 1.0,
            confidence: 0.0,
        };
    }

    // Accumulate channel sums using sparse sampling
    let mut r_sum: u64 = 0;
    let mut g_sum: u64 = 0;
    let mut b_sum: u64 = 0;
    let mut count: u64 = 0;

    for i in (0..pixels).step_by(step) {
        let offset = i * 3;
        if offset + 2 < rgb.len() {
            r_sum += rgb[offset] as u64;
            g_sum += rgb[offset + 1] as u64;
            b_sum += rgb[offset + 2] as u64;
            count += 1;
        }
    }

    if count == 0 {
        return WhiteBalanceResult {
            red_gain: 1.0,
            green_gain: 1.0,
            blue_gain: 1.0,
            confidence: 0.0,
        };
    }

    // Calculate averages
    let r_avg = r_sum as f32 / count as f32;
    let g_avg = g_sum as f32 / count as f32;
    let b_avg = b_sum as f32 / count as f32;

    // Use green as reference (most common in natural scenes)
    // Calculate gains to make R and B match G
    let target = g_avg.max(1.0); // Avoid division by zero

    let red_gain = if r_avg > 1.0 { target / r_avg } else { 1.0 };
    let green_gain = 1.0; // Green is reference
    let blue_gain = if b_avg > 1.0 { target / b_avg } else { 1.0 };

    // Clamp gains to reasonable range
    let red_gain = red_gain.clamp(0.5, 2.0);
    let blue_gain = blue_gain.clamp(0.5, 2.0);

    // Calculate confidence based on how much the image deviates from neutral
    // Higher deviation = lower confidence (might be intentionally colored scene)
    let max_deviation = ((r_avg - g_avg).abs().max((b_avg - g_avg).abs())) / g_avg.max(1.0);
    let confidence = (1.0 - max_deviation.min(1.0)).max(0.0);

    WhiteBalanceResult {
        red_gain,
        green_gain,
        blue_gain,
        confidence,
    }
}

/// Full color correction pipeline
///
/// Applies all corrections in the optimal order:
/// 1. RGB Gain (white balance)
/// 2. Saturation/Hue (in color space)
/// 3. Brightness/Contrast/Gamma (via LUT)
pub fn apply_color_correction(rgb: &mut [u8], settings: &ColorCorrection, lut: &ColorCorrectionLUT) {
    if !settings.enabled {
        return;
    }

    // Apply RGB gain first (white balance correction)
    apply_rgb_gain(rgb, settings.red_gain, settings.green_gain, settings.blue_gain);

    // Apply saturation and hue (color adjustments)
    apply_saturation_hue(rgb, settings.saturation, settings.hue);

    // Apply brightness, contrast, gamma via LUT
    apply_lut(rgb, lut);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_space_matrix() {
        let bt601 = ColorMatrix::BT601.to_fixed_point();
        let bt709 = ColorMatrix::BT709.to_fixed_point();

        // BT.709 should have different coefficients than BT.601
        assert_ne!(bt601.kr, bt709.kr);
        assert_ne!(bt601.kb, bt709.kb);
    }

    #[test]
    fn test_limited_range_expansion() {
        // Test that limited range black (Y=16) becomes full range black (0)
        let yuyv = vec![16u8, 128, 16, 128]; // Black in limited range
        let mut rgb = vec![0u8; 6];

        yuyv_to_rgb_corrected(&yuyv, &mut rgb, 2, 1, ColorSpace::Bt709, QuantizationRange::Limited);

        // Should be close to black
        assert!(rgb[0] < 10, "R should be near 0, got {}", rgb[0]);
        assert!(rgb[1] < 10, "G should be near 0, got {}", rgb[1]);
        assert!(rgb[2] < 10, "B should be near 0, got {}", rgb[2]);
    }

    #[test]
    fn test_full_range_passthrough() {
        // Test that full range values pass through correctly
        let yuyv = vec![128u8, 128, 128, 128]; // Mid-gray
        let mut rgb = vec![0u8; 6];

        yuyv_to_rgb_corrected(&yuyv, &mut rgb, 2, 1, ColorSpace::Bt709, QuantizationRange::Full);

        // Should be close to mid-gray (128)
        assert!((rgb[0] as i32 - 128).abs() < 5, "R should be near 128, got {}", rgb[0]);
        assert!((rgb[1] as i32 - 128).abs() < 5, "G should be near 128, got {}", rgb[1]);
        assert!((rgb[2] as i32 - 128).abs() < 5, "B should be near 128, got {}", rgb[2]);
    }

    #[test]
    fn test_lut_identity() {
        let settings = ColorCorrection::default();
        let lut = ColorCorrectionLUT::from_settings(&settings);

        // Default settings should be identity
        for i in 0..256 {
            assert_eq!(lut.gamma_lut[i], i as u8);
        }
    }

    #[test]
    fn test_preset_creation() {
        let hd = ColorCorrection::preset_hd_standard();
        assert_eq!(hd.color_space, ColorSpace::Bt709);
        assert_eq!(hd.input_range, QuantizationRange::Limited);
        assert!(hd.enabled);

        let hdr = ColorCorrection::preset_hdr_to_sdr();
        assert_eq!(hdr.color_space, ColorSpace::Bt2020);
    }
}
