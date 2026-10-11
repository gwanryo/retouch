//! Render pipeline (AC-E5d). Fixed stage order, which the WebGL2 preview mirrors (A9):
//!
//! 1. encoded → linear (`srgb_decode`, unclamped)
//! 2. linear stage: exposure, white balance
//! 3. linear → encoded (`srgb_encode`, unclamped)
//! 4. encoded stage: contrast/highlights/shadows/whites/blacks, then vibrance/saturation
//! 5. global color, per pixel on encoded values: (a) tone curve, master then per channel
//!    (`curve`), (b) HSL 8 bands (`hsl`), (c) color grading (`grade`), (d) split toning
//!    (`grade`). Inactive features are skipped, so recipes that do not use them render
//!    bit-identically to Plan 1a.
//! 6. [Plan 1b later splits: range-mask eval on the ORIGINAL linear pixel, mask composite,
//!    detail]
//! 7. finiteness check. Clamping happens only in `buffer::quantize` (A3 point ①).
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
mod blur;
mod clarity;
mod curve;
mod grade;
mod hsl;
mod memo;
mod spatial;

use crate::buffer::{dequantize, quantize, BufferError, ImageF32, Rgba8};
use crate::color::{srgb_decode, srgb_encode};
use crate::recipe::{Basic, Normalized, Recipe};
use spatial::SpatialOps;
use thiserror::Error;

/// Per-render inputs that are not part of the recipe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderContext {
    /// Challenge seed (manifest `seed`, AC-E1d). Consumed by grain (stage 8c).
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

/// First feature in `r` that this build cannot render yet, if any. "Active" means it changes
/// pixels; Plan 1b's renderer must treat every inactive case as a no-op too. `crop` is never
/// listed: render ignores it by contract (A10).
pub fn unsupported_feature(r: &Recipe) -> Option<&'static str> {
    let checks: [(&'static str, bool); 4] = [
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
    ctx: &RenderContext,
) -> Result<ImageF32, RenderError> {
    let r = recipe.recipe();
    reject_unsupported(r)?;
    let b = &r.basic;
    let ops = GlobalOps::new(r);
    let mut out = src.map_pixels(|px| global_encoded(linear_stage(px.map(srgb_decode), b), &ops));
    if let Some(sp) = SpatialOps::new(r) {
        let (w, h) = (out.width(), out.height());
        let (wu, hu) = (w as usize, h as usize);
        let mut s = out.into_samples();
        sp.apply_global(&mut s, wu, hu);
        sp.apply_detail(&mut s, wu, hu, ctx.seed);
        out = ImageF32::from_samples(w, h, s)?;
    }
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

/// Stages 4-5, prepared once per render. Inactive stage-5 features are `None` and never run, so
/// a recipe that does not use them renders bit-identically to Plan 1a.
struct GlobalOps<'a> {
    basic: &'a Basic,
    /// Stage 4a, `dehaze / 100`; `None` when dehaze is 0.
    dehaze: Option<f32>,
    curves: Option<curve::CurveSet>,
    hsl: Option<hsl::HslPlan>,
    grade: Option<grade::GradePlan>,
    split: Option<grade::SplitPlan>,
}

impl<'a> GlobalOps<'a> {
    fn new(r: &'a Recipe) -> Self {
        Self {
            basic: &r.basic,
            dehaze: (r.basic.dehaze != 0.0).then(|| r.basic.dehaze / 100.0),
            curves: curve::CurveSet::new(&r.tone_curve),
            hsl: hsl::HslPlan::new(&r.hsl),
            grade: grade::GradePlan::new(&r.color_grade),
            split: grade::SplitPlan::new(&r.split_toning),
        }
    }

    /// Neither dehaze (4a) nor any stage-5 feature is active, so the basic tone and color of
    /// stage 4 alone are the whole of stages 4-5.
    fn only_basic(&self) -> bool {
        self.dehaze.is_none()
            && self.curves.is_none()
            && self.hsl.is_none()
            && self.grade.is_none()
            && self.split.is_none()
    }
}

/// Stage 4 alone: the same calls, in the same order, as the stage-4 lines of [`global_encoded`],
/// so it is bit-identical to it whenever [`GlobalOps::only_basic`] holds. A separate body
/// (not called from `global_encoded`) gives the stage-5-free recipes their own monomorphic loop;
/// sharing one generic loop with the stage-5 checks cost +10% render time (Task 6 profile F).
fn basic_encoded(enc: [f32; 3], b: &Basic) -> [f32; 3] {
    let enc = basic::apply_tone_encoded(enc, b);
    basic::apply_color_encoded(enc, b.vibrance, b.saturation)
}

/// Stages 4-5 for one encoded pixel, shared by both entry points: basic tone and color (4),
/// then tone curve (5a), HSL (5b), color grading (5c) and split toning (5d).
fn global_encoded(enc: [f32; 3], ops: &GlobalOps) -> [f32; 3] {
    let b = ops.basic;
    let enc = match ops.dehaze {
        Some(d) => basic::apply_dehaze_encoded(enc, d),
        None => enc,
    };
    let enc = basic::apply_tone_encoded(enc, b);
    let mut px = basic::apply_color_encoded(enc, b.vibrance, b.saturation);
    if let Some(c) = &ops.curves {
        px = c.apply(px);
    }
    if let Some(h) = &ops.hsl {
        px = h.apply(px);
    }
    if let Some(g) = &ops.grade {
        px = g.apply(px);
    }
    if let Some(s) = &ops.split {
        px = s.apply(px);
    }
    px
}

/// Stage 7.
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

/// LUT path for 8-bit input: the same stages as [`render`], before quantization. Test oracle
/// only since Task 6a: [`render_rgba8`] runs the same per-pixel math through [`shade`].
#[cfg(test)]
pub(crate) fn render_rgba8_f32(
    src: &Rgba8,
    recipe: &Normalized,
    ctx: RenderContext,
) -> Result<ImageF32, RenderError> {
    let r = recipe.recipe();
    reject_unsupported(r)?;
    let b = &r.basic;
    let lut = linear_stage_lut(b);
    let ops = GlobalOps::new(r);
    let mut samples: Vec<f32> = src
        .data()
        .chunks_exact(4)
        .flat_map(|p| {
            let enc = [
                lut[0][usize::from(p[0])],
                lut[1][usize::from(p[1])],
                lut[2][usize::from(p[2])],
            ];
            global_encoded(enc, &ops)
        })
        .collect();
    if let Some(sp) = SpatialOps::new(r) {
        let (w, h) = (src.width() as usize, src.height() as usize);
        sp.apply_global(&mut samples, w, h);
        sp.apply_detail(&mut samples, w, h, ctx.seed);
    }
    finite(ImageF32::from_samples(src.width(), src.height(), samples)?)
}

/// RGBA8 in, RGBA8 out: the full-resolution render with its single final quantization
/// (master plan A3 point ①). This is what the answer PNG stores.
///
/// Bit-identical to `render(&ImageF32::from_rgba8(src), ..)?.to_rgba8()`, but faster: stages 1-3
/// come from the LUT, recipes without stage-5 features run a stage-4-only loop, and pixels are
/// memoized by their input bytes (see `render/memo.rs` for why that is exact and when it stops being
/// valid). Output alpha is 255, as `ImageF32::to_rgba8` writes it.
pub fn render_rgba8(
    src: &Rgba8,
    recipe: &Normalized,
    ctx: &RenderContext,
) -> Result<Rgba8, RenderError> {
    render_rgba8_with(src, recipe, *ctx, Strategy::Memo).map(|(img, _)| img)
}

/// How [`shade`] visits pixels. Both give the same bytes and the same error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategy {
    /// Byte memo for the first [`memo::PROBE_PIXELS`], then kept or dropped by
    /// [`memo::falls_back`].
    Memo,
    /// Every pixel computed. Production reaches the same loop only through the memo's fallback;
    /// tests use this variant as the uncached reference.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "uncached reference strategy for tests")
    )]
    Direct,
}

/// What the memo did, for tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MemoStats {
    /// Cache hits among the first [`memo::PROBE_PIXELS`] pixels.
    probe_hits: usize,
    /// The rest of the image ran the direct loop.
    fell_back: bool,
}

fn render_rgba8_with(
    src: &Rgba8,
    recipe: &Normalized,
    ctx: RenderContext,
    strategy: Strategy,
) -> Result<(Rgba8, MemoStats), RenderError> {
    let r = recipe.recipe();
    reject_unsupported(r)?;
    if let Some(sp) = SpatialOps::new(r) {
        let (samples, stats) = spatial_samples(src, r, &sp, ctx.seed, strategy);
        let mut data = Vec::with_capacity(src.data().len());
        for px in samples.chunks_exact(3) {
            let [red, green, blue] = finish([px[0], px[1], px[2]])?;
            data.extend_from_slice(&[red, green, blue, 255]);
        }
        return Ok((Rgba8::new(src.width(), src.height(), data)?, stats));
    }
    let lut = linear_stage_lut(&r.basic);
    let ops = GlobalOps::new(r);
    let (data, stats) = if ops.only_basic() {
        let b = ops.basic;
        shade(src.data(), &lut, |enc| basic_encoded(enc, b), strategy)?
    } else {
        shade(src.data(), &lut, |enc| global_encoded(enc, &ops), strategy)?
    };
    Ok((Rgba8::new(src.width(), src.height(), data)?, stats))
}

/// Spatial path: stages 1-5 per pixel through [`shade_f32`] (kernel chosen as in the byte path),
/// then `apply_global` and `apply_detail` on the unquantized RGB buffer (capacity `3 * pixels`).
/// The byte buffer is allocated by the caller only after this returns.
fn spatial_samples(
    src: &Rgba8,
    r: &Recipe,
    sp: &SpatialOps,
    seed: u32,
    strategy: Strategy,
) -> (Vec<f32>, MemoStats) {
    let lut = linear_stage_lut(&r.basic);
    let ops = GlobalOps::new(r);
    let (mut samples, stats) = if ops.only_basic() {
        let b = ops.basic;
        shade_f32(src.data(), &lut, |enc| basic_encoded(enc, b), strategy)
    } else {
        shade_f32(src.data(), &lut, |enc| global_encoded(enc, &ops), strategy)
    };
    let (w, h) = (src.width() as usize, src.height() as usize);
    sp.apply_global(&mut samples, w, h);
    sp.apply_detail(&mut samples, w, h, seed);
    (samples, stats)
}

/// Stages 4-5 by `kernel` on LUT-encoded RGBA8 pixels `px`, then stage 7 and quantization.
/// Returns RGBA8 bytes with alpha 255.
fn shade<K: Fn([f32; 3]) -> [f32; 3]>(
    px: &[u8],
    lut: &[[f32; 256]; 3],
    kernel: K,
    strategy: Strategy,
) -> Result<(Vec<u8>, MemoStats), RenderError> {
    drive::<[u8; 3], u8, RenderError>(
        px,
        strategy,
        px.len(),
        |p| finish(kernel(lut_pixel(p, lut))),
        |out, [r, g, b]| out.extend_from_slice(&[r, g, b, 255]),
    )
}

/// Stages 4-5 by `kernel` on LUT-encoded RGBA8 pixels `px`, unquantized: RGB triples (length
/// `3 * pixels`), no finiteness check, so NaN and infinity pass through with their bits. Same
/// probe and fallback rules as [`shade`].
fn shade_f32<K: Fn([f32; 3]) -> [f32; 3]>(
    px: &[u8],
    lut: &[[f32; 256]; 3],
    kernel: K,
    strategy: Strategy,
) -> (Vec<f32>, MemoStats) {
    let infallible = drive::<[f32; 3], f32, std::convert::Infallible>(
        px,
        strategy,
        px.len() / 4 * 3,
        |p| Ok(kernel(lut_pixel(p, lut))),
        |out, rgb| out.extend_from_slice(&rgb),
    );
    match infallible {
        Ok(done) => done,
        Err(never) => match never {},
    }
}

/// Probe, decide, then cache or go direct, shared by [`shade`] and [`shade_f32`]. `compute` maps
/// one RGBA8 pixel to its value `V`, `push` appends it to the output. Only misses call `compute`;
/// a hit's key was stored from a successful miss with the same bytes, so an error always surfaces
/// on a miss.
fn drive<V, T, E>(
    px: &[u8],
    strategy: Strategy,
    capacity: usize,
    compute: impl Fn(&[u8]) -> Result<V, E>,
    push: impl Fn(&mut Vec<T>, V),
) -> Result<(Vec<T>, MemoStats), E>
where
    V: Copy + Default,
{
    let mut out = Vec::with_capacity(capacity);
    let mut stats = MemoStats::default();
    let direct = |px: &[u8], out: &mut Vec<T>| -> Result<(), E> {
        for p in px.chunks_exact(4) {
            push(out, compute(p)?);
        }
        Ok(())
    };
    let memoized = |px: &[u8], cache: &mut memo::Memo<V>, out: &mut Vec<T>| -> Result<usize, E> {
        let mut hits = 0;
        for p in px.chunks_exact(4) {
            let key = memo::key(p[0], p[1], p[2]);
            let value = if let Some(v) = cache.get(key) {
                hits += 1;
                v
            } else {
                let v = compute(p)?;
                cache.insert(key, v);
                v
            };
            push(out, value);
        }
        Ok(hits)
    };
    match strategy {
        Strategy::Direct => direct(px, &mut out)?,
        Strategy::Memo => {
            let total = px.len() / 4;
            let (probe, rest) = px.split_at(total.min(memo::PROBE_PIXELS) * 4);
            let mut cache = memo::Memo::<V>::new();
            stats.probe_hits = memoized(probe, &mut cache, &mut out)?;
            stats.fell_back = memo::falls_back(total, stats.probe_hits);
            if stats.fell_back {
                drop(cache);
                direct(rest, &mut out)?;
            } else {
                memoized(rest, &mut cache, &mut out)?;
            }
        }
    }
    Ok((out, stats))
}

/// Stages 1-3 of one RGBA8 pixel from the LUT (alpha ignored).
fn lut_pixel(p: &[u8], lut: &[[f32; 256]; 3]) -> [f32; 3] {
    [
        lut[0][usize::from(p[0])],
        lut[1][usize::from(p[1])],
        lut[2][usize::from(p[2])],
    ]
}

/// Stage 7 for one pixel, then the render's single final quantization (A3 point ①). The same
/// check and the same error as [`finite`] on the whole image.
fn finish(px: [f32; 3]) -> Result<[u8; 3], RenderError> {
    if px.iter().all(|v| v.is_finite()) {
        Ok(px.map(quantize))
    } else {
        Err(RenderError::NonFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{
        identity_curve, ColorGrade, Crop, GradeZone, Hsl, Mask, RangeChannel, SplitToning,
    };

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
    fn tone_curve_lifting_midtones_brightens_the_image() {
        let src = gradient(16, 8);
        let r = norm(|r| r.tone_curve.master = vec![[0.0, 0.0], [128.0, 170.0], [255.0, 255.0]]);
        let out = render(&src, &r, &CTX).unwrap();
        let mean = |img: &ImageF32| img.pixels().map(basic::luma).sum::<f32>();
        assert!(mean(&out) > mean(&src));
    }

    /// Review Y1 on both entry points: the curves whose textbook tangent limiter gave NaN.
    #[test]
    fn tiny_secant_curves_render_finite_on_both_paths() {
        let curves: [Vec<[f32; 2]>; 4] = [
            vec![[0.0, 0.0], [1.0, 1e-38], [2.0, 255.0]],
            vec![[0.0, 0.0], [128.0, 1e-38], [255.0, 255.0]],
            vec![[0.0, 255.0], [1.0, 1e-38], [2.0, 0.0]],
            vec![[0.0, 255.0], [128.0, 1e-38], [255.0, 0.0]],
        ];
        for pts in curves {
            let r = norm(|r| r.tone_curve.master = pts.clone());
            for v in [0u8, 1, 128, 255] {
                let src = Rgba8::new(1, 1, vec![v, v, v, 255]).unwrap();
                let general = render(&ImageF32::from_rgba8(&src), &r, &CTX);
                let fast = render_rgba8_f32(&src, &r, CTX);
                let (general, fast) = (general.unwrap(), fast.unwrap());
                assert_same_bits(&fast, &general);
            }
        }
    }

    #[test]
    fn hsl_saturation_minus_100_grays_vivid_pixels() {
        let src = ImageF32::new_filled(2, 1, [0.8, 0.2, 0.15]).unwrap();
        let out = render(&src, &norm(|r| r.hsl.saturation = [-100.0; 8]), &CTX).unwrap();
        let px = out.pixel(0, 0);
        let chroma = px[0].max(px[1]).max(px[2]) - px[0].min(px[1]).min(px[2]);
        assert!(chroma <= 1e-5, "{px:?}");
    }

    /// P1: inactive spatial features leave `SpatialOps` empty, so the byte path runs.
    #[test]
    fn spatial_ops_are_none_without_spatial_features() {
        let r = norm(|r| {
            r.detail.sharpen_radius = 2.0;
            r.detail.grain_size = 80.0;
            r.detail.vignette_midpoint = 10.0;
            r.basic.dehaze = 30.0;
        });
        assert!(SpatialOps::new(r.recipe()).is_none());
    }

    #[test]
    fn clarity_renders_identically_on_both_entry_points() {
        let src = all_levels();
        for (t, c) in [(40.0, 0.0), (0.0, -70.0), (100.0, 100.0)] {
            let r = norm(|r| {
                r.basic.texture = t;
                r.basic.clarity = c;
            });
            assert_eq!(
                render_rgba8(&src, &r, &CTX).unwrap(),
                render(&ImageF32::from_rgba8(&src), &r, &CTX)
                    .unwrap()
                    .to_rgba8()
            );
            let sp = SpatialOps::new(r.recipe()).unwrap();
            let (fast, _) = spatial_samples(&src, r.recipe(), &sp, 0, Strategy::Memo);
            let reference = render(&ImageF32::from_rgba8(&src), &r, &CTX).unwrap();
            assert_eq!(
                fast.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                reference
                    .samples()
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn rejects_each_plan_1b_feature_instead_of_ignoring_it() {
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
    fn split_toning_warms_highlights() {
        let src = ImageF32::new_filled(2, 2, [0.9; 3]).unwrap();
        let r = norm(|r| {
            r.split_toning.highlight_hue = 40.0;
            r.split_toning.highlight_sat = 80.0;
        });
        let px = render(&src, &r, &CTX).unwrap().pixel(1, 1);
        assert!(px[0] > px[2], "{px:?}");
    }

    #[test]
    fn color_grade_lifts_shadows_in_render() {
        let src = ImageF32::new_filled(2, 2, [0.1; 3]).unwrap();
        let r = norm(|r| r.color_grade.shadows.lum = 100.0);
        let px = render(&src, &r, &CTX).unwrap().pixel(0, 1);
        assert!(px.iter().all(|&v| (v - 0.2).abs() <= 1e-6), "{px:?}");
    }

    #[test]
    fn grade_hue_or_blending_alone_has_no_effect_and_is_accepted() {
        let src = gradient(4, 4);
        let r = norm(|r| {
            r.color_grade.shadows.hue = 200.0;
            r.color_grade.blending = 80.0;
            r.detail.sharpen_radius = 2.0;
        });
        let base = norm(|r| r.detail.sharpen_radius = 2.0);
        assert_same_bits(
            &render(&src, &r, &CTX).unwrap(),
            &render(&src, &base, &CTX).unwrap(),
        );
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

    /// The LUT path must equal the reference `render` bit for bit on every 8-bit level.
    fn assert_lut_matches_reference(recipes: &[Normalized]) {
        let src = all_levels();
        for (k, r) in recipes.iter().enumerate() {
            let reference = render(&ImageF32::from_rgba8(&src), r, &CTX).unwrap();
            let fast = render_rgba8_f32(&src, r, CTX).unwrap();
            assert_same_bits(&fast, &reference);
            assert_eq!(fast.to_rgba8(), reference.to_rgba8(), "recipe {k}");
        }
    }

    #[test]
    fn lut_render_matches_reference_on_grid() {
        assert_lut_matches_reference(&basic_grid());
    }

    /// One setting per stage-5 feature, plus basic values that push samples outside [0, 1]
    /// before stage 5.
    fn color_features() -> [fn(&mut Recipe); 6] {
        [
            |r| {
                r.tone_curve.master = vec![
                    [0.0, 20.0],
                    [64.0, 40.0],
                    [128.0, 128.0],
                    [192.0, 215.0],
                    [255.0, 240.0],
                ];
            },
            |r| {
                r.tone_curve.red = vec![[0.0, 0.0], [128.0, 150.0], [255.0, 255.0]];
                r.tone_curve.blue = vec![[0.0, 30.0], [255.0, 220.0]];
            },
            |r| {
                r.hsl.hue = [100.0, -100.0, 50.0, -50.0, 30.0, -30.0, 70.0, -70.0];
                r.hsl.saturation = [60.0, -40.0, 20.0, -100.0, 100.0, 35.0, -15.0, 5.0];
                r.hsl.luminance = [-50.0, 25.0, 40.0, -20.0, 10.0, -60.0, 100.0, -100.0];
            },
            |r| {
                r.color_grade = ColorGrade {
                    shadows: GradeZone {
                        hue: 220.0,
                        sat: 80.0,
                        lum: -40.0,
                    },
                    midtones: GradeZone {
                        hue: 30.0,
                        sat: 25.0,
                        lum: 10.0,
                    },
                    highlights: GradeZone {
                        hue: 50.0,
                        sat: 60.0,
                        lum: 30.0,
                    },
                    global: GradeZone {
                        hue: 300.0,
                        sat: 10.0,
                        lum: -5.0,
                    },
                    blending: 80.0,
                    balance: -30.0,
                };
            },
            |r| {
                r.split_toning = SplitToning {
                    shadow_hue: 180.0,
                    shadow_sat: 60.0,
                    highlight_hue: 40.0,
                    highlight_sat: 70.0,
                    balance: 30.0,
                };
            },
            |r| {
                r.basic.exposure = 2.5;
                r.basic.contrast = 80.0;
                r.basic.vibrance = -60.0;
            },
        ]
    }

    /// Every subset of `color_features`, the full set last.
    fn color_grid() -> Vec<Normalized> {
        let features = color_features();
        (0..1u32 << features.len())
            .map(|bits| {
                norm(|r| {
                    for (i, f) in features.iter().enumerate() {
                        if bits >> i & 1 == 1 {
                            f(r);
                        }
                    }
                })
            })
            .collect()
    }

    #[test]
    fn lut_render_matches_reference_on_color_grid() {
        assert_lut_matches_reference(&color_grid());
    }

    #[test]
    fn inactive_global_color_settings_are_bitwise_noops() {
        let src = all_levels();
        let base = norm(|r| {
            r.basic.exposure = 0.4;
            r.basic.saturation = 20.0;
        });
        let inert = norm(|r| {
            r.basic.exposure = 0.4;
            r.basic.saturation = 20.0;
            r.tone_curve.green = identity_curve();
            r.color_grade.midtones.hue = 120.0;
            r.color_grade.blending = 0.0;
            r.color_grade.balance = 100.0;
            r.split_toning.shadow_hue = 200.0;
            r.split_toning.balance = -100.0;
        });
        assert_eq!(
            render_rgba8(&src, &inert, &CTX).unwrap(),
            render_rgba8(&src, &base, &CTX).unwrap()
        );
        assert_same_bits(
            &render(&ImageF32::from_rgba8(&src), &inert, &CTX).unwrap(),
            &render(&ImageF32::from_rgba8(&src), &base, &CTX).unwrap(),
        );
    }

    #[test]
    fn extreme_global_color_values_stay_finite() {
        let src = ImageF32::from_rgba8(&all_levels());
        for bits in 0..128u32 {
            let on = |i: u32| bits >> i & 1 == 1;
            let pm = |i: u32| if on(i) { 100.0 } else { -100.0 };
            let r = norm(|r| {
                r.basic.exposure = if on(0) { 5.0 } else { -5.0 };
                r.basic.contrast = pm(1);
                r.hsl = Hsl {
                    hue: [pm(2); 8],
                    saturation: [pm(2); 8],
                    luminance: [pm(2); 8],
                };
                r.tone_curve.master = if on(3) {
                    vec![[0.0, 0.0], [1.0, 255.0], [255.0, 255.0]]
                } else {
                    vec![[0.0, 255.0], [255.0, 0.0]]
                };
                let z = GradeZone {
                    hue: 300.0,
                    sat: 100.0,
                    lum: pm(4),
                };
                r.color_grade = ColorGrade {
                    shadows: z,
                    midtones: z,
                    highlights: z,
                    global: z,
                    blending: if on(5) { 100.0 } else { 0.0 },
                    balance: pm(5),
                };
                r.split_toning = SplitToning {
                    shadow_hue: 200.0,
                    shadow_sat: 100.0,
                    highlight_hue: 30.0,
                    highlight_sat: 100.0,
                    balance: pm(6),
                };
            });
            assert!(render(&src, &r, &CTX).is_ok(), "combination {bits:#09b}");
        }
    }

    /// Order-only check: each stage's own effect is verified by its module's tests.
    #[test]
    fn stage_5_order_is_curves_hsl_grading_split() {
        let all = color_grid().pop().unwrap();
        let rec = all.recipe();
        let ops = GlobalOps::new(rec);
        let curves = curve::CurveSet::new(&rec.tone_curve).unwrap();
        let hsl = hsl::HslPlan::new(&rec.hsl).unwrap();
        let grade = grade::GradePlan::new(&rec.color_grade).unwrap();
        let split = grade::SplitPlan::new(&rec.split_toning).unwrap();
        let b = &rec.basic;
        for px in [[0.1, 0.5, 0.9], [0.8, 0.3, 0.2], [1.6, 1.1, 0.4]] {
            let stage4 = basic::apply_color_encoded(
                basic::apply_tone_encoded(px, b),
                b.vibrance,
                b.saturation,
            );
            let expected = split.apply(grade.apply(hsl.apply(curves.apply(stage4))));
            assert_eq!(
                global_encoded(px, &ops).map(f32::to_bits),
                expected.map(f32::to_bits)
            );
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

    // ---- Task 6a: byte memo and stage-5-free loop (M1-M7) ----

    fn global_max() -> Normalized {
        Recipe::from_json(include_str!(
            "../../../golden/recipes/player_global_max.json"
        ))
        .unwrap()
    }

    /// Memo path, direct path and the f32 reference all give the same bytes.
    fn assert_memo_matches(src: &Rgba8, r: &Normalized) -> MemoStats {
        let (memo, stats) = render_rgba8_with(src, r, CTX, Strategy::Memo).unwrap();
        let (direct, _) = render_rgba8_with(src, r, CTX, Strategy::Direct).unwrap();
        let reference = render(&ImageF32::from_rgba8(src), r, &CTX)
            .unwrap()
            .to_rgba8();
        assert_eq!(direct, reference, "direct path");
        assert_eq!(memo, reference, "memo path");
        assert_eq!(
            render_rgba8(src, r, &CTX).unwrap(),
            reference,
            "entry point"
        );
        stats
    }

    /// Fixed-seed LCG bytes (Numerical Recipes constants), top byte of each step.
    fn lcg_bytes(n: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 24) as u8
            })
            .collect()
    }

    /// RGBA8 image whose RGB bytes are uniform LCG noise and alpha varies.
    fn noise(w: u32, h: u32, seed: u32) -> Rgba8 {
        let mut d = lcg_bytes((w * h * 4) as usize, seed);
        for (i, a) in d.iter_mut().skip(3).step_by(4).enumerate() {
            *a = (i % 256) as u8;
        }
        Rgba8::new(w, h, d).unwrap()
    }

    /// M1: all 8-bit levels, basic-only recipes, the 64-subset color grid and the max recipe.
    #[test]
    fn memo_render_matches_reference_on_all_levels() {
        let src = all_levels();
        let basic = basic_grid();
        let picks = [0usize, 341, 682, 1023, 1024, 1500, 2047];
        for r in picks.iter().map(|&i| &basic[i]).chain(&color_grid()) {
            assert_memo_matches(&src, r);
        }
        assert_memo_matches(&src, &global_max());
    }

    /// M2: 512x512 uniform noise misses almost always, so the memo falls back after the probe.
    #[test]
    fn memo_falls_back_on_noise_and_stays_bit_identical() {
        let stats = assert_memo_matches(&noise(512, 512, 0x5EED), &global_max());
        assert!(stats.fell_back, "{stats:?}");
    }

    /// RGBA8 image cycling through 64 fixed colors (alpha 0): large but nearly all cache hits.
    fn few_colors(w: u32, h: u32) -> Rgba8 {
        let palette = lcg_bytes(64 * 3, 7);
        let mut d = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..(w * h) as usize {
            let c = (i * 37 / 5) % 64;
            d.extend_from_slice(&palette[c * 3..c * 3 + 3]);
            d.push(0);
        }
        Rgba8::new(w, h, d).unwrap()
    }

    /// M2 (other branch): a large image with few colors keeps the memo past the probe.
    #[test]
    fn memo_keeps_cache_on_few_colors_and_stays_bit_identical() {
        let stats = assert_memo_matches(&few_colors(512, 512), &global_max());
        assert!(!stats.fell_back, "{stats:?}");
    }

    /// F1: the f32 memo returns exactly the kernel output, cached or not, with or without fallback.
    #[test]
    fn f32_memo_matches_direct_bits() {
        let r = norm(|r| r.hsl.saturation = [30.0; 8]);
        let ops = GlobalOps::new(r.recipe());
        let lut = linear_stage_lut(&r.recipe().basic);
        for (img, falls) in [(noise(512, 512, 9), true), (few_colors(512, 512), false)] {
            let (memo, stats) = shade_f32(
                img.data(),
                &lut,
                |e| global_encoded(e, &ops),
                Strategy::Memo,
            );
            let (direct, _) = shade_f32(
                img.data(),
                &lut,
                |e| global_encoded(e, &ops),
                Strategy::Direct,
            );
            assert_eq!(stats.fell_back, falls);
            assert_eq!(
                memo.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                direct.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
        }
    }

    /// F2: a NaN-producing kernel gives the same NaN bits on both strategies.
    #[test]
    fn f32_memo_keeps_nan_bits() {
        let lut = linear_stage_lut(&Recipe::default().basic);
        let bad = lut[0][77];
        let kernel = |px: [f32; 3]| {
            if px[0].to_bits() == bad.to_bits() {
                [f32::NAN, px[1], px[2]]
            } else {
                px
            }
        };
        let mut px = [10u8, 20, 30, 255].repeat(50);
        px.extend_from_slice(&[77, 0, 0, 255, 77, 0, 0, 255]);
        let bits = |v: Vec<f32>| v.into_iter().map(f32::to_bits).collect::<Vec<_>>();
        assert_eq!(
            bits(shade_f32(&px, &lut, kernel, Strategy::Memo).0),
            bits(shade_f32(&px, &lut, kernel, Strategy::Direct).0)
        );
    }

    /// M3: distinct keys that share one slot, interleaved so every visit evicts the previous one.
    #[test]
    fn memo_render_is_exact_under_hash_collisions() {
        let target = memo::slot_of(memo::key(200, 40, 90));
        let colliding: Vec<[u8; 3]> = (0..1u32 << 24)
            .map(|v| [v as u8, (v >> 8) as u8, (v >> 16) as u8])
            .filter(|p| memo::slot_of(memo::key(p[0], p[1], p[2])) == target)
            .take(6)
            .collect();
        assert_eq!(colliding.len(), 6);
        let mut d = Vec::new();
        for i in 0..600usize {
            let p = colliding[(i * 7 + i / 6) % 6];
            d.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
        let src = Rgba8::new(600, 1, d).unwrap();
        assert_memo_matches(&src, &global_max());
        assert_memo_matches(&src, &norm(|r| r.basic.exposure = 0.3));
    }

    /// M4: images at or below the probe size finish on the memo without a decision.
    #[test]
    fn memo_render_matches_reference_on_small_images() {
        let r = global_max();
        for (k, (w, h)) in [(1u32, 1u32), (3, 1), (1, 3), (255, 257), (256, 256)]
            .into_iter()
            .enumerate()
        {
            let stats = assert_memo_matches(&noise(w, h, k as u32), &r);
            assert!(!stats.fell_back, "{w}x{h}");
        }
    }

    /// M5: a test kernel that turns one input level non-finite (validated recipes cannot, see
    /// `extreme_global_color_values_stay_finite`). Both strategies return the same error, whether
    /// the bad pixel comes first, after repeats, or after the fallback decision.
    #[test]
    fn memo_and_direct_report_the_same_non_finite_error() {
        let lut = linear_stage_lut(&Recipe::default().basic);
        let bad = lut[0][77];
        let kernel = |px: [f32; 3]| {
            if px[0].to_bits() == bad.to_bits() {
                [px[0], f32::NAN, px[2]]
            } else {
                px
            }
        };
        // Keep level 77 out of the probe so the error comes from the direct fallback loop.
        let mut late = noise(300, 300, 3).into_data();
        for p in late.chunks_exact_mut(4).filter(|p| p[0] == 77) {
            p[0] = 78;
        }
        let (_, stats) =
            shade(&late, &lut, kernel, Strategy::Memo).expect("finite without level 77");
        assert!(stats.fell_back);
        let n = late.len();
        late[n - 8] = 77;
        let mut repeated = [10u8, 20, 30, 255].repeat(100);
        repeated.extend_from_slice(&[77, 0, 0, 255, 77, 0, 0, 255]);
        for px in [late, repeated, vec![77, 1, 2, 3]] {
            let memo = shade(&px, &lut, kernel, Strategy::Memo).map(|(d, _)| d);
            let direct = shade(&px, &lut, kernel, Strategy::Direct).map(|(d, _)| d);
            assert_eq!(memo, Err(RenderError::NonFinite));
            assert_eq!(direct, Err(RenderError::NonFinite));
        }
    }

    /// M7: the stage-4-only kernel equals `global_encoded` bit for bit when stage 5 is inactive.
    #[test]
    fn basic_only_kernel_matches_global_encoded() {
        let src = all_levels();
        for r in &basic_grid() {
            let rec = r.recipe();
            let ops = GlobalOps::new(rec);
            assert!(ops.only_basic());
            let lut = linear_stage_lut(&rec.basic);
            for p in src.data().chunks_exact(4) {
                let enc = [
                    lut[0][usize::from(p[0])],
                    lut[1][usize::from(p[1])],
                    lut[2][usize::from(p[2])],
                ];
                assert_eq!(
                    basic_encoded(enc, &rec.basic).map(f32::to_bits),
                    global_encoded(enc, &ops).map(f32::to_bits)
                );
            }
        }
    }

    /// M7 (dispatch): any single stage-5 feature leaves the stage-4-only loop.
    #[test]
    fn any_stage5_feature_disables_the_basic_only_loop() {
        let features = color_features();
        for f in &features[..5] {
            let r = norm(f);
            assert!(!GlobalOps::new(r.recipe()).only_basic());
        }
        assert!(GlobalOps::new(norm(|r| features[5](r)).recipe()).only_basic());
        let r = norm(|r| r.basic.dehaze = 10.0);
        assert!(!GlobalOps::new(r.recipe()).only_basic());
    }

    #[test]
    fn dehaze_disables_the_basic_only_loop_and_renders() {
        let r = norm(|r| r.basic.dehaze = 40.0);
        assert!(!GlobalOps::new(r.recipe()).only_basic());
        let src = ImageF32::new_filled(2, 2, [0.8; 3]).unwrap();
        assert!(render(&src, &r, &CTX).unwrap().pixel(0, 0)[0] < 0.8);
    }

    #[test]
    fn dehaze_lut_path_matches_reference_on_all_levels() {
        let src = all_levels();
        for d in [-100.0, -35.0, 20.0, 100.0] {
            let r = norm(|r| r.basic.dehaze = d);
            assert_eq!(
                render_rgba8(&src, &r, &CTX).unwrap(),
                render(&ImageF32::from_rgba8(&src), &r, &CTX)
                    .unwrap()
                    .to_rgba8()
            );
        }
    }
}
