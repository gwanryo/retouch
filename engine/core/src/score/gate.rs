//! Relative score, error statistics and the perfect-score gate (AC-S1a, S1c, S1j). Pure
//! functions over ΔE arrays so every boundary can be tested with exact numbers.

use crate::region::RegionMask;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tile edge in scoring-image pixels for the per-tile p95 gate (AC-S1a).
pub const TILE: u32 = 64;
/// Max per-tile p95 ΔE00 for a perfect score. Placeholder until the AC-S4 bench (Plan 3);
/// any change requires a `SCORING_VERSION` bump (AC-S1b).
pub const T_TILE: f64 = 2.0;
/// Mean ΔE00 at or below which the relative score is 100 (JND).
pub const PERFECT_MEAN_DE: f64 = 1.0;
/// Pixels with ΔE00 strictly above this are "big errors".
pub const BIG_ERROR_DE: f64 = 5.0;
/// Max fraction of big-error pixels for a perfect score (inclusive).
pub const BIG_ERROR_MAX_RATIO: f64 = 0.01;
/// Challenges with original↔answer mean ΔE00 below this are ineligible (AC-S1j).
pub const MIN_ELIGIBLE_DE: f64 = 3.0;

/// Every scoring parameter (AC-S1b). Serialized into golden `expected.json` and `versions()`;
/// the golden guard fails when it changes while `SCORING_VERSION` does not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringParams {
    /// [`TILE`].
    pub tile: u32,
    /// [`T_TILE`].
    pub t_tile: f64,
    /// [`PERFECT_MEAN_DE`].
    pub perfect_mean_de: f64,
    /// [`BIG_ERROR_DE`].
    pub big_error_de: f64,
    /// [`BIG_ERROR_MAX_RATIO`].
    pub big_error_max_ratio: f64,
    /// [`MIN_ELIGIBLE_DE`].
    pub min_eligible_de: f64,
    /// [`crate::SCORING_LONG_EDGE`].
    pub long_edge: u32,
    /// Percentile rule name.
    pub p95: String,
    /// Region-to-scoring mask mapping rule name.
    pub region_resample: String,
}

/// The parameters this build scores with.
pub fn scoring_params() -> ScoringParams {
    ScoringParams {
        tile: TILE,
        t_tile: T_TILE,
        perfect_mean_de: PERFECT_MEAN_DE,
        big_error_de: BIG_ERROR_DE,
        big_error_max_ratio: BIG_ERROR_MAX_RATIO,
        min_eligible_de: MIN_ELIGIBLE_DE,
        long_edge: crate::SCORING_LONG_EDGE,
        p95: "nearest_rank_ceil".to_owned(),
        region_resample: "source_pixel_at_center".to_owned(),
    }
}

/// Statistics failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatsError {
    /// ΔE array and mask disagree in length.
    #[error("{de} ΔE values for a {mask}-pixel region mask")]
    LengthMismatch {
        /// ΔE values given.
        de: usize,
        /// Mask pixels.
        mask: usize,
    },
    /// E contains no pixels.
    #[error("evaluation region is empty")]
    EmptyRegion,
}

/// Error statistics over the region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ErrorStats {
    /// Mean ΔE00 over E.
    pub mean_de: f64,
    /// Max over 64×64 tiles (partial edge tiles included) of the tile's p95 ΔE00 over E.
    pub tile_p95_max: f64,
    /// Fraction of E with ΔE00 > [`BIG_ERROR_DE`].
    pub big_error_ratio: f64,
}

/// p95 by nearest rank: sort ascending, take index `ceil(0.95 * n) - 1`.
fn p95(vals: &mut [f64]) -> f64 {
    vals.sort_by(f64::total_cmp);
    let idx = ((0.95 * vals.len() as f64).ceil() as usize).max(1) - 1;
    vals[idx]
}

/// Mean, tile p95 max and big-error ratio of `de` (row-major, `mask` dimensions) over E.
pub fn error_stats(de: &[f64], mask: &RegionMask) -> Result<ErrorStats, StatsError> {
    if de.len() != mask.len() {
        return Err(StatsError::LengthMismatch {
            de: de.len(),
            mask: mask.len(),
        });
    }
    let (w, h) = (mask.width(), mask.height());
    let mut sum = 0.0;
    let mut n = 0usize;
    let mut big = 0usize;
    for (i, &d) in de.iter().enumerate() {
        if mask.contains(i) {
            sum += d;
            n += 1;
            if d > BIG_ERROR_DE {
                big += 1;
            }
        }
    }
    if n == 0 {
        return Err(StatsError::EmptyRegion);
    }
    let mut worst = 0.0f64;
    let mut vals = Vec::with_capacity((TILE * TILE) as usize);
    for ty in (0..h).step_by(TILE as usize) {
        for tx in (0..w).step_by(TILE as usize) {
            vals.clear();
            for y in ty..(ty + TILE).min(h) {
                for x in tx..(tx + TILE).min(w) {
                    let i = y as usize * w as usize + x as usize;
                    if mask.contains(i) {
                        vals.push(de[i]);
                    }
                }
            }
            if !vals.is_empty() {
                worst = worst.max(p95(&mut vals));
            }
        }
    }
    Ok(ErrorStats {
        mean_de: sum / n as f64,
        tile_p95_max: worst,
        big_error_ratio: big as f64 / n as f64,
    })
}

/// Outcome of the gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    /// `D_E >= 3`.
    pub eligible: bool,
    /// `clamp(100 * (1 - (d - 1) / (D - 1)), 0, 100)`; 0 when ineligible.
    pub raw_score: f64,
    /// All three perfect conditions hold (and eligible).
    pub perfect: bool,
    /// 100 if perfect, else `round_half_up(min(raw, 99))`; 0 when ineligible.
    pub correction_score: u8,
}

fn round_half_up(x: f64) -> u8 {
    (x.clamp(0.0, 100.0) + 0.5).floor() as u8
}

/// Applies AC-S1j (relative score), AC-S1a (perfect gate) and AC-S1c (non-perfect ≤ 99).
pub fn verdict(stats: &ErrorStats, d_e_original: f64) -> Verdict {
    if d_e_original < MIN_ELIGIBLE_DE {
        return Verdict {
            eligible: false,
            raw_score: 0.0,
            perfect: false,
            correction_score: 0,
        };
    }
    let raw = (100.0
        * (1.0 - (stats.mean_de - PERFECT_MEAN_DE) / (d_e_original - PERFECT_MEAN_DE)))
        .clamp(0.0, 100.0);
    let perfect = stats.mean_de <= PERFECT_MEAN_DE
        && stats.tile_p95_max <= T_TILE
        && stats.big_error_ratio <= BIG_ERROR_MAX_RATIO;
    Verdict {
        eligible: true,
        raw_score: raw,
        perfect,
        correction_score: if perfect {
            100
        } else {
            round_half_up(raw.min(99.0))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::RegionSpec;

    const EPS: f64 = 1e-9;

    fn stats(mean_de: f64, tile_p95_max: f64, big_error_ratio: f64) -> ErrorStats {
        ErrorStats {
            mean_de,
            tile_p95_max,
            big_error_ratio,
        }
    }

    fn ok() -> ErrorStats {
        stats(0.5, 1.0, 0.0)
    }

    #[test]
    fn scoring_params_snapshot_matches_scoring_version() {
        // Update both halves together, and only together with a SCORING_VERSION bump.
        let snapshot = r#"["scoring-0.1.0",{"tile":64,"t_tile":2.0,"perfect_mean_de":1.0,"big_error_de":5.0,"big_error_max_ratio":0.01,"min_eligible_de":3.0,"long_edge":1024,"p95":"nearest_rank_ceil","region_resample":"source_pixel_at_center"}]"#;
        let now = serde_json::to_string(&(crate::SCORING_VERSION, scoring_params())).unwrap();
        assert_eq!(now, snapshot);
    }

    mod relative_score {
        use super::*;

        #[test]
        fn submitting_original_distance_scores_zero() {
            assert_eq!(verdict(&stats(6.0, 9.0, 0.5), 6.0).correction_score, 0);
        }

        #[test]
        fn halfway_scores_50() {
            assert!((verdict(&stats(3.5, 9.0, 0.5), 6.0).raw_score - 50.0).abs() < 1e-9);
        }

        #[test]
        fn worse_than_original_clamps_to_zero() {
            assert_eq!(verdict(&stats(9.0, 9.0, 0.5), 6.0).raw_score, 0.0);
        }
    }

    mod perfect_gate {
        use super::*;

        #[test]
        fn all_conditions_at_their_limits_are_perfect() {
            let v = verdict(&stats(PERFECT_MEAN_DE, T_TILE, BIG_ERROR_MAX_RATIO), 6.0);
            assert!(v.perfect && v.correction_score == 100, "{v:?}");
        }

        #[test]
        fn mean_just_above_one_alone_fails() {
            let v = verdict(&stats(1.0 + EPS, 1.0, 0.0), 6.0);
            assert!(!v.perfect && v.correction_score == 99, "{v:?}");
        }

        #[test]
        fn tile_just_above_t_tile_alone_fails_with_raw_100() {
            let v = verdict(&stats(0.5, T_TILE + EPS, 0.0), 6.0);
            assert_eq!(
                (v.raw_score, v.perfect, v.correction_score),
                (100.0, false, 99)
            );
        }

        #[test]
        fn big_ratio_just_above_one_percent_alone_fails() {
            let v = verdict(&stats(0.5, 1.0, BIG_ERROR_MAX_RATIO + EPS), 6.0);
            assert!(!v.perfect && v.correction_score == 99, "{v:?}");
        }

        #[test]
        fn raw_99_6_without_perfect_rounds_to_99_not_100() {
            // d chosen so raw = 99.6: (d - 1) / 5 = 0.004.
            let v = verdict(&stats(1.02, 1.0, 0.0), 6.0);
            assert!(
                (v.raw_score - 99.6).abs() < 1e-9 && v.correction_score == 99,
                "{v:?}"
            );
        }
    }

    mod eligibility {
        use super::*;

        #[test]
        fn d_e_exactly_3_is_eligible() {
            assert!(verdict(&ok(), 3.0).eligible);
        }

        #[test]
        fn d_e_just_below_3_is_ineligible_and_scores_zero() {
            let v = verdict(&ok(), 3.0 - EPS);
            assert_eq!(
                (v.eligible, v.perfect, v.correction_score),
                (false, false, 0)
            );
        }

        #[test]
        fn d_e_just_above_3_is_eligible() {
            assert!(verdict(&ok(), 3.0 + EPS).eligible);
        }
    }

    mod error_stats {
        use super::*;

        fn full(w: u32, h: u32) -> RegionMask {
            RegionSpec::Full {}.rasterize(w, h).unwrap()
        }

        #[test]
        fn rejects_length_mismatch_instead_of_panicking() {
            let err = error_stats(&[0.0; 3], &full(2, 2)).unwrap_err();
            assert_eq!(err, StatsError::LengthMismatch { de: 3, mask: 4 });
        }

        #[test]
        fn small_bad_patch_hidden_by_the_mean_is_caught_by_the_tile() {
            let (w, h) = (128u32, 128u32);
            let mut de = vec![0.2f64; (w * h) as usize];
            for y in 0..16 {
                for x in 0..16 {
                    de[y * w as usize + x] = 40.0; // 256 px = 6.25% of a 64x64 tile > 5%
                }
            }
            let s = error_stats(&de, &full(w, h)).unwrap();
            assert!(s.mean_de < 1.0 && s.tile_p95_max == 40.0, "{s:?}");
        }

        #[test]
        fn only_the_last_partial_tile_is_bad() {
            let (w, h) = (70u32, 70u32);
            let mut de = vec![0.1f64; (w * h) as usize];
            for y in 64..70 {
                for x in 64..70 {
                    de[y * w as usize + x] = 3.0;
                }
            }
            assert_eq!(error_stats(&de, &full(w, h)).unwrap().tile_p95_max, 3.0);
        }

        #[test]
        fn exactly_5_is_not_a_big_error_but_just_above_is() {
            let de = [BIG_ERROR_DE, BIG_ERROR_DE + EPS, 0.0, 0.0];
            let s = error_stats(&de, &full(2, 2)).unwrap();
            assert_eq!(s.big_error_ratio, 0.25);
        }

        #[test]
        fn p95_uses_nearest_rank_ceil() {
            // n = 20 → index ceil(19) - 1 = 18 → the 19th smallest value.
            let de: Vec<f64> = (1..=20).map(f64::from).collect();
            assert_eq!(error_stats(&de, &full(20, 1)).unwrap().tile_p95_max, 19.0);
        }
    }
}
