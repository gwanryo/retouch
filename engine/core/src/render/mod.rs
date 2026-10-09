//! Render pipeline (AC-E5d). Fixed stage order, which the WebGL2 preview mirrors (A9):
//!
//! 1. encoded → linear (`srgb_decode`, unclamped)
//! 2. linear stage: exposure, white balance
//! 3. linear → encoded (`srgb_encode`, unclamped)
//! 4. encoded stage: contrast/highlights/shadows/whites/blacks, then vibrance/saturation
//! 5. [Plan 1b: HSL, curves, grading, range-mask eval on the ORIGINAL linear pixel, mask
//!    composite, detail]
//! 6. finiteness check. Clamping happens only in `buffer::quantize` (A3 point ①).
//!
//! Input and output are encoded-sRGB `ImageF32` of the SAME size, always (master plan A10):
//! the recipe's `crop` is never applied here. Cropping is a separate display operation
//! (`crop_rgba8`, Plan 1b) and the scoring path never uses it.
//!
//! [`render_rgba8`] precomputes stages 1-3 as a per-channel 256-entry LUT: exposure and white
//! balance are per-channel gains, so on 8-bit input each channel's stage-3 value depends on
//! that channel's byte alone, and evaluating the same functions in the same order 256 times
//! gives bit-identical results (Plan 1b-1). This holds only while stages 1-3 stay per-channel
//! functions of the input byte. Features that need per-pixel linear values get them from the
//! source (`srgb_decode` of the f32 input in [`render`], `srgb_decode(dequantize(byte))` in
//! [`render_rgba8`]); stages are never reordered to fit the LUT.

pub mod basic;

use crate::buffer::{dequantize, BufferError, ImageF32, Rgba8};
use crate::color::{srgb_decode, srgb_encode};
use crate::recipe::{identity_curve, Basic, Normalized, Recipe};
use thiserror::Error;

/// Per-render inputs that are not part of the recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderContext {
    /// Challenge seed (manifest `seed`, AC-E1d). Consumed by grain from Plan 1b; ignored now.
    pub seed: u32,
}

/// Render failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    /// The recipe uses a feature this build does not render yet. Never silently ignored.
    #[error("recipe uses `{feature}`, which is rendered from {plan}")]
    NotYetSupported {
        /// Feature path.
        feature: &'static str,
        /// Plan that adds it.
        plan: &'static str,
    },
    /// A sample became NaN or infinite (should be unreachable for validated recipes).
    #[error("render produced a non-finite sample")]
    NonFinite,
    /// Output buffer construction failed (unreachable for a valid `Rgba8` input).
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

/// First feature in `r` that Plan 1a cannot render, if any. "Active" means it changes pixels;
/// Plan 1b's renderer must treat every inactive case as a no-op too. `crop` is never listed:
/// render ignores it by contract (A10).
pub fn unsupported_feature(r: &Recipe) -> Option<&'static str> {
    let b = &r.basic;
    let zone_active = |z: &crate::recipe::GradeZone| z.sat != 0.0 || z.lum != 0.0;
    let g = &r.color_grade;
    let t = &r.tone_curve;
    let id = identity_curve();
    let checks: [(&'static str, bool); 11] = [
        ("basic.texture", b.texture != 0.0),
        ("basic.clarity", b.clarity != 0.0),
        ("basic.dehaze", b.dehaze != 0.0),
        (
            "hsl",
            r.hsl
                .hue
                .iter()
                .chain(&r.hsl.saturation)
                .chain(&r.hsl.luminance)
                .any(|&v| v != 0.0),
        ),
        (
            "tone_curve",
            [&t.master, &t.red, &t.green, &t.blue]
                .iter()
                .any(|c| **c != id),
        ),
        (
            "color_grade",
            [&g.shadows, &g.midtones, &g.highlights, &g.global]
                .into_iter()
                .any(zone_active),
        ),
        (
            "split_toning",
            r.split_toning.shadow_sat != 0.0 || r.split_toning.highlight_sat != 0.0,
        ),
        ("detail.sharpen", r.detail.sharpen_amount != 0.0),
        ("detail.grain", r.detail.grain_amount != 0.0),
        ("detail.vignette", r.detail.vignette_amount != 0.0),
        ("masks", !r.masks.is_empty()),
    ];
    checks
        .into_iter()
        .find(|&(_, active)| active)
        .map(|(f, _)| f)
}

/// Render a validated recipe onto `src` (encoded sRGB f32).
pub fn render(
    src: &ImageF32,
    recipe: &Normalized,
    _ctx: &RenderContext, // seed is consumed by grain from Plan 1b
) -> Result<ImageF32, RenderError> {
    let r = recipe.recipe();
    reject_unsupported(r)?;
    let b = &r.basic;
    let out = src.map_pixels(|px| global_encoded(linear_stage(px.map(srgb_decode), b), b));
    finite(out)
}

/// Both entry points refuse the same features, never silently ignoring them.
fn reject_unsupported(r: &Recipe) -> Result<(), RenderError> {
    match unsupported_feature(r) {
        Some(feature) => Err(RenderError::NotYetSupported {
            feature,
            plan: "Plan 1b",
        }),
        None => Ok(()),
    }
}

/// Stages 2-3 for one linear pixel: exposure, white balance, back to encoded.
fn linear_stage(lin: [f32; 3], b: &Basic) -> [f32; 3] {
    let lin = basic::apply_exposure_linear(lin, b.exposure);
    let lin = basic::apply_white_balance_linear(lin, b.temperature, b.tint);
    lin.map(srgb_encode)
}

/// Stage 4: the per-pixel global encoded-domain ops shared by both entry points.
fn global_encoded(enc: [f32; 3], b: &Basic) -> [f32; 3] {
    let enc = basic::apply_tone_encoded(enc, b);
    basic::apply_color_encoded(enc, b.vibrance, b.saturation)
}

/// Stage 6.
fn finite(out: ImageF32) -> Result<ImageF32, RenderError> {
    if out.samples().iter().all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err(RenderError::NonFinite)
    }
}

/// Stages 1-3 for every byte value, per channel. Calls exactly the functions [`render`] calls,
/// in the same order, so each entry is bit-identical to the per-pixel result.
fn linear_stage_lut(b: &Basic) -> [[f32; 256]; 3] {
    let mut lut = [[0.0; 256]; 3];
    for v in 0..=255u8 {
        let d = srgb_decode(dequantize(v));
        let enc = linear_stage([d, d, d], b);
        for (c, table) in lut.iter_mut().enumerate() {
            table[usize::from(v)] = enc[c];
        }
    }
    lut
}

/// LUT fast path for 8-bit input: the same stages as [`render`], before quantization.
pub(crate) fn render_rgba8_f32(
    src: &Rgba8,
    recipe: &Normalized,
    _ctx: RenderContext, // seed is consumed by grain from Plan 1b
) -> Result<ImageF32, RenderError> {
    let r = recipe.recipe();
    reject_unsupported(r)?;
    let b = &r.basic;
    let lut = linear_stage_lut(b);
    let samples = src
        .data()
        .chunks_exact(4)
        .flat_map(|p| {
            let enc = [
                lut[0][usize::from(p[0])],
                lut[1][usize::from(p[1])],
                lut[2][usize::from(p[2])],
            ];
            global_encoded(enc, b)
        })
        .collect();
    finite(ImageF32::from_samples(src.width(), src.height(), samples)?)
}

/// RGBA8 in, RGBA8 out: the full-resolution render with its single final quantization
/// (master plan A3 point ①). This is what the answer PNG stores.
pub fn render_rgba8(
    src: &Rgba8,
    recipe: &Normalized,
    ctx: &RenderContext,
) -> Result<Rgba8, RenderError> {
    Ok(render_rgba8_f32(src, recipe, *ctx)?.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Crop, Mask, RangeChannel};

    fn gradient(w: u32, h: u32) -> ImageF32 {
        let t = |i: u32, n: u32| {
            if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.5
            }
        };
        let mut img = ImageF32::new_filled(w, h, [0.0; 3]).unwrap();
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (t(x, w), t(y, h));
                img.set_pixel(x, y, [u, v, 0.5 * (u + v)]);
            }
        }
        img
    }

    fn norm(f: impl FnOnce(&mut Recipe)) -> Normalized {
        let mut r = Recipe::default();
        f(&mut r);
        r.normalize().unwrap()
    }

    const CTX: RenderContext = RenderContext { seed: 0 };

    #[test]
    fn identity_recipe_quantizes_back_to_identical_bytes() {
        let src = gradient(16, 8).to_rgba8();
        assert_eq!(render_rgba8(&src, &norm(|_| {}), &CTX).unwrap(), src);
    }

    #[test]
    fn renders_1x1_and_1xn_images() {
        for (w, h) in [(1, 1), (1, 7), (7, 1)] {
            let out = render(&gradient(w, h), &norm(|r| r.basic.exposure = 0.5), &CTX).unwrap();
            assert_eq!((out.width(), out.height()), (w, h));
        }
    }

    #[test]
    fn plus_one_ev_brightens_mean_luma() {
        let src = gradient(16, 8);
        let out = render(&src, &norm(|r| r.basic.exposure = 1.0), &CTX).unwrap();
        let mean = |img: &ImageF32| img.pixels().map(basic::luma).sum::<f32>();
        assert!(mean(&out) > mean(&src));
    }

    #[test]
    fn exposure_overflow_is_recovered_by_highlights_not_clipped() {
        // 0.80 and 0.85 both exceed 1.0 after +1 EV; without mid-pipeline clamping they stay
        // distinct and highlights -100 brings both back below 1 (master plan A3).
        let src = ImageF32::from_samples(2, 1, vec![0.8, 0.8, 0.8, 0.85, 0.85, 0.85]).unwrap();
        let r = norm(|r| {
            r.basic.exposure = 1.0;
            r.basic.highlights = -100.0;
        });
        let out = render_rgba8(&src.to_rgba8(), &r, &CTX).unwrap();
        let (a, b) = (out.data()[0], out.data()[4]);
        assert!(a < 255 && b < 255 && a != b, "a={a} b={b}");
    }

    #[test]
    fn seed_does_not_change_a_basic_render() {
        let src = gradient(32, 16).to_rgba8();
        let r = norm(|r| r.basic.contrast = 40.0);
        let a = render_rgba8(&src, &r, &RenderContext { seed: 1 }).unwrap();
        let b = render_rgba8(
            &src,
            &r,
            &RenderContext {
                seed: 4_000_000_000,
            },
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rendering_twice_is_bitwise_identical() {
        let src = gradient(32, 16);
        let r = norm(|r| {
            r.basic.exposure = 0.7;
            r.basic.contrast = 30.0;
            r.basic.temperature = -20.0;
            r.basic.saturation = 15.0;
        });
        assert_eq!(
            render(&src, &r, &CTX).unwrap(),
            render(&src, &r, &CTX).unwrap()
        );
    }

    #[test]
    fn extreme_basic_values_stay_finite_on_every_8bit_level() {
        let levels: Vec<f32> = (0..=255u8)
            .flat_map(|v| [f32::from(v) / 255.0; 3])
            .collect();
        let src = ImageF32::from_samples(256, 1, levels).unwrap();
        for bits in 0..1024u32 {
            let pick = |i: u32, lo: f32, hi: f32| if bits >> i & 1 == 1 { hi } else { lo };
            let r = norm(|r| {
                let b = &mut r.basic;
                b.exposure = pick(0, -5.0, 5.0);
                b.contrast = pick(1, -100.0, 100.0);
                b.highlights = pick(2, -100.0, 100.0);
                b.shadows = pick(3, -100.0, 100.0);
                b.whites = pick(4, -100.0, 100.0);
                b.blacks = pick(5, -100.0, 100.0);
                b.temperature = pick(6, -100.0, 100.0);
                b.tint = pick(7, -100.0, 100.0);
                b.vibrance = pick(8, -100.0, 100.0);
                b.saturation = pick(9, -100.0, 100.0);
            });
            assert!(render(&src, &r, &CTX).is_ok(), "combination {bits:#012b}");
        }
    }

    #[test]
    fn crop_never_changes_the_scoring_render() {
        let src = gradient(8, 8).to_rgba8();
        let plain = render_rgba8(&src, &norm(|r| r.basic.exposure = 0.4), &CTX).unwrap();
        let cropped = norm(|r| {
            r.basic.exposure = 0.4;
            r.crop = Some(Crop {
                x: 0.1,
                y: 0.2,
                w: 0.6,
                h: 0.7,
            });
        });
        assert_eq!(render_rgba8(&src, &cropped, &CTX).unwrap(), plain);
    }

    #[test]
    fn red_with_exposure_5_and_vibrance_100_stays_red() {
        let src = ImageF32::new_filled(1, 1, [0.8, 0.1, 0.1]).unwrap();
        let r = norm(|r| {
            r.basic.exposure = 5.0;
            r.basic.vibrance = 100.0;
        });
        let px = render(&src, &r, &CTX).unwrap().pixel(0, 0);
        assert!(px[0] > px[1] && px[0] > px[2], "hue inverted: {px:?}");
        let bytes = render_rgba8(&src.to_rgba8(), &r, &CTX).unwrap();
        assert_eq!(bytes.data()[0], 255, "{:?}", bytes.data());
    }

    fn rejected(f: impl FnOnce(&mut Recipe)) -> &'static str {
        match render(&gradient(4, 4), &norm(f), &CTX) {
            Err(RenderError::NotYetSupported { feature, .. }) => feature,
            other => panic!("expected NotYetSupported, got {other:?}"),
        }
    }

    #[test]
    fn rejects_each_plan_1b_feature_instead_of_ignoring_it() {
        assert_eq!(rejected(|r| r.basic.clarity = 10.0), "basic.clarity");
        assert_eq!(rejected(|r| r.hsl.saturation[2] = -20.0), "hsl");
        assert_eq!(
            rejected(|r| r.tone_curve.red[1] = [255.0, 200.0]),
            "tone_curve"
        );
        assert_eq!(
            rejected(|r| r.color_grade.midtones.sat = 10.0),
            "color_grade"
        );
        assert_eq!(
            rejected(|r| r.split_toning.highlight_sat = 5.0),
            "split_toning"
        );
        assert_eq!(rejected(|r| r.detail.grain_amount = 30.0), "detail.grain");
        assert_eq!(
            rejected(|r| r.detail.vignette_amount = -30.0),
            "detail.vignette"
        );
        assert_eq!(
            rejected(|r| r.masks.push(Mask::Range {
                channel: RangeChannel::Luminance,
                lo: 0.0,
                hi: 0.5,
                smooth: 0.1,
                invert: false,
                adjust: crate::recipe::Adjust::default(),
            })),
            "masks"
        );
    }

    #[test]
    fn grade_hue_or_blending_alone_has_no_effect_and_is_accepted() {
        let src = gradient(4, 4);
        let r = norm(|r| {
            r.color_grade.shadows.hue = 200.0;
            r.color_grade.blending = 80.0;
            r.detail.sharpen_radius = 2.0;
        });
        assert!(render(&src, &r, &CTX).is_ok());
    }

    /// 256×4: column v; rows (v,0,0), (0,v,0), (0,0,v), (v,255-v,v/2).
    fn all_levels() -> Rgba8 {
        let mut d = Vec::with_capacity(256 * 4 * 4);
        for row in 0..4u8 {
            for v in 0..=255u8 {
                let px = match row {
                    0 => [v, 0, 0],
                    1 => [0, v, 0],
                    2 => [0, 0, v],
                    _ => [v, 255 - v, v / 2],
                };
                d.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        Rgba8::new(256, 4, d).unwrap()
    }

    fn assert_same_bits(a: &ImageF32, b: &ImageF32) {
        assert_eq!((a.width(), a.height()), (b.width(), b.height()));
        for (i, (x, y)) in a.samples().iter().zip(b.samples()).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "sample {i}: {x} vs {y}");
        }
    }

    /// 2048 recipes: for each 10-bit mask, slider i takes the low or high value of a pair;
    /// first with the extremes, then with mid values. Slider order matches
    /// `extreme_basic_values_stay_finite_on_every_8bit_level`.
    fn basic_grid() -> Vec<Normalized> {
        let mut out = Vec::with_capacity(2048);
        for (ev, other) in [((-5.0, 5.0), (-100.0, 100.0)), ((-1.3, 0.7), (-37.0, 23.0))] {
            for bits in 0..1024u32 {
                let pick = |i: u32, (lo, hi): (f32, f32)| if bits >> i & 1 == 1 { hi } else { lo };
                out.push(norm(|r| {
                    let b = &mut r.basic;
                    b.exposure = pick(0, ev);
                    b.contrast = pick(1, other);
                    b.highlights = pick(2, other);
                    b.shadows = pick(3, other);
                    b.whites = pick(4, other);
                    b.blacks = pick(5, other);
                    b.temperature = pick(6, other);
                    b.tint = pick(7, other);
                    b.vibrance = pick(8, other);
                    b.saturation = pick(9, other);
                }));
            }
        }
        out
    }

    #[test]
    fn lut_render_matches_reference_on_grid() {
        let src = all_levels();
        for (k, r) in basic_grid().iter().enumerate() {
            let reference = render(&ImageF32::from_rgba8(&src), r, &CTX).unwrap();
            let fast = render_rgba8_f32(&src, r, CTX).unwrap();
            assert_same_bits(&fast, &reference);
            assert_eq!(fast.to_rgba8(), reference.to_rgba8(), "recipe {k}");
        }
    }

    #[test]
    fn lut_path_handles_degenerate_sizes() {
        let r = norm(|r| {
            r.basic.exposure = 0.5;
            r.basic.temperature = 20.0;
            r.basic.shadows = 40.0;
        });
        for (w, h) in [(1u32, 1u32), (3000, 1), (1, 3000), (7, 5)] {
            let data = (0..w * h * 4)
                .map(|i| ((i * 31 + (i / 4) * 17) % 256) as u8)
                .collect();
            let src = Rgba8::new(w, h, data).unwrap();
            assert_same_bits(
                &render_rgba8_f32(&src, &r, CTX).unwrap(),
                &render(&ImageF32::from_rgba8(&src), &r, &CTX).unwrap(),
            );
        }
    }
}
