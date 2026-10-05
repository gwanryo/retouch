//! Recipe JSON schema v1, field for field (master plan §3.1). Every section exists from Plan 1a
//! so a v1 file never fails to parse; each default is the "no effect" value.

use serde::{Deserialize, Serialize};

/// Global tone/color controls. Names and ranges follow Lightroom `crs:` (AC-E3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Basic {
    /// Exposure2012, EV, -5..5.
    pub exposure: f32,
    /// Contrast2012, -100..100.
    pub contrast: f32,
    /// Highlights2012, -100..100.
    pub highlights: f32,
    /// Shadows2012, -100..100.
    pub shadows: f32,
    /// Whites2012, -100..100.
    pub whites: f32,
    /// Blacks2012, -100..100.
    pub blacks: f32,
    /// Relative white balance for SDR JPEG input, -100..100 (AC-E4c).
    pub temperature: f32,
    /// Relative tint, -100..100.
    pub tint: f32,
    /// -100..100.
    pub vibrance: f32,
    /// -100..100.
    pub saturation: f32,
    /// -100..100 (rendered from Plan 1b).
    pub texture: f32,
    /// -100..100 (rendered from Plan 1b).
    pub clarity: f32,
    /// -100..100 (rendered from Plan 1b).
    pub dehaze: f32,
}

/// HSL 8 bands in the order R, O, Y, G, A, B, P, M; each -100..100 (Plan 1b).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hsl {
    /// Hue shift per band.
    pub hue: [f32; 8],
    /// Saturation per band.
    pub saturation: [f32; 8],
    /// Luminance per band.
    pub luminance: [f32; 8],
}

/// Tone curves, points `[x, y]` in 0..255, x strictly increasing, 2..=16 points (Plan 1b).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToneCurve {
    /// Applied to all channels.
    pub master: Vec<[f32; 2]>,
    /// Red channel.
    pub red: Vec<[f32; 2]>,
    /// Green channel.
    pub green: Vec<[f32; 2]>,
    /// Blue channel.
    pub blue: Vec<[f32; 2]>,
}

/// The identity curve `[[0,0],[255,255]]`.
pub fn identity_curve() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [255.0, 255.0]]
}

impl Default for ToneCurve {
    fn default() -> Self {
        Self {
            master: identity_curve(),
            red: identity_curve(),
            green: identity_curve(),
            blue: identity_curve(),
        }
    }
}

/// One color-grading wheel: hue 0..360, sat 0..100, lum -100..100.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GradeZone {
    /// Hue in degrees.
    pub hue: f32,
    /// Saturation.
    pub sat: f32,
    /// Luminance.
    pub lum: f32,
}

/// Color grading (Plan 1b). `blending` 0..100 (default 50), `balance` -100..100.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorGrade {
    /// Shadows wheel.
    pub shadows: GradeZone,
    /// Midtones wheel.
    pub midtones: GradeZone,
    /// Highlights wheel.
    pub highlights: GradeZone,
    /// Global wheel.
    pub global: GradeZone,
    /// Zone blending.
    pub blending: f32,
    /// Shadow/highlight balance.
    pub balance: f32,
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self {
            shadows: GradeZone::default(),
            midtones: GradeZone::default(),
            highlights: GradeZone::default(),
            global: GradeZone::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

/// Split toning (Plan 1b): hue 0..360, sat 0..100, balance -100..100.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SplitToning {
    /// Shadow hue.
    pub shadow_hue: f32,
    /// Shadow saturation.
    pub shadow_sat: f32,
    /// Highlight hue.
    pub highlight_hue: f32,
    /// Highlight saturation.
    pub highlight_sat: f32,
    /// Balance.
    pub balance: f32,
}

/// Sharpening, grain, vignette (Plan 1b). Ranges in [`super::validate`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Detail {
    /// 0..150.
    pub sharpen_amount: f32,
    /// 0.5..3.0.
    pub sharpen_radius: f32,
    /// 0..100.
    pub sharpen_detail: f32,
    /// 0..100.
    pub sharpen_masking: f32,
    /// 0..100.
    pub grain_amount: f32,
    /// 0..100.
    pub grain_size: f32,
    /// 0..100.
    pub grain_frequency: f32,
    /// -100..100.
    pub vignette_amount: f32,
    /// 0..100.
    pub vignette_midpoint: f32,
    /// 0..100.
    pub vignette_feather: f32,
}

impl Default for Detail {
    fn default() -> Self {
        Self {
            sharpen_amount: 0.0,
            sharpen_radius: 1.0,
            sharpen_detail: 25.0,
            sharpen_masking: 0.0,
            grain_amount: 0.0,
            grain_size: 25.0,
            grain_frequency: 50.0,
            vignette_amount: 0.0,
            vignette_midpoint: 50.0,
            vignette_feather: 50.0,
        }
    }
}

/// Per-mask adjustments, 11 kinds (AC-E5b). Rendering: Plan 1b.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjust {
    /// EV, -5..5.
    pub exposure: f32,
    /// -100..100.
    pub contrast: f32,
    /// -100..100.
    pub highlights: f32,
    /// -100..100.
    pub shadows: f32,
    /// -100..100.
    pub whites: f32,
    /// -100..100.
    pub blacks: f32,
    /// -100..100.
    pub temperature: f32,
    /// -100..100.
    pub tint: f32,
    /// -100..100.
    pub saturation: f32,
    /// 0..150.
    pub sharpen_amount: f32,
    /// -100..100.
    pub clarity: f32,
}

/// Range mask channel, evaluated on the *original* linear pixel (AC-E5e).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RangeChannel {
    /// Luminance 0..1.
    Luminance,
    /// Hue 0..1 = 0..360°.
    Hue,
}

/// Mask geometry in original normalized coordinates [0,1] (AC-E5). Rendering: Plan 1b.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Mask {
    /// Linear gradient from (x0,y0) = full effect to (x1,y1) = none.
    Linear {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
        /// Feather 0..1.
        feather: f32,
        /// Invert.
        #[serde(default)]
        invert: bool,
        /// Adjustments inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
    /// Ellipse centered at (cx,cy) with radii (rx,ry).
    Radial {
        /// Center x.
        cx: f32,
        /// Center y.
        cy: f32,
        /// Radius x, (0,1].
        rx: f32,
        /// Radius y, (0,1].
        ry: f32,
        /// Feather 0..1.
        feather: f32,
        /// Invert.
        #[serde(default)]
        invert: bool,
        /// Adjustments inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
    /// Pixels whose channel value lies in [lo, hi], with smooth edges of width `smooth`.
    Range {
        /// Channel to threshold.
        channel: RangeChannel,
        /// Lower bound 0..1.
        lo: f32,
        /// Upper bound 0..1, >= lo.
        hi: f32,
        /// Edge width 0..1.
        smooth: f32,
        /// Invert.
        #[serde(default)]
        invert: bool,
        /// Adjustments inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
}

/// Axis-aligned crop in original normalized coordinates (AC-E7a).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Crop {
    /// The whole frame; a crop equal to this has no effect.
    pub const FULL: Crop = Crop {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
}

/// A full recipe (schema v1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Must equal [`crate::SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Global controls.
    #[serde(default)]
    pub basic: Basic,
    /// HSL bands.
    #[serde(default)]
    pub hsl: Hsl,
    /// Tone curves.
    #[serde(default)]
    pub tone_curve: ToneCurve,
    /// Color grading.
    #[serde(default)]
    pub color_grade: ColorGrade,
    /// Split toning.
    #[serde(default)]
    pub split_toning: SplitToning,
    /// Detail.
    #[serde(default)]
    pub detail: Detail,
    /// Up to 3 masks (AC-E5a).
    #[serde(default)]
    pub masks: Vec<Mask>,
    /// Optional crop; `null` and absent mean none.
    #[serde(default)]
    pub crop: Option<Crop>,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            schema_version: crate::SCHEMA_VERSION,
            basic: Basic::default(),
            hsl: Hsl::default(),
            tone_curve: ToneCurve::default(),
            color_grade: ColorGrade::default(),
            split_toning: SplitToning::default(),
            detail: Detail::default(),
            masks: Vec::new(),
            crop: None,
        }
    }
}
