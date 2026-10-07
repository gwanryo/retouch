//! Scoring (AC-S1a..k, S2, S3). Master plan A5/A10: a [`ScoringReference`] is built ONCE per
//! challenge from the full-resolution original and answer renders (2048) plus the region spec.
//! It rasterizes E at 2048, maps it to the scoring resolution, and caches the scoring images and
//! Lab values. Each submission is a full-frame 2048 player render.
//!
//! Plan 1a: E = full, leakage penalty 0, no composition score, diagnostics tone/color with a
//! provisional formula (AC-S2 formulas replace it in Plan 1b with a `SCORING_VERSION` bump).

pub mod gate;

pub use gate::{
    error_stats, scoring_params, verdict, ErrorStats, ScoringParams, StatsError, Verdict,
    BIG_ERROR_DE, BIG_ERROR_MAX_RATIO, MIN_ELIGIBLE_DE, PERFECT_MEAN_DE, TILE, T_TILE,
};

use crate::buffer::{dequantize, Rgba8};
use crate::color::{delta_e00, linear_srgb_to_lab_d50, srgb_decode, Lab};
use crate::recipe::Crop;
use crate::region::{RegionError, RegionMask, RegionSpec};
use crate::resample::{scoring_image, ResampleError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use thiserror::Error;

/// Scoring failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScoreError {
    /// Images differ in size.
    #[error("size mismatch: expected {expected_w}x{expected_h}, got {got_w}x{got_h}")]
    SizeMismatch {
        /// Reference width.
        expected_w: u32,
        /// Reference height.
        expected_h: u32,
        /// Offending width.
        got_w: u32,
        /// Offending height.
        got_h: u32,
    },
    /// Region spec or rasterization failed.
    #[error(transparent)]
    Region(#[from] RegionError),
    /// Statistics failed (empty E, length mismatch).
    #[error(transparent)]
    Stats(#[from] StatsError),
    /// Scoring-image construction failed.
    #[error(transparent)]
    Resample(#[from] ResampleError),
    /// Composition input given; composition scoring arrives in Plan 1b.
    #[error("`{0}` is scored from Plan 1b")]
    NotYetSupported(&'static str),
}

/// Crop boxes for the composition score (master plan §3.4 `crop_json`, A10). Plan 1a accepts
/// only "no boxes".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionInput {
    /// The answer's crop box (manifest).
    #[serde(default)]
    pub answer: Option<Crop>,
    /// The player's crop box.
    #[serde(default)]
    pub player: Option<Crop>,
}

/// One entry of `top_error_regions` (master plan §3.3). Always empty in Plan 1a.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorRegion {
    /// Left, normalized.
    pub x: f64,
    /// Top, normalized.
    pub y: f64,
    /// Width, normalized.
    pub w: f64,
    /// Height, normalized.
    pub h: f64,
    /// Mean player − answer ΔL*.
    pub mean_dl: f64,
    /// Mean Δa*.
    pub mean_da: f64,
    /// Mean Δb*.
    pub mean_db: f64,
}

/// Diagnostic indicators 0..100; `None` serializes as `null` = "평가 없음" (AC-S2d).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    /// Provisional: relative score of mean |ΔL*|. `None` when the original is already within 1.
    pub tone: Option<u8>,
    /// Provisional: relative score of mean Δa*b* distance. `None` likewise.
    pub color: Option<u8>,
    /// Plan 1b.
    pub detail: Option<u8>,
    /// Plan 1b.
    pub local: Option<u8>,
    /// Plan 1b.
    pub composition: Option<u8>,
}

/// Score JSON (master plan §3.3). Field order is the serialization order and part of the
/// golden contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// 0..100 shown to the player.
    pub correction_score: u8,
    /// Unrounded relative score before the perfect gate.
    pub raw_score: f64,
    /// All three perfect conditions met.
    pub perfect: bool,
    /// `d_e_original >= 3` (AC-S1j).
    pub eligible: bool,
    /// Mean ΔE00 player↔answer over E.
    pub d_e: f64,
    /// Mean ΔE00 original↔answer over E.
    pub d_e_original: f64,
    /// Same value as `d_e` (kept for the §3.3 contract; meaning fixed: mean over E).
    pub mean_de: f64,
    /// Max over tiles of the per-tile p95 ΔE00.
    pub tile_p95_max: f64,
    /// [`T_TILE`] used for this score.
    pub t_tile: f64,
    /// Fraction of E with ΔE00 > [`BIG_ERROR_DE`].
    pub big_error_ratio: f64,
    /// Plan 1b; 0 here.
    pub leakage_penalty: f64,
    /// Plan 1b; `null` here.
    pub composition_score: Option<u8>,
    /// Diagnostic indicators.
    pub diagnostics: Diagnostics,
    /// Plan 1b; `[]` here.
    pub top_error_regions: Vec<ErrorRegion>,
    /// [`crate::ENGINE_VERSION`].
    pub engine_version: String,
    /// [`crate::SCORING_VERSION`].
    pub scoring_version: String,
}

/// Hasher for packed color keys. Memo lookups only decide speed, never values: every entry is
/// the same pure function of its key, so results are bit-identical to computing per pixel.
#[derive(Clone, Copy, Default)]
struct ColorHasher(u64);

impl Hasher for ColorHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(self.0 ^ u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        // multiplicative mix; fold the high half down so both bucket bits and tag bits vary
        let h = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = h ^ (h >> 32);
    }
}

type ColorMemo<V> = HashMap<u64, V, BuildHasherDefault<ColorHasher>>;

/// RGB bytes of an RGBA8 pixel as a 24-bit key.
fn rgb_key(p: &[u8]) -> u64 {
    u64::from(p[0]) << 16 | u64::from(p[1]) << 8 | u64::from(p[2])
}

/// Lab of every pixel, computed once per distinct color (Plan 1b-1 C1/C4).
fn lab_image(img: &Rgba8) -> Vec<Lab> {
    let lut: Vec<f32> = (0..=255u8).map(|v| srgb_decode(dequantize(v))).collect();
    let mut memo = ColorMemo::<Lab>::default();
    img.data()
        .chunks_exact(4)
        .map(|p| {
            *memo.entry(rgb_key(p)).or_insert_with(|| {
                let c = |i: usize| lut[usize::from(p[i])];
                linear_srgb_to_lab_d50(c(0), c(1), c(2))
            })
        })
        .collect()
}

/// Per-pixel ΔE00 between two scoring images in pixel order. A pixel whose RGB bytes are equal
/// has equal Lab, and ΔE00 of equal Lab is exactly `+0.0` (exhaustively tested in `color`), so
/// it is skipped. Otherwise ΔE00 is computed once per distinct (a, b) color pair: Lab is a pure
/// function of the color, so the value at every position is bit-identical to `delta_e00`.
fn delta_e_image<'a>(
    a: &'a Rgba8,
    lab_a: &'a [Lab],
    b: &'a Rgba8,
    lab_b: &'a [Lab],
) -> impl Iterator<Item = f64> + 'a {
    a.data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .zip(lab_a.iter().zip(lab_b))
        .scan(ColorMemo::<f64>::default(), |memo, ((pa, pb), (la, lb))| {
            Some(if pa[..3] == pb[..3] {
                0.0
            } else {
                *memo
                    .entry(rgb_key(pa) << 24 | rgb_key(pb))
                    .or_insert_with(|| delta_e00(*la, *lb))
            })
        })
}

fn mean_over(values: impl Iterator<Item = f64>, mask: &RegionMask) -> f64 {
    let (sum, n) = values
        .enumerate()
        .filter(|(i, _)| mask.contains(*i))
        .fold((0.0, 0usize), |(s, n), (_, v)| (s + v, n + 1));
    sum / n as f64
}

fn provisional_diag(player: f64, original: f64) -> Option<u8> {
    (original > 1.0).then(|| {
        let r = (100.0 * (1.0 - (player - 1.0) / (original - 1.0))).clamp(0.0, 100.0);
        (r + 0.5).floor() as u8
    })
}

fn check_size(img: &Rgba8, w: u32, h: u32) -> Result<(), ScoreError> {
    if (img.width(), img.height()) == (w, h) {
        Ok(())
    } else {
        Err(ScoreError::SizeMismatch {
            expected_w: w,
            expected_h: h,
            got_w: img.width(),
            got_h: img.height(),
        })
    }
}

/// Everything about original/answer/region that does not depend on the player. Build once
/// when a challenge loads, score many submissions (A5).
#[derive(Clone, Debug)]
pub struct ScoringReference {
    width: u32,
    height: u32,
    mask: RegionMask,
    original_scoring: Rgba8,
    answer_scoring: Rgba8,
    lab_answer: Vec<Lab>,
    d_e_original: f64,
    dl_original: f64,
    dc_original: f64,
}

impl ScoringReference {
    /// `original` and `answer` are full-resolution, full-frame renders of equal size.
    pub fn new(original: &Rgba8, answer: &Rgba8, region: &RegionSpec) -> Result<Self, ScoreError> {
        let (width, height) = (original.width(), original.height());
        check_size(answer, width, height)?;
        let original_scoring = scoring_image(original, crate::SCORING_LONG_EDGE)?;
        let answer_scoring = scoring_image(answer, crate::SCORING_LONG_EDGE)?;
        let (sw, sh) = (answer_scoring.width(), answer_scoring.height());
        let mask = region.rasterize(width, height)?.resample_to(sw, sh)?;
        if mask.count() == 0 {
            return Err(StatsError::EmptyRegion.into());
        }
        let lab_answer = lab_image(&answer_scoring);
        let lab_o = lab_image(&original_scoring);
        let pairs = || lab_o.iter().zip(&lab_answer);
        let d_e_original = mean_over(
            delta_e_image(&original_scoring, &lab_o, &answer_scoring, &lab_answer),
            &mask,
        );
        let dl_original = mean_over(pairs().map(|(o, a)| (o.l - a.l).abs()), &mask);
        let dc_original = mean_over(
            pairs().map(|(o, a)| libm::hypot(o.a - a.a, o.b - a.b)),
            &mask,
        );
        Ok(Self {
            width,
            height,
            mask,
            original_scoring,
            answer_scoring,
            lab_answer,
            d_e_original,
            dl_original,
            dc_original,
        })
    }

    /// The original's scoring image (A4).
    pub fn original_scoring(&self) -> &Rgba8 {
        &self.original_scoring
    }

    /// The answer's scoring image (A4); its hash is `answer.sha256_rgba8_1024` in the manifest.
    pub fn answer_scoring(&self) -> &Rgba8 {
        &self.answer_scoring
    }

    /// Score one submission: a full-frame player render at the original's resolution.
    /// `composition` must be `None` or carry no boxes in Plan 1a.
    pub fn score(
        &self,
        player: &Rgba8,
        composition: Option<&CompositionInput>,
    ) -> Result<Score, ScoreError> {
        if composition.is_some_and(|c| c.answer.is_some() || c.player.is_some()) {
            return Err(ScoreError::NotYetSupported("composition"));
        }
        check_size(player, self.width, self.height)?;
        let player_scoring = scoring_image(player, crate::SCORING_LONG_EDGE)?;
        let lab_p = lab_image(&player_scoring);
        let de: Vec<f64> = delta_e_image(
            &player_scoring,
            &lab_p,
            &self.answer_scoring,
            &self.lab_answer,
        )
        .collect();
        let stats = error_stats(&de, &self.mask)?;
        let v = verdict(&stats, self.d_e_original);
        let pairs = || lab_p.iter().zip(&self.lab_answer);
        let dl = mean_over(pairs().map(|(p, a)| (p.l - a.l).abs()), &self.mask);
        let dc = mean_over(
            pairs().map(|(p, a)| libm::hypot(p.a - a.a, p.b - a.b)),
            &self.mask,
        );
        Ok(Score {
            correction_score: v.correction_score,
            raw_score: v.raw_score,
            perfect: v.perfect,
            eligible: v.eligible,
            d_e: stats.mean_de,
            d_e_original: self.d_e_original,
            mean_de: stats.mean_de,
            tile_p95_max: stats.tile_p95_max,
            t_tile: T_TILE,
            big_error_ratio: stats.big_error_ratio,
            leakage_penalty: 0.0,
            composition_score: None,
            diagnostics: Diagnostics {
                tone: provisional_diag(dl, self.dl_original),
                color: provisional_diag(dc, self.dc_original),
                detail: None,
                local: None,
                composition: None,
            },
            top_error_regions: Vec::new(),
            engine_version: crate::ENGINE_VERSION.to_owned(),
            scoring_version: crate::SCORING_VERSION.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ImageF32;
    use crate::color::{linear_srgb_to_lab_d50, srgb_decode};
    use crate::recipe::Recipe;
    use crate::render::{render_rgba8, RenderContext};

    const FULL: RegionSpec = RegionSpec::Full {};

    fn gradient(w: u32, h: u32) -> Rgba8 {
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
                img.set_pixel(x, y, [0.2 + 0.6 * t(x, w), 0.3 + 0.4 * t(y, h), 0.5]);
            }
        }
        img.to_rgba8()
    }

    fn with_exposure(src: &Rgba8, ev: f32) -> Rgba8 {
        let mut r = Recipe::default();
        r.basic.exposure = ev;
        render_rgba8(src, &r.normalize().unwrap(), &RenderContext::default()).unwrap()
    }

    fn score(o: &Rgba8, a: &Rgba8, p: &Rgba8) -> Score {
        ScoringReference::new(o, a, &FULL)
            .unwrap()
            .score(p, None)
            .unwrap()
    }

    fn gray(w: u32, h: u32, v: u8) -> Vec<u8> {
        [v, v, v, 255].repeat((w * h) as usize)
    }

    fn gray_de(a: u8, b: u8) -> f64 {
        let lab = |v: u8| {
            let l = srgb_decode(f32::from(v) / 255.0);
            linear_srgb_to_lab_d50(l, l, l)
        };
        delta_e00(lab(a), lab(b))
    }

    /// Smallest gray level above `base` whose ΔE00 to `base` is at least `target`.
    fn level_with_de(base: u8, target: f64) -> u8 {
        (base..=255).find(|&v| gray_de(base, v) >= target).unwrap()
    }

    /// Answer = gray 128, original = gray 100 (`D_E` ≈ 10), player = answer with `edit` applied.
    fn gray_case(w: u32, h: u32, edit: impl Fn(u32, u32) -> Option<u8>) -> Score {
        let o = Rgba8::new(w, h, gray(w, h, 100)).unwrap();
        let a = Rgba8::new(w, h, gray(w, h, 128)).unwrap();
        let mut p = gray(w, h, 128);
        for y in 0..h {
            for x in 0..w {
                if let Some(v) = edit(x, y) {
                    let i = ((y * w + x) * 4) as usize;
                    p[i..i + 3].copy_from_slice(&[v, v, v]);
                }
            }
        }
        score(&o, &a, &Rgba8::new(w, h, p).unwrap())
    }

    #[test]
    fn submitting_the_original_scores_zero() {
        let src = gradient(96, 64);
        assert_eq!(
            score(&src, &with_exposure(&src, 1.0), &src).correction_score,
            0
        );
    }

    #[test]
    fn submitting_the_answer_is_perfect_100() {
        let src = gradient(96, 64);
        let ans = with_exposure(&src, 1.0);
        let s = score(&src, &ans, &ans);
        assert!(
            s.perfect && s.correction_score == 100 && s.eligible,
            "{s:?}"
        );
    }

    #[test]
    fn half_the_correction_scores_strictly_between_0_and_100() {
        let src = gradient(96, 64);
        let s = score(&src, &with_exposure(&src, 1.0), &with_exposure(&src, 0.5));
        assert!((1..=99).contains(&s.correction_score), "{s:?}");
    }

    mod perfect_gate_through_score {
        use super::*;

        #[test]
        fn mean_alone_fails() {
            let v = level_with_de(128, 1.2);
            let s = gray_case(128, 128, |_, _| Some(v));
            let others_pass = s.tile_p95_max <= T_TILE && s.big_error_ratio <= BIG_ERROR_MAX_RATIO;
            assert!(s.d_e > 1.0 && others_pass, "{s:?}");
            assert!(!s.perfect && s.correction_score <= 99, "{s:?}");
        }

        #[test]
        fn tile_alone_fails_and_scores_99() {
            let v = level_with_de(128, 3.0); // above T_TILE, not a big error
            let s = gray_case(128, 128, |x, y| (x < 16 && y < 16).then_some(v));
            let others_pass = s.d_e <= 1.0 && s.big_error_ratio <= BIG_ERROR_MAX_RATIO;
            assert!(s.tile_p95_max > T_TILE && others_pass, "{s:?}");
            assert_eq!((s.perfect, s.correction_score), (false, 99), "{s:?}");
        }

        #[test]
        fn big_error_ratio_alone_fails_and_scores_99() {
            let v = level_with_de(128, 8.0); // a big error, but only ~1.5% per tile
            let s = gray_case(128, 128, |x, y| ((x * 7 + y * 13) % 67 == 0).then_some(v));
            let others_pass = s.d_e <= 1.0 && s.tile_p95_max <= T_TILE;
            assert!(
                s.big_error_ratio > BIG_ERROR_MAX_RATIO && others_pass,
                "{s:?}"
            );
            assert_eq!((s.perfect, s.correction_score), (false, 99), "{s:?}");
        }

        #[test]
        fn only_the_last_partial_tile_bad_scores_99() {
            let v = level_with_de(128, 3.0);
            let s = gray_case(70, 70, |x, y| (x >= 64 && y >= 64).then_some(v));
            assert!(s.d_e <= 1.0 && s.tile_p95_max > T_TILE, "{s:?}");
            assert_eq!((s.perfect, s.correction_score), (false, 99), "{s:?}");
        }
    }

    #[test]
    fn identical_original_and_answer_is_ineligible() {
        let src = gradient(16, 16);
        let s = score(&src, &src, &src);
        assert_eq!((s.eligible, s.correction_score), (false, 0));
    }

    #[test]
    fn rejects_player_size_mismatch() {
        let a = gradient(8, 8);
        let reference = ScoringReference::new(&a, &a, &FULL).unwrap();
        let err = reference.score(&gradient(9, 8), None).unwrap_err();
        assert!(
            matches!(err, ScoreError::SizeMismatch { got_w: 9, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rejects_answer_size_mismatch() {
        let err = ScoringReference::new(&gradient(8, 8), &gradient(8, 9), &FULL).unwrap_err();
        assert!(
            matches!(err, ScoreError::SizeMismatch { got_h: 9, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn rejects_composition_boxes_in_plan_1a() {
        let a = gradient(8, 8);
        let reference = ScoringReference::new(&a, &a, &FULL).unwrap();
        let boxes = CompositionInput {
            answer: Some(Crop::FULL),
            player: None,
        };
        let err = reference.score(&a, Some(&boxes)).unwrap_err();
        assert_eq!(err, ScoreError::NotYetSupported("composition"));
        assert!(reference
            .score(&a, Some(&CompositionInput::default()))
            .is_ok());
    }

    /// AC-E3c for scoring: every corner of the basic panel (all 1024 min/max combinations) as
    /// the answer, scored against the complementary corner as the player, yields finite numbers.
    #[test]
    fn extreme_basic_values_score_to_finite_numbers() {
        let src = gradient(16, 12);
        let corner = |bits: u32| {
            let pick = |i: u32, lo: f32, hi: f32| if bits >> i & 1 == 1 { hi } else { lo };
            let mut r = Recipe::default();
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
            render_rgba8(&src, &r.normalize().unwrap(), &RenderContext::default()).unwrap()
        };
        for bits in 0..1024u32 {
            let s = score(&src, &corner(bits), &corner(!bits & 0x3FF));
            let numbers = [
                s.raw_score,
                s.d_e,
                s.d_e_original,
                s.mean_de,
                s.tile_p95_max,
                s.big_error_ratio,
            ];
            assert!(
                numbers.iter().all(|v| v.is_finite()) && s.correction_score <= 100,
                "combination {bits:#012b}: {s:?}"
            );
        }
    }

    #[test]
    fn scores_1x1_images() {
        let a = gradient(1, 1);
        assert!(ScoringReference::new(&a, &with_exposure(&a, 2.0), &FULL)
            .unwrap()
            .score(&a, None)
            .is_ok());
    }

    #[test]
    fn builds_scoring_images_at_1024_from_2048x1365() {
        let a = gradient(2048, 1365);
        let reference = ScoringReference::new(&a, &a, &FULL).unwrap();
        let s = reference.answer_scoring();
        assert_eq!((s.width(), s.height()), (1024, 683));
    }

    #[test]
    fn serialized_keys_match_master_plan_3_3_in_order() {
        let src = gradient(16, 16);
        let s = score(&src, &with_exposure(&src, 1.0), &src);
        let json = serde_json::to_string(&s).unwrap();
        let want = [
            "correction_score",
            "raw_score",
            "perfect",
            "eligible",
            "d_e",
            "d_e_original",
            "mean_de",
            "tile_p95_max",
            "t_tile",
            "big_error_ratio",
            "leakage_penalty",
            "composition_score",
            "diagnostics",
            "top_error_regions",
            "engine_version",
            "scoring_version",
        ];
        // Raw-string positions: serde_json::Map would sort the keys.
        let positions: Vec<usize> = want
            .iter()
            .map(|k| json.find(&format!("\"{k}\":")).unwrap())
            .collect();
        assert!(positions.windows(2).all(|p| p[0] < p[1]), "{json}");
        assert!(json.contains(r#""top_error_regions":[]"#) && json.contains(r#""detail":null"#));
    }

    /// Left half from `left`, right half from `right` (same size).
    fn half_and_half(left: &Rgba8, right: &Rgba8) -> Rgba8 {
        let w = left.width() as usize;
        let data = left
            .data()
            .chunks_exact(4)
            .zip(right.data().chunks_exact(4))
            .enumerate()
            .flat_map(|(i, (l, r))| {
                if i % w < w / 2 {
                    l.to_vec()
                } else {
                    r.to_vec()
                }
            })
            .collect();
        Rgba8::new(left.width(), left.height(), data).unwrap()
    }

    #[test]
    fn delta_e_image_matches_naive_bitwise() {
        let a = gradient(64, 48);
        let shifted = with_exposure(&a, 0.8);
        for (b, equal_pixels) in [
            (a.clone(), 3072),
            (shifted.clone(), 0),
            (half_and_half(&a, &shifted), 1536),
        ] {
            let (sa, sb) = (
                scoring_image(&a, 1024).unwrap(),
                scoring_image(&b, 1024).unwrap(),
            );
            let (la, lb) = (lab_image(&sa), lab_image(&sb));
            let same = sa
                .data()
                .chunks_exact(4)
                .zip(sb.data().chunks_exact(4))
                .filter(|(p, q)| p[..3] == q[..3])
                .count();
            assert_eq!(same, equal_pixels);
            let fast: Vec<u64> = delta_e_image(&sa, &la, &sb, &lb)
                .map(f64::to_bits)
                .collect();
            let naive: Vec<u64> = la
                .iter()
                .zip(&lb)
                .map(|(x, y)| delta_e00(*x, *y).to_bits())
                .collect();
            assert_eq!(fast, naive);
        }
    }

    /// `w`×`h` image whose pixel `i` is `palette[pick(i) % palette.len()]`.
    fn palette_image(w: u32, h: u32, palette: &[[u8; 3]], pick: impl Fn(usize) -> usize) -> Rgba8 {
        let data = (0..(w * h) as usize)
            .flat_map(|i| {
                let c = palette[pick(i) % palette.len()];
                [c[0], c[1], c[2], 255]
            })
            .collect();
        Rgba8::new(w, h, data).unwrap()
    }

    const PALETTE: [[u8; 3]; 16] = [
        [0, 0, 0],
        [255, 255, 255],
        [12, 200, 40],
        [200, 12, 40],
        [40, 12, 200],
        [128, 128, 128],
        [90, 60, 30],
        [30, 60, 90],
        [250, 240, 10],
        [10, 240, 250],
        [1, 2, 3],
        [254, 253, 252],
        [77, 77, 78],
        [77, 78, 77],
        [180, 90, 140],
        [60, 160, 110],
    ];

    /// Same player color against different answer colors (and the reverse) hits the memo with
    /// a key that must include both colors.
    #[test]
    fn delta_e_image_matches_naive_on_repeated_color_pairs() {
        let a = palette_image(97, 61, &PALETTE, |i| i);
        let b = palette_image(97, 61, &PALETTE, |i| i / 3 + i % 5);
        let (la, lb) = (lab_image(&a), lab_image(&b));
        let fast: Vec<u64> = delta_e_image(&a, &la, &b, &lb).map(f64::to_bits).collect();
        let naive: Vec<u64> = la
            .iter()
            .zip(&lb)
            .map(|(x, y)| delta_e00(*x, *y).to_bits())
            .collect();
        assert_eq!(fast, naive);
    }

    fn assert_lab_matches_uncached(img: &Rgba8) {
        let naive: Vec<[u64; 3]> = img
            .data()
            .chunks_exact(4)
            .map(|p| {
                let d = |v: u8| srgb_decode(dequantize(v));
                let l = linear_srgb_to_lab_d50(d(p[0]), d(p[1]), d(p[2]));
                [l.l.to_bits(), l.a.to_bits(), l.b.to_bits()]
            })
            .collect();
        let got: Vec<[u64; 3]> = lab_image(img)
            .iter()
            .map(|l| [l.l.to_bits(), l.a.to_bits(), l.b.to_bits()])
            .collect();
        assert_eq!(got, naive);
    }

    #[test]
    fn lab_image_cache_matches_uncached_bitwise() {
        // one color; a repeated 16-color palette; 4096 distinct colors; seeded LCG noise
        assert_lab_matches_uncached(&palette_image(64, 48, &PALETTE[2..3], |i| i));
        assert_lab_matches_uncached(&palette_image(64, 48, &PALETTE, |i| i * 7));
        let distinct = (0..4096u32)
            .flat_map(|i| [(i & 255) as u8, (i >> 4) as u8, 255 - (i >> 8) as u8, 255])
            .collect();
        assert_lab_matches_uncached(&Rgba8::new(4096, 1, distinct).unwrap());
        let mut seed = 12_345u32;
        let noise = (0..64 * 48 * 4)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 24) as u8
            })
            .collect();
        assert_lab_matches_uncached(&Rgba8::new(64, 48, noise).unwrap());
    }

    /// Stage timings on the 2048×1365 golden photo (Plan 1b-1 Task 2 Step 0). Median of 5.
    #[test]
    #[ignore = "profiling; run with --release -- --ignored --nocapture"]
    fn profile_scoring_stages() {
        use std::collections::HashSet;
        use std::time::Instant;

        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../golden");
        let read = |p: &str| std::fs::read(format!("{root}/{p}")).unwrap();
        let recipe = |p: &str| Recipe::from_json(&String::from_utf8(read(p)).unwrap()).unwrap();
        let median = |label: &str, f: &mut dyn FnMut()| {
            let mut ms: Vec<f64> = (0..5)
                .map(|_| {
                    let t = Instant::now();
                    f();
                    t.elapsed().as_secs_f64() * 1e3
                })
                .collect();
            ms.sort_by(f64::total_cmp);
            println!("{label:<24} {:>8.1} ms", ms[2]);
        };
        let ctx = RenderContext::default();
        let src =
            crate::decode::decode_rgba8(&read("images/lake_2048x1365_baseline420.jpg")).unwrap();
        let answer = render_rgba8(&src, &recipe("recipes/warm_contrast.json"), &ctx).unwrap();
        let player_recipe = recipe("recipes/player_warm_near.json");

        let mut player = None;
        median("render 2048", &mut || {
            player = Some(render_rgba8(&src, &player_recipe, &ctx).unwrap());
        });
        let player = player.unwrap();
        let mut ps = None;
        median("scoring_image", &mut || {
            ps = Some(scoring_image(&player, 1024).unwrap());
        });
        let ps = ps.unwrap();
        let sa = scoring_image(&answer, 1024).unwrap();
        let mut lp = Vec::new();
        median("lab_image", &mut || lp = lab_image(&ps));
        let la = lab_image(&sa);
        let mut de = Vec::new();
        median("delta_e00 every pixel", &mut || {
            de = lp.iter().zip(&la).map(|(p, a)| delta_e00(*p, *a)).collect();
        });
        median("delta_e_image", &mut || {
            de = delta_e_image(&ps, &lp, &sa, &la).collect();
        });
        let mask = FULL.rasterize(ps.width(), ps.height()).unwrap();
        median("error_stats", &mut || {
            error_stats(&de, &mask).unwrap();
        });
        median("ScoringReference::new", &mut || {
            ScoringReference::new(&src, &answer, &FULL).unwrap();
        });
        let colors: HashSet<[u8; 3]> = ps
            .data()
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        let same = ps
            .data()
            .chunks_exact(4)
            .zip(sa.data().chunks_exact(4))
            .filter(|(p, q)| p[..3] == q[..3])
            .count();
        let n = ps.data().len() / 4;
        println!("distinct player colors    {} of {n}", colors.len());
        println!(
            "pixels equal to answer    {same} ({:.1}%)",
            100.0 * same as f64 / n as f64
        );
    }
}
