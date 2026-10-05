//! Validation rules (master plan §3.1, AC-E3c/E3d). Non-finite numbers and structural problems
//! are rejected; out-of-range numbers are clamped and reported with their path.

use super::schema::{
    Adjust, Basic, ColorGrade, Crop, Detail, GradeZone, Hsl, Mask, Recipe, SplitToning, ToneCurve,
};
use super::{Normalization, RecipeError};

pub(super) const MAX_MASKS: usize = 3;
pub(super) const MAX_CURVE_POINTS: usize = 16;
pub(super) const MIN_CROP_AREA: f32 = 0.25;

struct Clamper {
    notes: Vec<Normalization>,
}

impl Clamper {
    fn field(&mut self, path: String, v: &mut f32, lo: f32, hi: f32) -> Result<(), RecipeError> {
        if !v.is_finite() {
            return Err(RecipeError::NonFinite(path));
        }
        let c = v.clamp(lo, hi);
        if c != *v {
            self.notes.push(Normalization {
                path,
                from: *v,
                to: c,
                reason: "clamped",
            });
            *v = c;
        }
        Ok(())
    }

    fn zone(&mut self, p: &str, z: &mut GradeZone) -> Result<(), RecipeError> {
        self.field(format!("{p}.hue"), &mut z.hue, 0.0, 360.0)?;
        self.field(format!("{p}.sat"), &mut z.sat, 0.0, 100.0)?;
        self.field(format!("{p}.lum"), &mut z.lum, -100.0, 100.0)
    }

    fn adjust(&mut self, p: &str, a: &mut Adjust) -> Result<(), RecipeError> {
        self.field(format!("{p}.exposure"), &mut a.exposure, -5.0, 5.0)?;
        let pm100: [(&str, &mut f32); 9] = [
            ("contrast", &mut a.contrast),
            ("highlights", &mut a.highlights),
            ("shadows", &mut a.shadows),
            ("whites", &mut a.whites),
            ("blacks", &mut a.blacks),
            ("temperature", &mut a.temperature),
            ("tint", &mut a.tint),
            ("saturation", &mut a.saturation),
            ("clarity", &mut a.clarity),
        ];
        for (name, v) in pm100 {
            self.field(format!("{p}.{name}"), v, -100.0, 100.0)?;
        }
        self.field(
            format!("{p}.sharpen_amount"),
            &mut a.sharpen_amount,
            0.0,
            150.0,
        )
    }

    fn curve(&mut self, name: &'static str, pts: &mut [[f32; 2]]) -> Result<(), RecipeError> {
        if pts.len() < 2 || pts.len() > MAX_CURVE_POINTS {
            return Err(RecipeError::CurvePointCount {
                channel: name,
                count: pts.len(),
            });
        }
        for (i, p) in pts.iter_mut().enumerate() {
            self.field(format!("tone_curve.{name}[{i}][0]"), &mut p[0], 0.0, 255.0)?;
            self.field(format!("tone_curve.{name}[{i}][1]"), &mut p[1], 0.0, 255.0)?;
        }
        if let Some(i) = pts.windows(2).position(|w| w[1][0] <= w[0][0]) {
            return Err(RecipeError::CurveNotIncreasing {
                channel: name,
                index: i + 1,
            });
        }
        Ok(())
    }

    fn mask(&mut self, i: usize, m: &mut Mask) -> Result<(), RecipeError> {
        let p = format!("masks[{i}]");
        let degenerate = |reason: &'static str| RecipeError::DegenerateMask { index: i, reason };
        match m {
            Mask::Linear {
                x0,
                y0,
                x1,
                y1,
                feather,
                adjust,
                ..
            } => {
                for (n, v) in [
                    ("x0", &mut *x0),
                    ("y0", &mut *y0),
                    ("x1", &mut *x1),
                    ("y1", &mut *y1),
                ] {
                    self.field(format!("{p}.{n}"), v, 0.0, 1.0)?;
                }
                self.field(format!("{p}.feather"), feather, 0.0, 1.0)?;
                self.adjust(&format!("{p}.adjust"), adjust)?;
                if x0 == x1 && y0 == y1 {
                    return Err(degenerate("linear mask start equals end"));
                }
            }
            Mask::Radial {
                cx,
                cy,
                rx,
                ry,
                feather,
                adjust,
                ..
            } => {
                for (n, v) in [
                    ("cx", &mut *cx),
                    ("cy", &mut *cy),
                    ("rx", &mut *rx),
                    ("ry", &mut *ry),
                ] {
                    self.field(format!("{p}.{n}"), v, 0.0, 1.0)?;
                }
                self.field(format!("{p}.feather"), feather, 0.0, 1.0)?;
                self.adjust(&format!("{p}.adjust"), adjust)?;
                if *rx == 0.0 || *ry == 0.0 {
                    return Err(degenerate("radial mask radius is zero"));
                }
            }
            Mask::Range {
                lo,
                hi,
                smooth,
                adjust,
                ..
            } => {
                self.field(format!("{p}.lo"), lo, 0.0, 1.0)?;
                self.field(format!("{p}.hi"), hi, 0.0, 1.0)?;
                self.field(format!("{p}.smooth"), smooth, 0.0, 1.0)?;
                self.adjust(&format!("{p}.adjust"), adjust)?;
                if lo > hi {
                    return Err(RecipeError::RangeInverted { index: i });
                }
                if lo == hi && *smooth == 0.0 {
                    return Err(degenerate("range mask selects nothing"));
                }
            }
        }
        Ok(())
    }
}

impl Clamper {
    fn basic(&mut self, b: &mut Basic) -> Result<(), RecipeError> {
        self.field("basic.exposure".into(), &mut b.exposure, -5.0, 5.0)?;
        let pm100: [(&str, &mut f32); 12] = [
            ("contrast", &mut b.contrast),
            ("highlights", &mut b.highlights),
            ("shadows", &mut b.shadows),
            ("whites", &mut b.whites),
            ("blacks", &mut b.blacks),
            ("temperature", &mut b.temperature),
            ("tint", &mut b.tint),
            ("vibrance", &mut b.vibrance),
            ("saturation", &mut b.saturation),
            ("texture", &mut b.texture),
            ("clarity", &mut b.clarity),
            ("dehaze", &mut b.dehaze),
        ];
        for (name, v) in pm100 {
            self.field(format!("basic.{name}"), v, -100.0, 100.0)?;
        }
        Ok(())
    }

    fn hsl(&mut self, h: &mut Hsl) -> Result<(), RecipeError> {
        let bands = [
            ("hue", &mut h.hue),
            ("saturation", &mut h.saturation),
            ("luminance", &mut h.luminance),
        ];
        for (name, band) in bands {
            for (i, v) in band.iter_mut().enumerate() {
                self.field(format!("hsl.{name}[{i}]"), v, -100.0, 100.0)?;
            }
        }
        Ok(())
    }

    fn grade(&mut self, g: &mut ColorGrade) -> Result<(), RecipeError> {
        self.zone("color_grade.shadows", &mut g.shadows)?;
        self.zone("color_grade.midtones", &mut g.midtones)?;
        self.zone("color_grade.highlights", &mut g.highlights)?;
        self.zone("color_grade.global", &mut g.global)?;
        self.field("color_grade.blending".into(), &mut g.blending, 0.0, 100.0)?;
        self.field("color_grade.balance".into(), &mut g.balance, -100.0, 100.0)
    }

    fn split(&mut self, s: &mut SplitToning) -> Result<(), RecipeError> {
        let p = "split_toning";
        self.field(format!("{p}.shadow_hue"), &mut s.shadow_hue, 0.0, 360.0)?;
        self.field(format!("{p}.shadow_sat"), &mut s.shadow_sat, 0.0, 100.0)?;
        self.field(
            format!("{p}.highlight_hue"),
            &mut s.highlight_hue,
            0.0,
            360.0,
        )?;
        self.field(
            format!("{p}.highlight_sat"),
            &mut s.highlight_sat,
            0.0,
            100.0,
        )?;
        self.field(format!("{p}.balance"), &mut s.balance, -100.0, 100.0)
    }

    fn detail(&mut self, d: &mut Detail) -> Result<(), RecipeError> {
        let ranges: [(&str, &mut f32, f32, f32); 10] = [
            ("sharpen_amount", &mut d.sharpen_amount, 0.0, 150.0),
            ("sharpen_radius", &mut d.sharpen_radius, 0.5, 3.0),
            ("sharpen_detail", &mut d.sharpen_detail, 0.0, 100.0),
            ("sharpen_masking", &mut d.sharpen_masking, 0.0, 100.0),
            ("grain_amount", &mut d.grain_amount, 0.0, 100.0),
            ("grain_size", &mut d.grain_size, 0.0, 100.0),
            ("grain_frequency", &mut d.grain_frequency, 0.0, 100.0),
            ("vignette_amount", &mut d.vignette_amount, -100.0, 100.0),
            ("vignette_midpoint", &mut d.vignette_midpoint, 0.0, 100.0),
            ("vignette_feather", &mut d.vignette_feather, 0.0, 100.0),
        ];
        for (name, v, lo, hi) in ranges {
            self.field(format!("detail.{name}"), v, lo, hi)?;
        }
        Ok(())
    }

    fn curves(&mut self, t: &mut ToneCurve) -> Result<(), RecipeError> {
        self.curve("master", &mut t.master)?;
        self.curve("red", &mut t.red)?;
        self.curve("green", &mut t.green)?;
        self.curve("blue", &mut t.blue)
    }
}

fn validate_crop(k: Crop) -> Result<(), RecipeError> {
    for (name, v) in [
        ("crop.x", k.x),
        ("crop.y", k.y),
        ("crop.w", k.w),
        ("crop.h", k.h),
    ] {
        if !v.is_finite() {
            return Err(RecipeError::NonFinite(name.into()));
        }
    }
    let inside = k.x >= 0.0
        && k.y >= 0.0
        && k.w > 0.0
        && k.h > 0.0
        && k.x + k.w <= 1.0 + 1e-6
        && k.y + k.h <= 1.0 + 1e-6;
    if !inside {
        return Err(RecipeError::CropOutOfBounds {
            x: k.x,
            y: k.y,
            w: k.w,
            h: k.h,
        });
    }
    if k.w * k.h < MIN_CROP_AREA {
        return Err(RecipeError::CropTooSmall(k.w * k.h));
    }
    Ok(())
}

pub(super) fn normalize(mut r: Recipe) -> Result<(Recipe, Vec<Normalization>), RecipeError> {
    if r.schema_version != crate::SCHEMA_VERSION {
        return Err(RecipeError::SchemaVersion(r.schema_version));
    }
    if r.masks.len() > MAX_MASKS {
        return Err(RecipeError::TooManyMasks(r.masks.len()));
    }
    let mut c = Clamper { notes: Vec::new() };
    c.basic(&mut r.basic)?;
    c.hsl(&mut r.hsl)?;
    c.curves(&mut r.tone_curve)?;
    c.grade(&mut r.color_grade)?;
    c.split(&mut r.split_toning)?;
    c.detail(&mut r.detail)?;
    for (i, m) in r.masks.iter_mut().enumerate() {
        c.mask(i, m)?;
    }
    if let Some(k) = r.crop {
        validate_crop(k)?;
    }
    Ok((r, c.notes))
}
