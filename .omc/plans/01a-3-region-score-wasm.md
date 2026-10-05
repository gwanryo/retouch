# Plan 1a-3: 영역·채점·WASM (Task 8~10)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development(권장) 또는 superpowers:executing-plans. Rust 코드 전에 `rust-best-practices` 로드. 공통 계약·사전 조건·TDD 예외는 `01-engine-foundation.md`에 있다.

**목표:** 평가 영역(`RegionSpec` → 원본 해상도 래스터 → 채점 해상도 매핑), SHA-256, 상대 점수·3중 만점 게이트·채점 매개변수, 챌린지당 1회 생성하는 `ScoringReference`와 Score JSON, WASM API(§3.4), 그리고 골든보다 먼저 네이티브==wasm을 확인하는 스모크와 CI wasm 단계를 만든다.

**선행 조건:** 분할 2(`01a-2-recipe-render.md`) PR이 `main`에 병합되어 있다. 사용하는 것: `scoring_image`, `decode_rgba8`, `encode_png`, `Recipe`, `render_rgba8`, `RenderContext`, `Crop`.

**브랜치 / PR:** `feat/engine-1a-3-region-score-wasm` / "Plan 1a-3: 영역·채점·WASM"

**커버 AC:** S1a, S1b(스냅숏), S1c, S1g(wasm), S1j, S2(필드), S3(부분), E1a(스모크), 마스터 A5·A10·§3.3·§3.4.

**완료 인수 조건:** 분할 1의 완료 명령 전부 + 아래가 성공하고 CI 녹색.
```bash
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
wasm-pack test --node engine/wasm --locked
wasm-pack build engine/wasm --target nodejs --release -- --locked
node --test "engine/wasm/test/*.test.mjs"
```
참고(검증 실행): `engine-core` 단위 테스트 165개, 스모크 1개, wasm 단위 테스트 6개(Sharma 1, 오류 경로 5), node 테스트 6개.

**Review Focus (이 분할):** 경계 정확값의 만점 조건(Task 9a), `score()` 경로에서 조건별 단독 실패(Task 9b), 0 크기·과대 치수·길이 불일치(Task 8, 10, wasm32에서 core 함수를 직접 호출), wasm 반환 객체의 소유권과 `free()`(Task 10).

---

## Task 8: 평가 영역과 해시

**Files:**
- Modify: `engine/core/src/region.rs`, `engine/core/src/hash.rs`

**Interfaces:**
- Consumes: Task 2 `checked_len`, `BufferError`.
- Produces: `enum RegionSpec{Full{}}`(serde `{"kind":"full"}`, 미지 필드 거부), `RegionSpec::from_json`, `RegionSpec::rasterize(w,h)->Result<RegionMask,RegionError>`, `RegionMask::{width,height,len,is_empty,contains(i),count,resample_to(w,h)->Result<RegionMask,_>}`, `enum RegionError{Json, Buffer}`; `sha256_hex(&[u8])->String`.

`Full {}`가 구조체 변형인 이유: serde는 내부 태그 enum의 **유닛** 변형에서 남는 키를 무시해 `deny_unknown_fields`가 동작하지 않는다(확인함).

- [ ] **Step 1: 브랜치와 실패하는 테스트** — `git switch main && git pull && git switch -c feat/engine-1a-3-region-score-wasm`. 두 파일을 Step 3 구현의 `fn` 본문 `todo!()`로 만들고 각각 아래 테스트를 붙인다.

`region.rs` 테스트:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full() {
        assert_eq!(
            RegionSpec::from_json(r#"{"kind":"full"}"#).unwrap(),
            RegionSpec::Full {}
        );
    }

    #[test]
    fn rejects_kinds_not_in_plan_1a() {
        assert!(RegionSpec::from_json(r#"{"kind":"masks","masks":[]}"#).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(RegionSpec::from_json(r#"{"kind":"full","x":1}"#).is_err());
    }

    #[test]
    fn full_contains_every_pixel() {
        assert_eq!(RegionSpec::Full {}.rasterize(5, 3).unwrap().count(), 15);
    }

    #[test]
    fn rasterize_rejects_zero_size() {
        let err = RegionSpec::Full {}.rasterize(0, 3).unwrap_err();
        assert_eq!(
            err,
            RegionError::Buffer(BufferError::Empty {
                width: 0,
                height: 3
            })
        );
    }

    #[test]
    fn rasterize_rejects_overflowing_size_without_allocating() {
        let err = RegionSpec::Full {}
            .rasterize(u32::MAX, u32::MAX)
            .unwrap_err();
        assert!(
            matches!(err, RegionError::Buffer(BufferError::TooLarge { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn resample_maps_by_pixel_center() {
        // 4x1 source with alpha [0, 255, 255, 0] → 2x1: centers fall in source px 1 and 3.
        let src = RegionMask {
            width: 4,
            height: 1,
            alpha: vec![0, 255, 255, 0],
        };
        let out = src.resample_to(2, 1).unwrap();
        assert_eq!(out.alpha, vec![255, 0]);
    }

    #[test]
    fn full_2048x1365_maps_to_full_1024x683() {
        let m = RegionSpec::Full {}.rasterize(2048, 1365).unwrap();
        assert_eq!(m.resample_to(1024, 683).unwrap().count(), 1024 * 683);
    }

    #[test]
    fn contains_is_false_out_of_range() {
        assert!(!RegionSpec::Full {}.rasterize(2, 2).unwrap().contains(4));
    }
}
```

`hash.rs` 테스트:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_empty_input_to_known_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashes_abc_to_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
```

- [ ] **Step 2: 실패 확인**

```bash
cargo test --locked --manifest-path engine/Cargo.toml -p engine-core region::
cargo test --locked --manifest-path engine/Cargo.toml -p engine-core hash::
```
Expected: 둘 다 FAIL.

- [ ] **Step 3: 구현**

`engine/core/src/region.rs`:
```rust
//! Evaluation region E (master plan A5). Callers pass a region *spec* (manifest `region` JSON);
//! this module rasterizes it so native and wasm build the exact same mask.
//!
//! E is built once per challenge in `ScoringReference::new`: rasterize at the original's 2048
//! resolution (where 1b's geometry and range tests look at original pixels), then map to the
//! scoring resolution with [`RegionMask::resample_to`]. Plan 1a: `{"kind":"full"}` only.

use crate::buffer::{checked_len, BufferError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Region specification as stored in the manifest (`challenge.region`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum RegionSpec {
    /// Every pixel. A struct variant so `deny_unknown_fields` rejects `{"kind":"full","x":1}`
    /// (serde ignores extra keys on unit variants of internally tagged enums).
    Full {},
}

/// Region errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegionError {
    /// Bad region JSON (unknown kind, unknown field, syntax).
    #[error("region json: {0}")]
    Json(String),
    /// Mask dimensions out of bounds.
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

/// Rasterized region: one alpha byte per pixel, row-major. A pixel is inside when alpha ≥ 128.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionMask {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

impl RegionMask {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of pixels (`width * height`).
    pub fn len(&self) -> usize {
        self.alpha.len()
    }

    /// Always false: constructors reject empty sizes.
    pub fn is_empty(&self) -> bool {
        self.alpha.is_empty()
    }

    /// Whether pixel index `i` (row-major) is inside E. Out-of-range indices are outside.
    pub fn contains(&self, i: usize) -> bool {
        self.alpha.get(i).is_some_and(|&a| a >= 128)
    }

    /// Number of pixels inside E.
    pub fn count(&self) -> usize {
        self.alpha.iter().filter(|&&a| a >= 128).count()
    }

    /// Maps the mask to `width`×`height` (scoring resolution). Plan 1a rule, part of the score
    /// contract: target pixel (x, y) takes the source alpha at the source pixel containing its
    /// center, `floor((x + 0.5) * src_w / dst_w)`, computed in u64 integer arithmetic.
    pub fn resample_to(&self, width: u32, height: u32) -> Result<RegionMask, RegionError> {
        let n = checked_len(width, height, 1)?;
        let (sw, sh) = (u64::from(self.width), u64::from(self.height));
        let (dw, dh) = (u64::from(width), u64::from(height));
        let mut alpha = Vec::with_capacity(n);
        for y in 0..dh {
            let sy = ((2 * y + 1) * sh / (2 * dh)).min(sh - 1);
            for x in 0..dw {
                let sx = ((2 * x + 1) * sw / (2 * dw)).min(sw - 1);
                alpha.push(self.alpha[(sy * sw + sx) as usize]);
            }
        }
        Ok(RegionMask {
            width,
            height,
            alpha,
        })
    }
}

impl RegionSpec {
    /// Parse a region spec.
    pub fn from_json(json: &str) -> Result<Self, RegionError> {
        serde_json::from_str(json).map_err(|e| RegionError::Json(e.to_string()))
    }

    /// Rasterize at `width`×`height` (the original's resolution).
    pub fn rasterize(&self, width: u32, height: u32) -> Result<RegionMask, RegionError> {
        let n = checked_len(width, height, 1)?;
        match self {
            RegionSpec::Full {} => Ok(RegionMask {
                width,
                height,
                alpha: vec![255; n],
            }),
        }
    }
}
```

`engine/core/src/hash.rs`:
```rust
//! SHA-256, hex-encoded: the golden fingerprint (AC-E1a).

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
```

- [ ] **Step 4: 통과 확인**

```bash
cargo test --locked --manifest-path engine/Cargo.toml -p engine-core region::
cargo test --locked --manifest-path engine/Cargo.toml -p engine-core hash::
```
Expected: 둘 다 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/region.rs engine/core/src/hash.rs
git commit -m "feat(engine): core 래스터화 평가 영역(크기 검사·채점 해상도 매핑)과 SHA-256"
```

---

## Task 9a: 상대 점수·오차 통계·만점 게이트·채점 매개변수

**Files:**
- Create: `engine/core/src/score/gate.rs`
- Modify: `engine/core/src/score/mod.rs`(`pub mod gate;`만)

**Interfaces:**
- Consumes: Task 8 `RegionMask`, `RegionSpec`.
- Produces: 상수 `TILE=64`, `T_TILE=2.0`, `PERFECT_MEAN_DE=1.0`, `BIG_ERROR_DE=5.0`, `BIG_ERROR_MAX_RATIO=0.01`, `MIN_ELIGIBLE_DE=3.0`; `struct ScoringParams`(Serialize, `deny_unknown_fields`), `scoring_params()->ScoringParams`; `enum StatsError{LengthMismatch{de,mask}, EmptyRegion}`; `struct ErrorStats{mean_de, tile_p95_max, big_error_ratio}`; `error_stats(&[f64], &RegionMask)->Result<ErrorStats,StatsError>`; `struct Verdict{eligible, raw_score, perfect, correction_score}`; `verdict(&ErrorStats, d_e_original)->Verdict`.

- [ ] **Step 1: 실패하는 테스트 작성** — `score/mod.rs`를 다음으로 두고:

```rust
//! Scoring (Task 9b).

pub mod gate;
```

`gate.rs`를 Step 3 구현의 `fn` 본문 `todo!()`로 만든 뒤 아래 테스트를 붙인다. 스냅숏 테스트는 매개변수와 `SCORING_VERSION`을 함께 고정한다. 커밋 사이의 미범프 변경은 분할 4의 guard가 잡는다.

```rust
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
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core score::gate` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
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
```

- [ ] **Step 4: 통과 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core score::gate` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/score
git commit -m "feat(engine): 상대 점수·3중 만점 게이트·부적격 판정·채점 매개변수"
```

---

## Task 9b: `ScoringReference`와 Score JSON

**Files:**
- Modify: `engine/core/src/score/mod.rs`

**Interfaces:**
- Consumes: Task 1 Lab·ΔE00, Task 2 `Rgba8`, Task 3 `scoring_image`, Task 5a `Crop`, Task 7 `render_rgba8`(테스트), Task 8 `RegionSpec`, Task 9a 게이트.
- Produces: `ScoringReference::new(&Rgba8 original, &Rgba8 answer, &RegionSpec)`(둘 다 전체 해상도·전체 프레임, 같은 크기), `ScoringReference::{original_scoring, answer_scoring}()->&Rgba8`, `ScoringReference::score(&Rgba8 player, Option<&CompositionInput>)->Result<Score,ScoreError>`; `struct CompositionInput{answer: Option<Crop>, player: Option<Crop>}`; `struct Score`(§3.3 필드·순서), `struct Diagnostics`(모두 `Option<u8>`), `struct ErrorRegion`; `enum ScoreError{SizeMismatch{..}, Region, Stats, Resample, NotYetSupported}`.

- [ ] **Step 1: 실패하는 테스트 작성** — Step 3 구현을 `fn` 본문 `todo!()`로 두고(`pub mod gate;`·`pub use` 유지) 아래 테스트를 붙인다. `perfect_gate_through_score`는 회색 영상으로 ΔE를 조절해 각 조건만 단독으로 실패시키고, 나머지 두 조건이 통과함을 함께 단언한다. `extreme_basic_values_score_to_finite_numbers`는 기본 패널 극값 1024조합을 정답으로, 그 보수 조합을 플레이어로 채점해 Score의 모든 실수가 유한한지 본다(AC-E3c의 채점 쪽. 렌더 쪽은 01a-2 Task 7).

```rust
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
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core score::tests` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
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

fn lab_image(img: &Rgba8) -> Vec<Lab> {
    let lut: Vec<f32> = (0..=255u8).map(|v| srgb_decode(dequantize(v))).collect();
    img.data()
        .chunks_exact(4)
        .map(|p| {
            let c = |i: usize| lut[usize::from(p[i])];
            linear_srgb_to_lab_d50(c(0), c(1), c(2))
        })
        .collect()
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
        let d_e_original = mean_over(pairs().map(|(o, a)| delta_e00(*o, *a)), &mask);
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
        let de: Vec<f64> = lab_p
            .iter()
            .zip(&self.lab_answer)
            .map(|(p, a)| delta_e00(*p, *a))
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
```

- [ ] **Step 4: 통과 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core` / Expected: core 전체 PASS(단위 165개, debug에서 극값 채점 테스트 약 3초).

- [ ] **Step 5: fmt·clippy·린트·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
bash scripts/lint-determinism.sh
git add engine/core/src/score/mod.rs
git commit -m "feat(engine): ScoringReference(로드 시 1회, 2048 입력)와 Score JSON(§3.3)"
```

---

## Task 10: WASM 바인딩, 초기 교차 타깃 스모크, CI wasm 단계

네이티브와 wasm이 같은 문자열을 내는지 골든보다 먼저 확인한다. 여기서 실패하면 분할 4로 넘어가지 않는다.

**Files:**
- Modify: `engine/wasm/src/lib.rs`, `.github/workflows/ci.yml`
- Create: `engine/wasm/tests/color_vectors.rs`, `engine/wasm/tests/core_errors.rs`, `engine/core/tests/smoke.rs`, `engine/wasm/test/smoke.test.mjs`

**Interfaces:**
- Consumes: core 전체.
- Produces (마스터 §3.4): `decode_image(bytes)->ImageData`, `validate_recipe(json)->String`, `render_rgba8(w,h,rgba,recipe_json,seed: u32)->Uint8Array`, `scoring_image(w,h,rgba,long_edge)->ImageData`, `class ScoringReference{constructor(w,h,original_2048,answer_2048,region_json), score(player_2048, crop_json|null)->String, answer_scoring()->ImageData, free()}`, `sha256_hex`, `versions()`(`{"engine","scoring","schema","scoring_params":{…}}`), `memory_bytes()`(성능 진단). 오류는 `JsError`로 throw(메시지 = core 오류의 `Display`).
- 소유권: `ImageData{width,height,rgba}`는 일반 객체가 아니라 Rust 메모리를 소유하는 wasm-bindgen 클래스다. 받은 쪽이 `free()`한다(동기 루프 안에서는 finalizer가 돌지 않으므로 GC에 기대지 않는다). `rgba` 읽기는 매번 새 `Uint8Array` 복사(`getter_with_clone`)이므로 한 번 읽어 보관하고 바로 `free()`한다. `decode_image`·`scoring_image`·`ScoringReference.answer_scoring()`은 호출마다 새 `ImageData`를 돌려준다. `ScoringReference`는 입력 배열을 복사해 두므로 생성 뒤 JS 쪽 배열을 버려도 되고, 챌린지를 떠날 때 `free()`한다. Worker 경계(Plan 2)에서는 `{width,height,rgba}` 일반 객체로 바꿔 넘긴다. 스모크·골든·벤치 JS는 모두 `try/finally`로 해제한다.
- wasm32 오류 경로: `engine/wasm/tests/core_errors.rs`가 `RegionSpec::rasterize`·`RegionMask::resample_to`·`error_stats`·`ScoringReference::new`를 직접 호출한다(wasm32는 `usize`가 32비트). JS 수준 테스트는 `Rgba8` 생성자에서 먼저 멈춰 이 함수들에 닿지 않는다. JS `assert.throws`는 엔진 오류 메시지 정규식을 지정해 wasm trap(`RuntimeError: unreachable`)이 통과하지 못하게 한다.
- 스모크: `cargo test -p engine-core --test smoke`가 `engine/target/smoke/{input.jpg,answer.png,native.json}`을 쓰고 node 테스트가 읽는다. 입력 1100×700 → 채점 1024×652(비정수 배율), 정답은 PNG로 저장 후 다시 디코드해 채점한다.

- [ ] **Step 1: 실패하는 테스트 작성**

`engine/wasm/tests/color_vectors.rs`:
```rust
//! Color math inside wasm32 (`wasm-pack test --node engine/wasm`): Sharma 2005 vectors both ways.
#![cfg(target_arch = "wasm32")]
#![allow(clippy::unwrap_used)]

use engine_core::color::{delta_e00, sharma_2005_rows};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn sharma_vectors_match_within_1e_4_in_both_argument_orders() {
    let rows = sharma_2005_rows();
    assert_eq!(rows.len(), 34);
    for (i, (c1, c2, want)) in rows.into_iter().enumerate() {
        for got in [delta_e00(c1, c2), delta_e00(c2, c1)] {
            assert!(
                (got - want).abs() <= 1e-4,
                "pair {}: want {want} got {got}",
                i + 1
            );
        }
    }
}
```

`engine/wasm/tests/core_errors.rs`(분할 1·3에서 이미 구현된 core를 wasm32에서 다시 확인하는 회귀 테스트라 Step 2에서도 통과한다):
```rust
//! Error paths of region and statistics code inside wasm32 (`wasm-pack test --node engine/wasm`).
//! `usize` is 32-bit here, so size checks must return `Err` before any allocation or index
//! arithmetic can overflow. These call the core functions directly: a JS-level test would stop
//! at the `Rgba8` constructor and never reach them.
#![cfg(target_arch = "wasm32")]
#![allow(clippy::unwrap_used)]

use engine_core::buffer::{BufferError, Rgba8};
use engine_core::region::{RegionError, RegionSpec};
use engine_core::score::{error_stats, ScoreError, ScoringReference, StatsError};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn rasterize_rejects_zero_size() {
    assert_eq!(
        RegionSpec::Full {}.rasterize(0, 3).unwrap_err(),
        RegionError::Buffer(BufferError::Empty {
            width: 0,
            height: 3
        })
    );
}

#[wasm_bindgen_test]
fn rasterize_rejects_overflowing_size() {
    let err = RegionSpec::Full {}
        .rasterize(u32::MAX, u32::MAX)
        .unwrap_err();
    assert!(
        matches!(err, RegionError::Buffer(BufferError::TooLarge { .. })),
        "{err:?}"
    );
}

#[wasm_bindgen_test]
fn resample_to_rejects_zero_and_overflowing_targets() {
    let mask = RegionSpec::Full {}.rasterize(4, 4).unwrap();
    assert_eq!(
        mask.resample_to(0, 1).unwrap_err(),
        RegionError::Buffer(BufferError::Empty {
            width: 0,
            height: 1
        })
    );
    let err = mask.resample_to(u32::MAX, u32::MAX).unwrap_err();
    assert!(
        matches!(err, RegionError::Buffer(BufferError::TooLarge { .. })),
        "{err:?}"
    );
}

#[wasm_bindgen_test]
fn error_stats_rejects_length_mismatch() {
    let mask = RegionSpec::Full {}.rasterize(2, 2).unwrap();
    assert_eq!(
        error_stats(&[0.0; 3], &mask).unwrap_err(),
        StatsError::LengthMismatch { de: 3, mask: 4 }
    );
}

#[wasm_bindgen_test]
fn scoring_reference_rejects_answer_size_mismatch() {
    let a = Rgba8::new(2, 2, vec![128; 16]).unwrap();
    let b = Rgba8::new(2, 1, vec![128; 8]).unwrap();
    let err = ScoringReference::new(&a, &b, &RegionSpec::Full {}).unwrap_err();
    assert!(
        matches!(err, ScoreError::SizeMismatch { got_h: 1, .. }),
        "{err:?}"
    );
}
```

`engine/core/tests/smoke.rs`:
```rust
//! Early cross-target smoke test (plan 01a-3 Task 10). Runs the reference path natively:
//! JPEG decode → render → answer PNG round trip → `ScoringReference` → score, and writes the
//! inputs plus the native results to `engine/target/smoke/`. `engine/wasm/test/smoke.test.mjs`
//! repeats the path in wasm and must produce identical strings.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use engine_core::buffer::Rgba8;
use engine_core::decode::{decode_rgba8, encode_png};
use engine_core::hash::sha256_hex;
use engine_core::recipe::Recipe;
use engine_core::region::RegionSpec;
use engine_core::render::{render_rgba8, RenderContext};
use engine_core::score::ScoringReference;
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

const ANSWER: &str = r#"{"schema_version":1,"basic":{"exposure":0.6,"contrast":20,"temperature":25,"saturation":10}}"#;
const PLAYER: &str = r#"{"schema_version":1,"basic":{"exposure":0.3,"contrast":10}}"#;
const SEED: u32 = 7;
// Non-integer downscale to the 1024 scoring size: 1100x700 → 1024x652.
const W: u16 = 1100;
const H: u16 = 700;

fn synthetic_jpeg() -> Vec<u8> {
    let mut rgb = Vec::with_capacity(usize::from(W) * usize::from(H) * 3);
    for y in 0..H {
        for x in 0..W {
            let r = (u32::from(x) * 255 / u32::from(W - 1)) as u8;
            let g = (u32::from(y) * 255 / u32::from(H - 1)) as u8;
            let b = if (x / 25 + y / 25) % 2 == 0 { 200 } else { 40 };
            rgb.extend_from_slice(&[r, g, b]);
        }
    }
    let mut out = Vec::new();
    let mut enc = Encoder::new(&mut out, 90);
    enc.set_sampling_factor(SamplingFactor::R_4_2_0);
    enc.encode(&rgb, W, H, ColorType::Rgb).unwrap();
    out
}

fn render(src: &Rgba8, json: &str) -> Rgba8 {
    let r = Recipe::from_json(json).unwrap();
    render_rgba8(src, &r, &RenderContext { seed: SEED }).unwrap()
}

#[test]
fn native_reference_path_writes_smoke_expectations() {
    let jpeg = synthetic_jpeg();
    let original = decode_rgba8(&jpeg).unwrap();
    let answer_png = encode_png(&render(&original, ANSWER)).unwrap();
    // Score from the saved PNG, exactly as the game will (A4: PNG reproduces the input).
    let answer = decode_rgba8(&answer_png).unwrap();
    let player = render(&original, PLAYER);
    let reference = ScoringReference::new(&original, &answer, &RegionSpec::Full {}).unwrap();
    let s = reference.score(&player, None).unwrap();
    assert!((1..=99).contains(&s.correction_score), "{s:?}");
    let scoring = reference.answer_scoring();
    assert_eq!((scoring.width(), scoring.height()), (1024, 652));

    let expected = serde_json::json!({
        "answer_recipe": ANSWER,
        "player_recipe": PLAYER,
        "seed": SEED,
        "original_sha256": sha256_hex(original.data()),
        "answer_sha256": sha256_hex(answer.data()),
        "player_sha256": sha256_hex(player.data()),
        "answer_scoring_sha256": sha256_hex(scoring.data()),
        "score_json": serde_json::to_string(&s).unwrap(),
    });
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/smoke");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.jpg"), &jpeg).unwrap();
    fs::write(dir.join("answer.png"), &answer_png).unwrap();
    fs::write(dir.join("native.json"), expected.to_string()).unwrap();
}
```

`engine/wasm/test/smoke.test.mjs`:
```js
// Early cross-target smoke (plan 01a-3 Task 10): the wasm build must reproduce the native
// results written by `cargo test -p engine-core --test smoke`. Run that first, then:
//   node --test "engine/wasm/test/*.test.mjs"
// Every `ImageData` and `ScoringReference` owns wasm memory: read `.rgba` once (each read
// copies) and `free()` in `finally`.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const smokeDir = resolve(here, '../../target/smoke')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (name) => new Uint8Array(readFileSync(resolve(smokeDir, name)))

/** Decode with the engine, copy the pixels out once and release the wasm object. */
const decode = (bytes) => {
  const img = wasm.decode_image(bytes)
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

test('wasm reference path equals native smoke output', () => {
  const nativePath = resolve(smokeDir, 'native.json')
  assert.ok(existsSync(nativePath), `${nativePath} missing: run cargo test -p engine-core --test smoke first`)
  const native = JSON.parse(readFileSync(nativePath, 'utf8'))
  const original = decode(read('input.jpg'))
  assert.equal(wasm.sha256_hex(original.rgba), native.original_sha256, 'jpeg decode')

  const { width: w, height: h } = original
  const rendered = wasm.render_rgba8(w, h, original.rgba, native.answer_recipe, native.seed)
  assert.equal(wasm.sha256_hex(rendered), native.answer_sha256, 'render')
  const answer = decode(read('answer.png'))
  assert.equal(wasm.sha256_hex(answer.rgba), native.answer_sha256, 'answer PNG decode')
  const player = wasm.render_rgba8(w, h, original.rgba, native.player_recipe, native.seed)
  assert.equal(wasm.sha256_hex(player), native.player_sha256, 'player render')

  const reference = new wasm.ScoringReference(w, h, original.rgba, answer.rgba, '{"kind":"full"}')
  try {
    const scoring = reference.answer_scoring()
    try {
      assert.equal(wasm.sha256_hex(scoring.rgba), native.answer_scoring_sha256, 'scoring image')
    } finally {
      scoring.free()
    }
    assert.equal(reference.score(player, null), native.score_json, 'score JSON string')
  } finally {
    reference.free()
  }
})

test('wasm rejects zero, mismatched and oversized dimensions with the engine error', () => {
  // The patterns are the engine's own messages, so a wasm trap (RuntimeError) cannot pass.
  assert.throws(() => wasm.scoring_image(0, 4, new Uint8Array(0), 1024), /^Error: empty image: 0x4$/)
  assert.throws(() => wasm.scoring_image(2, 2, new Uint8Array(15), 1024), /^Error: rgba buffer has 15 bytes, expected 16 for 2x2$/)
  assert.throws(() => wasm.render_rgba8(100000, 100000, new Uint8Array(4), '{"schema_version":1}', 0), /^Error: image too large: 100000x100000 /)
  assert.throws(() => new wasm.ScoringReference(0, 0, new Uint8Array(0), new Uint8Array(0), '{"kind":"full"}'), /^Error: empty image: 0x0$/)
  assert.throws(() => new wasm.ScoringReference(1, 1, new Uint8Array(4), new Uint8Array(4), '{"kind":"masks"}'), /^Error: region json: /)
  assert.throws(() => wasm.render_rgba8(1, 1, new Uint8Array(4), '{"schema_version":1,"basic":{"exposure":1e39}}', 0), /^Error: field `basic.exposure` is not a finite number$/)
})

test('ScoringReference rejects a wrong-size player and composition boxes in Plan 1a', () => {
  const px = new Uint8Array(4 * 4 * 4).fill(128)
  const reference = new wasm.ScoringReference(4, 4, px, px, '{"kind":"full"}')
  try {
    assert.throws(() => reference.score(new Uint8Array(4 * 4 * 3), null), /^Error: rgba buffer has 48 bytes, expected 64 for 4x4$/)
    assert.throws(() => reference.score(px, '{"answer":{"x":0,"y":0,"w":1,"h":1},"player":null}'), /^Error: `composition` is scored from Plan 1b$/)
    assert.equal(JSON.parse(reference.score(px, '{"answer":null,"player":null}')).eligible, false)
  } finally {
    reference.free()
  }
})

test('wasm handles 1x1, 1xN and Nx1 images', () => {
  for (const [w, h] of [[1, 1], [1, 9], [9, 1]]) {
    const rgba = new Uint8Array(w * h * 4).fill(128)
    const out = wasm.render_rgba8(w, h, rgba, '{"schema_version":1,"basic":{"exposure":0.5}}', 0)
    assert.equal(out.length, w * h * 4)
    const small = wasm.scoring_image(w, h, out, 1024)
    try {
      assert.deepEqual([small.width, small.height], [w, h])
    } finally {
      small.free()
    }
  }
})

test('ImageData owns wasm memory: rgba reads copy, free() releases it', () => {
  const img = wasm.scoring_image(2, 2, new Uint8Array(16).fill(9), 1024)
  assert.ok(img instanceof wasm.ImageData)
  assert.notStrictEqual(img.rgba, img.rgba, 'each rgba read is a fresh copy')
  img.free()
  assert.throws(() => img.width, /null pointer passed to rust/)
})

test('validate_recipe reports clamped fields with mask-indexed paths', () => {
  const json = '{"schema_version":1,"basic":{"exposure":9},"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5,"adjust":{"tint":-300}}]}'
  const report = JSON.parse(wasm.validate_recipe(json))
  assert.deepEqual(report.normalizations, [
    { path: 'basic.exposure', from: 9, to: 5, reason: 'clamped' },
    { path: 'masks[0].adjust.tint', from: -300, to: -100, reason: 'clamped' },
  ])
  assert.equal(report.recipe.basic.exposure, 5)
})
```

- [ ] **Step 2: 실패 확인(동작 RED)**: `engine/wasm/src/lib.rs`를 Step 3 구현의 스텁으로 둔다. 구조체(`ImageData`, `ScoringReference`)·`From` impl·`#[wasm_bindgen]` 속성·시그니처는 Step 3 그대로, 모든 `fn` 본문(`rgba8`, `ScoringReference::{new, score, answer_scoring}` 포함)만 `todo!()`. 패키지가 없어서가 아니라 바인딩 동작이 없어서 실패해야 한다.

```bash
cargo test --locked --manifest-path engine/Cargo.toml -p engine-core --test smoke
wasm-pack build engine/wasm --target nodejs --release -- --locked
node --test "engine/wasm/test/*.test.mjs"
wasm-pack test --node engine/wasm --locked
```
Expected: 네이티브 스모크 PASS(`native.json` 생성, debug에서 약 15~20초). 스텁 빌드는 미사용 import·변수 경고만 내고 성공. node는 6개 모두 FAIL이며 원인은 `RuntimeError: unreachable`(스텁 `todo!()` 패닉)이다. 오류 검사 테스트도 FAIL이다: 정규식이 엔진 메시지를 요구하므로 trap은 통과하지 못한다(확인함). `wasm-pack test`의 Sharma·오류 경로 6개는 core를 직접 부르므로 PASS(회귀 테스트).

- [ ] **Step 3: 구현** — `engine/wasm/src/lib.rs`:

```rust
//! wasm-bindgen surface (master plan §3.4). Thin: validate arguments, call `engine-core`,
//! return bytes or JSON. No math lives here.

use engine_core::buffer::Rgba8;
use engine_core::recipe::Recipe;
use engine_core::region::RegionSpec;
use engine_core::render::{self, RenderContext};
use engine_core::score;
use wasm_bindgen::prelude::*;

/// Pixels handed to JS (master plan §3.4). A wasm-bindgen class that owns Rust memory: the
/// caller must call `free()` when done, because finalizers never run inside a synchronous loop.
/// Every `rgba` read copies the bytes into a fresh `Uint8Array` (`getter_with_clone`), so read
/// it once, keep the copy, and free the `ImageData` right away.
#[wasm_bindgen(getter_with_clone)]
pub struct ImageData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 bytes, row-major.
    pub rgba: Vec<u8>,
}

impl From<Rgba8> for ImageData {
    fn from(img: Rgba8) -> Self {
        Self {
            width: img.width(),
            height: img.height(),
            rgba: img.into_data(),
        }
    }
}

fn rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Rgba8, JsError> {
    Ok(Rgba8::new(width, height, rgba.to_vec())?)
}

/// Decode JPEG/PNG with the reference decoder (never the browser's). The caller owns the
/// returned `ImageData` and must `free()` it.
#[wasm_bindgen]
pub fn decode_image(bytes: &[u8]) -> Result<ImageData, JsError> {
    Ok(engine_core::decode::decode_rgba8(bytes)?.into())
}

/// `{"recipe":{…clamped…},"normalizations":[{path,from,to,reason}]}`, or throws (AC-E3d).
#[wasm_bindgen]
pub fn validate_recipe(recipe_json: &str) -> Result<String, JsError> {
    Ok(serde_json::to_string(&Recipe::from_json(recipe_json)?)?)
}

/// Full-resolution render with its single final quantization (master plan A3 point ①).
/// Always full frame: output size = input size, `crop` is never applied (A10).
#[wasm_bindgen]
pub fn render_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
    recipe_json: &str,
    seed: u32,
) -> Result<Vec<u8>, JsError> {
    let src = rgba8(width, height, rgba)?;
    let recipe = Recipe::from_json(recipe_json)?;
    Ok(render::render_rgba8(&src, &recipe, &RenderContext { seed })?.into_data())
}

/// THE scoring-image function (master plan A4, A3 point ②). The caller owns the returned
/// `ImageData` and must `free()` it.
#[wasm_bindgen]
pub fn scoring_image(
    width: u32,
    height: u32,
    rgba: &[u8],
    long_edge: u32,
) -> Result<ImageData, JsError> {
    Ok(engine_core::scoring_image(&rgba8(width, height, rgba)?, long_edge)?.into())
}

/// Per-challenge scoring state (master plan A5): build once when the challenge loads, then
/// score each full-frame 2048 player render. The input arrays are copied in, not kept. Call
/// `free()` when leaving the challenge.
#[wasm_bindgen]
pub struct ScoringReference {
    width: u32,
    height: u32,
    inner: score::ScoringReference,
}

#[wasm_bindgen]
impl ScoringReference {
    /// `original_2048`/`answer_2048` are full-frame RGBA8 of `width`×`height`;
    /// `region_json` is the manifest `region`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        width: u32,
        height: u32,
        original_2048: &[u8],
        answer_2048: &[u8],
        region_json: &str,
    ) -> Result<ScoringReference, JsError> {
        let region = RegionSpec::from_json(region_json)?;
        let inner = score::ScoringReference::new(
            &rgba8(width, height, original_2048)?,
            &rgba8(width, height, answer_2048)?,
            &region,
        )?;
        Ok(Self {
            width,
            height,
            inner,
        })
    }

    /// Score JSON for one player render. `crop_json` is `null` in Plan 1a (composition: 1b).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "wasm-bindgen cannot take Option<&str>"
    )]
    pub fn score(&self, player_2048: &[u8], crop_json: Option<String>) -> Result<String, JsError> {
        let composition: Option<score::CompositionInput> =
            crop_json.as_deref().map(serde_json::from_str).transpose()?;
        let player = rgba8(self.width, self.height, player_2048)?;
        let s = self.inner.score(&player, composition.as_ref())?;
        Ok(serde_json::to_string(&s)?)
    }

    /// The answer's scoring image (hash = manifest `answer.sha256_rgba8_1024`). Returns a new
    /// `ImageData` copy on every call; the caller owns it and must `free()` it.
    pub fn answer_scoring(&self) -> ImageData {
        self.inner.answer_scoring().clone().into()
    }
}

/// Lower-case hex SHA-256.
#[wasm_bindgen]
pub fn sha256_hex(bytes: &[u8]) -> String {
    engine_core::hash::sha256_hex(bytes)
}

/// `{"engine":…,"scoring":…,"schema":1,"scoring_params":{…}}`.
#[wasm_bindgen]
pub fn versions() -> String {
    serde_json::json!({
        "engine": engine_core::ENGINE_VERSION,
        "scoring": engine_core::SCORING_VERSION,
        "schema": engine_core::SCHEMA_VERSION,
        "scoring_params": engine_core::score::scoring_params(),
    })
    .to_string()
}

/// Size of the wasm linear memory in bytes. Linear memory only grows, so this is the peak
/// (perf baseline diagnostics, not part of the scoring contract).
#[wasm_bindgen]
pub fn memory_bytes() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        (core::arch::wasm32::memory_size::<0>() * 65_536) as f64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}
```

- [ ] **Step 4: 통과 확인**

```bash
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
wasm-pack test --node engine/wasm --locked
wasm-pack build engine/wasm --target nodejs --release -- --locked
node --test "engine/wasm/test/*.test.mjs"
```
Expected: `sharma_vectors_match_within_1e_4_in_both_argument_orders ... ok`와 `core_errors` 5개 `ok`, node 6개 `✔`. 참고값(검증 실행): 스모크 채점 크기 1024×652. 첫 node 테스트가 실패하면 **결정성 위반**이다. `native.json`을 고치지 말고 `bash scripts/lint-determinism.sh`와 `cargo tree --locked --manifest-path engine/Cargo.toml -e features,normal -i zune-jpeg`부터 본다.

`wasm-pack test`의 `--locked` 위치: wasm-pack 0.15.0은 크레이트 경로 뒤 인자를 `cargo test`에 그대로 넘기고(`src/test/mod.rs` `cmd.args(extra_options)`), `--` 뒤 인자는 테스트 러너에 넘긴다(소스·실행 확인). 그래서 `wasm-pack test --node engine/wasm --locked`이고, `wasm-pack build`는 `-- --locked`다.

- [ ] **Step 5: CI wasm 단계 추가** — `.github/workflows/ci.yml` engine 잡의 `      # >>> wasm steps (Plan 1a-3 Task 10)` 줄을 아래 단계로 바꾼다(setup-node가 wasm-pack 단계보다 앞).

```yaml
      - name: Clippy (wasm32)
        run: cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
      - uses: actions/setup-node@v4
        with:
          node-version: 24
      - name: Install wasm-pack 0.15.0
        uses: jetli/wasm-pack-action@v0.4.0
        with:
          version: v0.15.0
      # wasm-pack 0.15.0 passes arguments after the crate path straight to `cargo test`
      # (src/test/mod.rs `cmd.args(extra_options)`); arguments after `--` would reach the test
      # runner instead.
      - name: Wasm unit tests (Sharma vectors inside wasm32)
        run: wasm-pack test --node engine/wasm --locked
      - name: Build wasm (nodejs, release)
        run: wasm-pack build engine/wasm --target nodejs --release -- --locked
      - name: Test (wasm == native)
        run: node --test "engine/wasm/test/*.test.mjs"
```

- [ ] **Step 6: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml --workspace --all-targets -- -D warnings
git add engine/wasm/src engine/wasm/tests engine/wasm/test/smoke.test.mjs engine/core/tests/smoke.rs .github/workflows/ci.yml
git commit -m "feat(engine): wasm 바인딩(§3.4, ScoringReference 클래스)과 네이티브==wasm 스모크, CI wasm 단계"
```

---

## 분할 마무리: 인수 확인과 PR

- [ ] **Step 1: 완료 인수 조건 실행** — 분할 1의 명령 6개와 이 문서 머리의 명령 4개. Expected: 모두 성공.

- [ ] **Step 2: 푸시·PR·필수 체크 대기**

```bash
git push -u origin feat/engine-1a-3-region-score-wasm
gh pr create --base main --head feat/engine-1a-3-region-score-wasm --title "Plan 1a-3: 영역·채점·WASM" --body "Plan 1a-3(.omc/plans/01a-3-region-score-wasm.md) Task 8~10. 평가 영역, 만점 게이트, ScoringReference, WASM API, 네이티브==wasm 스모크."
gh pr checks --watch --required
```
Expected: 필수 체크 전부 pass. 병합은 사용자 확인 후.
