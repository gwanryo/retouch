//! Validation rules (master plan §3.1, AC-E3c/E3d). Non-finite numbers and structural problems
//! are rejected; out-of-range numbers are clamped and reported with their path.

use super::schema::{Basic, ColorGrade, Detail, GradeZone, Hsl, Recipe, SplitToning};
use super::{Normalization, RecipeError};

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
}

pub(super) fn normalize(mut r: Recipe) -> Result<(Recipe, Vec<Normalization>), RecipeError> {
    if r.schema_version != crate::SCHEMA_VERSION {
        return Err(RecipeError::SchemaVersion(r.schema_version));
    }
    let mut c = Clamper { notes: Vec::new() };
    c.basic(&mut r.basic)?;
    c.hsl(&mut r.hsl)?;
    c.grade(&mut r.color_grade)?;
    c.split(&mut r.split_toning)?;
    c.detail(&mut r.detail)?;
    Ok((r, c.notes))
}
