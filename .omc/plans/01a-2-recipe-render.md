# Plan 1a-2: 레시피·렌더 (Task 5a~7)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development(권장) 또는 superpowers:executing-plans. Rust 코드 전에 `rust-best-practices` 로드. 공통 계약·사전 조건·TDD 예외는 `01-engine-foundation.md`에 있다.

**목표:** 레시피 스키마 v1 전체(마스터 §3.1)와 검증·정규화(`Normalized`), 기본 패널 연산, 렌더 파이프라인(비클램프, 전체 프레임, seed 컨텍스트, 1b 기능 명시 거부)을 만든다.

**선행 조건:** 분할 1(`01a-1-numeric-image.md`) PR이 `main`에 병합되어 있다. 사용하는 것: `buffer::{Rgba8, ImageF32}`, `color::{srgb_decode, srgb_encode}`.

**브랜치 / PR:** `feat/engine-1a-2-recipe-render` / "Plan 1a-2: 레시피·렌더"

**커버 AC:** E1e, E3c(부분), E3d(부분), 마스터 A3·A10, §3.1.

**완료 인수 조건:** 분할 1의 완료 명령 전부 성공, CI 녹색. 참고(검증 실행): 이 분할 끝의 `engine-core` 단위 테스트는 122개.

**Review Focus (이 분할):** 노출로 1을 넘은 하이라이트 복구(Task 7), 과대 범위 픽셀의 생동감 색 반전(Task 6·7), `1e39`·`null`·잘못된 타입·미지 필드(Task 5a·5b), 퇴화 마스크(Task 5c).

---

## Task 5a: 레시피 스키마 v1과 파싱

**Files:**
- Create: `engine/core/src/recipe/schema.rs`, `engine/core/src/recipe/validate.rs`
- Modify: `engine/core/src/recipe/mod.rs`

**Interfaces:**
- Produces: `Recipe{schema_version, basic, hsl, tone_curve, color_grade, split_toning, detail, masks, crop}`, `Basic`(13필드), `Hsl`, `ToneCurve`, `identity_curve()`, `GradeZone`, `ColorGrade`, `SplitToning`, `Detail`, `Adjust`, `RangeChannel`, `Mask{Linear,Radial,Range}`, `Crop`(`Crop::FULL`); `enum RecipeError`, `struct Normalization{path,from,to,reason}`, `struct Normalized`(필드 비공개, `recipe()`, `normalizations()`, `Serialize` → `{"recipe","normalizations"}`), `Recipe::from_json(&str)`, `Recipe::normalize(self)`.

- [ ] **Step 1: 브랜치와 스키마** — `git switch main && git pull && git switch -c feat/engine-1a-2-recipe-render`. 스키마는 타입만 있는 파일이라 테스트는 Step 3의 파싱 테스트가 대신한다.

`engine/core/src/recipe/schema.rs`:
```rust
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
```

`engine/core/src/recipe/validate.rs` (5b·5c에서 채운다):
```rust
//! Validation rules (master plan §3.1, AC-E3c/E3d). Filled in by Tasks 5b and 5c.

use super::schema::Recipe;
use super::{Normalization, RecipeError};

pub(super) fn normalize(r: Recipe) -> Result<(Recipe, Vec<Normalization>), RecipeError> {
    if r.schema_version != crate::SCHEMA_VERSION {
        return Err(RecipeError::SchemaVersion(r.schema_version));
    }
    Ok((r, Vec::new()))
}
```

- [ ] **Step 2: 실패하는 테스트 작성** — `recipe/mod.rs`를 Step 4 구현으로 만들되 `from_json`·`normalize` 본문을 `todo!()`로 두고 아래 테스트를 붙인다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn err(json: &str) -> RecipeError {
        Recipe::from_json(json).unwrap_err()
    }

    mod parsing {
        use super::*;

        #[test]
        fn minimal_recipe_gets_no_effect_defaults() {
            let n = Recipe::from_json(r#"{"schema_version":1}"#).unwrap();
            assert_eq!(n.recipe(), &Recipe::default());
        }

        #[test]
        fn accepts_the_full_master_plan_example_with_default_values() {
            let json = r#"{
              "schema_version": 1,
              "basic": {"exposure":0,"contrast":0,"highlights":0,"shadows":0,"whites":0,"blacks":0,
                        "temperature":0,"tint":0,"vibrance":0,"saturation":0,"texture":0,"clarity":0,"dehaze":0},
              "hsl": {"hue":[0,0,0,0,0,0,0,0],"saturation":[0,0,0,0,0,0,0,0],"luminance":[0,0,0,0,0,0,0,0]},
              "tone_curve": {"master":[[0,0],[255,255]],"red":[[0,0],[255,255]],"green":[[0,0],[255,255]],"blue":[[0,0],[255,255]]},
              "color_grade": {"shadows":{"hue":0,"sat":0,"lum":0},"midtones":{"hue":0,"sat":0,"lum":0},
                              "highlights":{"hue":0,"sat":0,"lum":0},"global":{"hue":0,"sat":0,"lum":0},"blending":50,"balance":0},
              "split_toning": {"shadow_hue":0,"shadow_sat":0,"highlight_hue":0,"highlight_sat":0,"balance":0},
              "detail": {"sharpen_amount":0,"sharpen_radius":1.0,"sharpen_detail":25,"sharpen_masking":0,
                         "grain_amount":0,"grain_size":25,"grain_frequency":50,
                         "vignette_amount":0,"vignette_midpoint":50,"vignette_feather":50},
              "masks": [],
              "crop": null
            }"#;
            assert_eq!(
                Recipe::from_json(json).unwrap().recipe(),
                &Recipe::default()
            );
        }

        #[test]
        fn rejects_unknown_top_level_field() {
            assert!(matches!(
                err(r#"{"schema_version":1,"foo":1}"#),
                RecipeError::Json(_)
            ));
        }

        #[test]
        fn rejects_unknown_nested_field() {
            let e = err(r#"{"schema_version":1,"basic":{"exposur":1}}"#);
            assert!(matches!(e, RecipeError::Json(_)), "{e:?}");
        }

        #[test]
        fn rejects_unknown_mask_field() {
            let e = err(
                r#"{"schema_version":1,"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5,"bogus":1}]}"#,
            );
            assert!(matches!(e, RecipeError::Json(_)), "{e:?}");
        }

        #[test]
        fn rejects_wrong_type() {
            let e = err(r#"{"schema_version":1,"basic":{"exposure":"1"}}"#);
            assert!(matches!(e, RecipeError::Json(_)), "{e:?}");
        }

        #[test]
        fn rejects_null_number() {
            let e = err(r#"{"schema_version":1,"basic":{"exposure":null}}"#);
            assert!(matches!(e, RecipeError::Json(_)), "{e:?}");
        }

        #[test]
        fn rejects_infinity_literal() {
            let e = err(r#"{"schema_version":1,"basic":{"exposure":Infinity}}"#);
            assert!(matches!(e, RecipeError::Json(_)), "{e:?}");
        }

        #[test]
        fn rejects_missing_schema_version() {
            assert!(matches!(err(r#"{"basic":{}}"#), RecipeError::Json(_)));
        }

        #[test]
        fn rejects_wrong_schema_version() {
            assert_eq!(
                err(r#"{"schema_version":2}"#),
                RecipeError::SchemaVersion(2)
            );
        }

        #[test]
        fn accepts_null_crop_as_none() {
            let n = Recipe::from_json(r#"{"schema_version":1,"crop":null}"#).unwrap();
            assert_eq!(n.recipe().crop, None);
        }

        #[test]
        fn round_trips_through_serde() {
            let src = r#"{"schema_version":1,"basic":{"exposure":0.5,"contrast":10},"masks":[{"kind":"linear","x0":0,"y0":0,"x1":0,"y1":1,"feather":0.3,"adjust":{"exposure":-1}}]}"#;
            let n = Recipe::from_json(src).unwrap();
            let again = Recipe::from_json(&serde_json::to_string(n.recipe()).unwrap()).unwrap();
            assert_eq!(again.recipe(), n.recipe());
        }
    }
}
```

- [ ] **Step 3: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::` / Expected: FAIL.

- [ ] **Step 4: 구현** — `engine/core/src/recipe/mod.rs` 테스트 모듈 위 전체:

```rust
//! Recipe JSON (master plan §3.1): schema, validation, and the `Normalized` type that render
//! requires. A `Normalized` can only be built by [`Recipe::normalize`] / [`Recipe::from_json`],
//! so the renderer never sees NaN or out-of-range values (AC-E3c).

mod schema;
mod validate;

pub use schema::*;

use serde::Serialize;
use thiserror::Error;

/// Reasons a recipe is rejected outright.
#[derive(Debug, Error, PartialEq)]
pub enum RecipeError {
    /// JSON syntax, wrong type, `null` for a number, or an unknown field.
    #[error("recipe json: {0}")]
    Json(String),
    /// `schema_version` is not the one this build accepts.
    #[error("unsupported schema_version {0}, expected {expected}", expected = crate::SCHEMA_VERSION)]
    SchemaVersion(u32),
    /// A number is NaN or infinite (e.g. JSON `1e39` overflows f32).
    #[error("field `{0}` is not a finite number")]
    NonFinite(String),
    /// More than 3 masks (AC-E5a).
    #[error("{0} masks, at most 3 allowed")]
    TooManyMasks(usize),
    /// Curve with fewer than 2 or more than 16 points.
    #[error("tone_curve.{channel} has {count} points, expected 2..=16")]
    CurvePointCount {
        /// Curve name.
        channel: &'static str,
        /// Points given.
        count: usize,
    },
    /// Curve x values not strictly increasing.
    #[error("tone_curve.{channel}[{index}] x is not greater than the previous point")]
    CurveNotIncreasing {
        /// Curve name.
        channel: &'static str,
        /// First offending point.
        index: usize,
    },
    /// Mask whose geometry selects nothing or is undefined.
    #[error("masks[{index}]: {reason}")]
    DegenerateMask {
        /// Mask index.
        index: usize,
        /// What is wrong.
        reason: &'static str,
    },
    /// Range mask with `lo > hi`.
    #[error("masks[{index}]: range lo > hi")]
    RangeInverted {
        /// Mask index.
        index: usize,
    },
    /// Crop box outside [0,1] or with non-positive size.
    #[error("crop box out of bounds: x={x} y={y} w={w} h={h}")]
    CropOutOfBounds {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        w: f32,
        /// Height.
        h: f32,
    },
    /// Crop area below 25% (AC-E7b).
    #[error("crop area {0:.3} below minimum 0.25")]
    CropTooSmall(f32),
}

/// A clamped field reported back to the user (AC-E3d).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Normalization {
    /// Path such as `basic.exposure`, `hsl.hue[3]`, `masks[0].adjust.exposure`.
    pub path: String,
    /// Input value.
    pub from: f32,
    /// Value after clamping.
    pub to: f32,
    /// Always `"clamped"` in schema v1.
    pub reason: &'static str,
}

/// A recipe that passed validation. Serializes as `{"recipe":…,"normalizations":[…]}`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Normalized {
    recipe: Recipe,
    normalizations: Vec<Normalization>,
}

impl Normalized {
    /// The validated, clamped recipe.
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }

    /// Fields that were clamped.
    pub fn normalizations(&self) -> &[Normalization] {
        &self.normalizations
    }
}

impl Recipe {
    /// Parse and validate.
    pub fn from_json(json: &str) -> Result<Normalized, RecipeError> {
        let recipe: Recipe =
            serde_json::from_str(json).map_err(|e| RecipeError::Json(e.to_string()))?;
        recipe.normalize()
    }

    /// Validate and clamp an in-memory recipe.
    pub fn normalize(self) -> Result<Normalized, RecipeError> {
        let (recipe, normalizations) = validate::normalize(self)?;
        Ok(Normalized {
            recipe,
            normalizations,
        })
    }
}
```

- [ ] **Step 5: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::` / Expected: 모두 PASS.

- [ ] **Step 6: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/recipe
git commit -m "feat(engine): 레시피 스키마 v1 전체 필드와 파싱 (미지 필드·null·타입 오류 거부)"
```

---

## Task 5b: 레시피 수치 정규화

**Files:**
- Modify: `engine/core/src/recipe/validate.rs`, `engine/core/src/recipe/mod.rs`(테스트)

**Interfaces:**
- Produces: 모든 수치 필드의 유한성 검사와 범위 클램프. 경로 형식 `basic.exposure`, `hsl.hue[3]`, `color_grade.shadows.hue`, `detail.grain_size`.

- [ ] **Step 1: 실패하는 테스트 작성** — `recipe/mod.rs` 테스트 모듈의 `mod parsing { … }` 뒤에 추가:

```rust
    mod clamping {
        use super::*;

        #[test]
        fn clamps_exposure_and_reports_path_from_to() {
            let n = Recipe::from_json(r#"{"schema_version":1,"basic":{"exposure":9}}"#).unwrap();
            let want = Normalization {
                path: "basic.exposure".into(),
                from: 9.0,
                to: 5.0,
                reason: "clamped",
            };
            assert_eq!(n.normalizations(), &[want]);
        }

        #[test]
        fn reports_hsl_band_index() {
            let n = Recipe::from_json(r#"{"schema_version":1,"hsl":{"hue":[0,0,0,150,0,0,0,0]}}"#)
                .unwrap();
            assert_eq!(n.normalizations()[0].path, "hsl.hue[3]");
        }

        #[test]
        fn serializes_report_with_recipe_and_normalizations() {
            let n = Recipe::from_json(r#"{"schema_version":1,"basic":{"tint":-300}}"#).unwrap();
            let v: serde_json::Value = serde_json::to_value(&n).unwrap();
            assert_eq!(v["normalizations"][0]["path"], "basic.tint");
            assert_eq!(v["recipe"]["basic"]["tint"], -100.0);
        }

        #[test]
        fn rejects_f32_overflow_as_non_finite() {
            let e = err(r#"{"schema_version":1,"basic":{"exposure":1e39}}"#);
            assert_eq!(e, RecipeError::NonFinite("basic.exposure".into()));
        }

        #[test]
        fn rejects_negative_f32_overflow_as_non_finite() {
            let e = err(r#"{"schema_version":1,"basic":{"contrast":-1e39}}"#);
            assert_eq!(e, RecipeError::NonFinite("basic.contrast".into()));
        }

        #[test]
        fn in_memory_nan_is_rejected_with_path() {
            let mut r = Recipe::default();
            r.detail.grain_size = f32::NAN;
            assert_eq!(
                r.normalize().unwrap_err(),
                RecipeError::NonFinite("detail.grain_size".into())
            );
        }
    }
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::tests::clamping` / Expected: FAIL(정규화 목록이 비어 있고 `1e39`가 통과됨).

- [ ] **Step 3: 구현** — `engine/core/src/recipe/validate.rs` 전체 교체:

```rust
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
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/recipe
git commit -m "feat(engine): 레시피 수치 정규화 (비유한 거부, 범위 클램프, 경로 보고)"
```

---

## Task 5c: 레시피 구조·기하 검증

**Files:**
- Modify: `engine/core/src/recipe/validate.rs`, `engine/core/src/recipe/mod.rs`(테스트)

**Interfaces:**
- Produces: 마스크 ≤ 3, 커브 2~16점·x 엄격 증가, 퇴화 마스크 거부(선형 끝점 동일, 방사형 반경 0, 범위 `lo>hi`·폭 0 하드 엣지), 크롭 범위·면적 ≥ 0.25, 마스크 경로 `masks[i].…`.

- [ ] **Step 1: 실패하는 테스트 작성** — 테스트 모듈의 `mod clamping { … }` 뒤에 추가:

```rust
    mod structure {
        use super::*;

        fn radial(cx: f32) -> String {
            format!(r#"{{"kind":"radial","cx":{cx},"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5}}"#)
        }

        #[test]
        fn reports_mask_index_in_path() {
            let json = format!(
                r#"{{"schema_version":1,"masks":[{},{{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5,"adjust":{{"exposure":-7}}}}]}}"#,
                radial(0.5)
            );
            let n = Recipe::from_json(&json).unwrap();
            assert_eq!(n.normalizations()[0].path, "masks[1].adjust.exposure");
        }

        #[test]
        fn rejects_more_than_three_masks() {
            let m = radial(0.5);
            let json = format!(r#"{{"schema_version":1,"masks":[{m},{m},{m},{m}]}}"#);
            assert_eq!(err(&json), RecipeError::TooManyMasks(4));
        }

        #[test]
        fn rejects_curve_with_17_points() {
            let pts: Vec<String> = (0..17)
                .map(|i| format!("[{},{}]", i * 15, i * 15))
                .collect();
            let json = format!(
                r#"{{"schema_version":1,"tone_curve":{{"red":[{}]}}}}"#,
                pts.join(",")
            );
            assert_eq!(
                err(&json),
                RecipeError::CurvePointCount {
                    channel: "red",
                    count: 17
                }
            );
        }

        #[test]
        fn rejects_curve_with_one_point() {
            let e = err(r#"{"schema_version":1,"tone_curve":{"master":[[0,0]]}}"#);
            assert_eq!(
                e,
                RecipeError::CurvePointCount {
                    channel: "master",
                    count: 1
                }
            );
        }

        #[test]
        fn rejects_non_increasing_curve_x() {
            let e = err(
                r#"{"schema_version":1,"tone_curve":{"blue":[[0,0],[128,100],[128,140],[255,255]]}}"#,
            );
            assert_eq!(
                e,
                RecipeError::CurveNotIncreasing {
                    channel: "blue",
                    index: 2
                }
            );
        }

        #[test]
        fn rejects_linear_mask_with_equal_endpoints() {
            let e = err(
                r#"{"schema_version":1,"masks":[{"kind":"linear","x0":0.3,"y0":0.3,"x1":0.3,"y1":0.3,"feather":0.5}]}"#,
            );
            assert!(
                matches!(e, RecipeError::DegenerateMask { index: 0, .. }),
                "{e:?}"
            );
        }

        #[test]
        fn rejects_radial_mask_with_zero_radius() {
            let e = err(
                r#"{"schema_version":1,"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0,"ry":0.2,"feather":0.5}]}"#,
            );
            assert!(
                matches!(e, RecipeError::DegenerateMask { index: 0, .. }),
                "{e:?}"
            );
        }

        #[test]
        fn rejects_inverted_range_mask() {
            let e = err(
                r#"{"schema_version":1,"masks":[{"kind":"range","channel":"luminance","lo":0.7,"hi":0.2,"smooth":0.1}]}"#,
            );
            assert_eq!(e, RecipeError::RangeInverted { index: 0 });
        }

        #[test]
        fn rejects_zero_width_hard_range_mask() {
            let e = err(
                r#"{"schema_version":1,"masks":[{"kind":"range","channel":"hue","lo":0.4,"hi":0.4,"smooth":0}]}"#,
            );
            assert!(
                matches!(e, RecipeError::DegenerateMask { index: 0, .. }),
                "{e:?}"
            );
        }

        #[test]
        fn rejects_crop_smaller_than_quarter_area() {
            let e = err(r#"{"schema_version":1,"crop":{"x":0,"y":0,"w":0.4,"h":0.4}}"#);
            assert!(matches!(e, RecipeError::CropTooSmall(_)), "{e:?}");
        }

        #[test]
        fn accepts_crop_of_exactly_quarter_area() {
            assert!(Recipe::from_json(
                r#"{"schema_version":1,"crop":{"x":0,"y":0,"w":0.5,"h":0.5}}"#
            )
            .is_ok());
        }

        #[test]
        fn rejects_crop_outside_unit_square() {
            let e = err(r#"{"schema_version":1,"crop":{"x":0.5,"y":0,"w":0.6,"h":1}}"#);
            assert!(matches!(e, RecipeError::CropOutOfBounds { .. }), "{e:?}");
        }
    }
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::tests::structure` / Expected: FAIL(구조 오류가 통과됨).

- [ ] **Step 3: 구현** — 5b의 `validate.rs`에 다음을 반영한다(5b 본문은 그대로 두고 추가·교체만).

(1) 파일 머리의 import **두 줄 전체**(`use super::schema::{Basic, …, SplitToning};`와 바로 다음 줄 `use super::{Normalization, RecipeError};`)를 다음 블록으로 교체한다. 블록에 두 번째 줄이 다시 들어 있으므로 기존 줄을 남기면 E0252(중복 import)가 난다. 교체 후 파일의 `use` 문은 아래 두 개뿐이어야 한다.
```rust
use super::schema::{
    Adjust, Basic, ColorGrade, Crop, Detail, GradeZone, Hsl, Mask, Recipe, SplitToning, ToneCurve,
};
use super::{Normalization, RecipeError};

pub(super) const MAX_MASKS: usize = 3;
pub(super) const MAX_CURVE_POINTS: usize = 16;
pub(super) const MIN_CROP_AREA: f32 = 0.25;
```

(2) 첫 `impl Clamper { … }`(`field`·`zone`이 있는 블록)에 다음 세 메서드를 추가:
```rust
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
```

(3) 둘째 `impl Clamper { … }`(`basic`·`hsl`·`grade`·`split`·`detail`)에 추가:
```rust
    fn curves(&mut self, t: &mut ToneCurve) -> Result<(), RecipeError> {
        self.curve("master", &mut t.master)?;
        self.curve("red", &mut t.red)?;
        self.curve("green", &mut t.green)?;
        self.curve("blue", &mut t.blue)
    }
```

(4) 파일 끝의 `normalize`를 다음으로 교체(`validate_crop` 추가 포함):
```rust
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
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core recipe::` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/recipe
git commit -m "feat(engine): 레시피 구조 검증 (마스크·커브·크롭, 퇴화 기하 거부)"
```

---

## Task 6: 기본 패널 연산

**Files:**
- Create: `engine/core/src/render/basic.rs`
- Modify: `engine/core/src/render/mod.rs`(`pub mod basic;`만)

**Interfaces:**
- Consumes: Task 5a `Basic`.
- Produces: `luma([f32;3])->f32`, `apply_exposure_linear`, `apply_white_balance_linear`, `apply_tone_encoded(px,&Basic)`, `apply_color_encoded(px, vibrance, saturation)`. 모두 비클램프, 생동감 가중치만 `[0,1]`로 제한.

- [ ] **Step 1: 실패하는 테스트 작성** — `render/mod.rs`를 다음 두 줄로 두고:

```rust
//! Render pipeline (Task 7).

pub mod basic;
```

`render/basic.rs`를 Step 3 구현의 `fn` 본문 `todo!()`로 만든 뒤 아래 테스트를 붙인다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: [f32; 3] = [0.4, 0.4, 0.4];
    const WARM: [f32; 3] = [0.7, 0.5, 0.3];

    fn chroma(p: [f32; 3]) -> f32 {
        p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])
    }

    fn basic(f: impl FnOnce(&mut Basic)) -> Basic {
        let mut b = Basic::default();
        f(&mut b);
        b
    }

    #[test]
    fn plus_one_ev_doubles_linear_values() {
        assert_eq!(
            apply_exposure_linear([0.25, 0.1, 0.4], 1.0),
            [0.5, 0.2, 0.8]
        );
    }

    #[test]
    fn zero_ev_is_identity() {
        assert_eq!(apply_exposure_linear(WARM, 0.0), WARM);
    }

    #[test]
    fn positive_temperature_raises_red_over_blue() {
        let out = apply_white_balance_linear(GRAY, 50.0, 0.0);
        assert!(out[0] > GRAY[0] && out[2] < GRAY[2], "{out:?}");
    }

    #[test]
    fn positive_tint_lowers_green() {
        assert!(apply_white_balance_linear(GRAY, 0.0, 50.0)[1] < GRAY[1]);
    }

    #[test]
    fn tone_defaults_are_identity() {
        assert_eq!(apply_tone_encoded(WARM, &Basic::default()), WARM);
    }

    #[test]
    fn tone_defaults_keep_values_above_one() {
        assert_eq!(
            apply_tone_encoded([1.3, 1.2, 1.1], &Basic::default()),
            [1.3, 1.2, 1.1]
        );
    }

    #[test]
    fn positive_contrast_pushes_values_away_from_mid_gray() {
        assert!(apply_tone_encoded([0.7; 3], &basic(|b| b.contrast = 50.0))[0] > 0.7);
    }

    #[test]
    fn negative_highlights_darken_bright_pixels() {
        assert!(apply_tone_encoded([0.9; 3], &basic(|b| b.highlights = -100.0))[0] < 0.9);
    }

    #[test]
    fn negative_highlights_bring_1_2_below_1() {
        let out = apply_tone_encoded([1.2; 3], &basic(|b| b.highlights = -100.0));
        assert!(out[0] < 1.0, "{out:?}");
    }

    #[test]
    fn positive_shadows_lift_dark_pixels() {
        assert!(apply_tone_encoded([0.1; 3], &basic(|b| b.shadows = 100.0))[0] > 0.1);
    }

    #[test]
    fn negative_saturation_reduces_chroma() {
        assert!(chroma(apply_color_encoded(WARM, 0.0, -50.0)) < chroma(WARM));
    }

    #[test]
    fn minus_100_saturation_yields_neutral_gray() {
        let out = apply_color_encoded(WARM, 0.0, -100.0);
        assert!(
            (out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6,
            "{out:?}"
        );
    }

    #[test]
    fn negative_vibrance_never_inverts_an_over_range_pixel() {
        // spread 2.9 > 1: an unclamped weight would give k = 1 + (-1)(1 - 2.9) = 2.9 (fine),
        // while +100 would give k = 1 + (1 - 2.9) = -0.9 (inverted). Both must keep hue order.
        let hot = [3.0, 0.2, 0.1];
        for v in [-100.0, 100.0] {
            let out = apply_color_encoded(hot, v, 0.0);
            assert!(out[0] > out[1] && out[1] >= out[2], "vibrance {v}: {out:?}");
        }
    }

    #[test]
    fn vibrance_boosts_dull_pixels_more_than_vivid_ones() {
        let dull = [0.5, 0.45, 0.4];
        let vivid = [0.9, 0.3, 0.1];
        let gain_dull = chroma(apply_color_encoded(dull, 100.0, 0.0)) / chroma(dull);
        let gain_vivid = chroma(apply_color_encoded(vivid, 100.0, 0.0)) / chroma(vivid);
        assert!(
            gain_dull > gain_vivid,
            "dull {gain_dull} vivid {gain_vivid}"
        );
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core render::basic` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
//! Basic panel math. These are OUR definitions, direction-compatible with Lightroom but not
//! identical (spec D8, AC-E3b). Two stages: linear-light ops, then encoded-domain ops.
//! Nothing here clamps (master plan A3): values above 1 or below 0 flow to the next stage and
//! only `buffer::quantize` clamps. The WebGL2 preview must port these formulas verbatim (A9).

use crate::recipe::Basic;

/// Rec.709 luma of an encoded pixel (a tonal weight, not a colorimetric quantity).
pub fn luma(px: [f32; 3]) -> f32 {
    0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear stage: exposure, gain `2^EV`.
pub fn apply_exposure_linear(px: [f32; 3], ev: f32) -> [f32; 3] {
    let g = libm::exp2f(ev);
    px.map(|x| x * g)
}

/// Linear stage: relative white balance. +temperature = warmer (more R, less B),
/// +tint = more magenta (less G). ±100 maps to ±20% channel gain.
pub fn apply_white_balance_linear(px: [f32; 3], temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature / 100.0;
    let n = tint / 100.0;
    [
        px[0] * (1.0 + 0.2 * t),
        px[1] * (1.0 - 0.2 * n),
        px[2] * (1.0 - 0.2 * t),
    ]
}

/// Encoded stage: contrast around 0.5, then luma-weighted highlights / shadows / whites /
/// blacks. Highlights scale proportionally (`x * (1 + 0.25 * hi * w)`), so values pushed above 1
/// by exposure stay distinguishable and `highlights < 0` can bring them back below 1.
pub fn apply_tone_encoded(px: [f32; 3], b: &Basic) -> [f32; 3] {
    let c = b.contrast / 100.0;
    let contrasted = px.map(|x| 0.5 + (x - 0.5) * (1.0 + 0.6 * c));

    let l = luma(contrasted);
    let w_hi = smoothstep(0.5, 1.0, l);
    let w_sh = 1.0 - smoothstep(0.0, 0.5, l);
    let hi = b.highlights / 100.0;
    let sh = b.shadows / 100.0;
    let wh = b.whites / 100.0;
    let bl = b.blacks / 100.0;
    let one_minus_l = 1.0 - l;

    contrasted.map(|x| {
        x + hi * 0.25 * w_hi * x
            + sh * 0.25 * w_sh * (1.0 - x)
            + wh * 0.15 * l * l
            + bl * 0.15 * one_minus_l * one_minus_l
    })
}

/// Encoded stage: saturation scales chroma around luma; vibrance does the same weighted toward
/// less saturated pixels. The vibrance weight `1 - spread` is clamped to [0, 1]: inputs are not
/// clamped (A3), so an over-exposed pixel can have `spread > 1`, and an unclamped weight would
/// make the chroma factor negative and invert the hue. With `s, v ∈ [-1, 1]` the factor
/// `k = (1 + s) * (1 + v * w)` is therefore always ≥ 0.
pub fn apply_color_encoded(px: [f32; 3], vibrance: f32, saturation: f32) -> [f32; 3] {
    let y = luma(px);
    let s = saturation / 100.0;
    let v = vibrance / 100.0;
    let mx = px[0].max(px[1]).max(px[2]);
    let mn = px[0].min(px[1]).min(px[2]);
    let w = (1.0 - (mx - mn)).clamp(0.0, 1.0);
    let k = (1.0 + s) * (1.0 + v * w);
    px.map(|x| y + (x - y) * k)
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core render::basic` / Expected: 모두 PASS. `negative_vibrance_never_inverts_an_over_range_pixel`은 가중치 `clamp`를 지우면 `+100`에서 실패한다.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/render
git commit -m "feat(engine): 기본 패널 연산 (비클램프, 하이라이트 비례식, 생동감 가중치 제한)"
```

---

## Task 7: 렌더 파이프라인

**Files:**
- Modify: `engine/core/src/render/mod.rs`

**Interfaces:**
- Consumes: Task 2 `ImageF32`, `Rgba8`; Task 5a `Normalized`, `Recipe`, `identity_curve`; Task 6 basic.
- Produces: `struct RenderContext{seed: u32}`(`Default`), `enum RenderError{NotYetSupported{feature,plan}, NonFinite}`, `unsupported_feature(&Recipe)->Option<&'static str>`(`crop`은 절대 포함 안 함, A10), `render(&ImageF32,&Normalized,&RenderContext)->Result<ImageF32,_>`(출력 크기 = 입력 크기), `render_rgba8(&Rgba8,&Normalized,&RenderContext)->Result<Rgba8,_>`(양자화 지점 ①).

- [ ] **Step 1: 실패하는 테스트 작성** — Step 3 구현을 `fn` 본문 `todo!()`로 두고(`pub mod basic;` 유지) 아래 테스트를 붙인다.

```rust
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
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core render::tests` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
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

pub mod basic;

use crate::buffer::{ImageF32, Rgba8};
use crate::color::{srgb_decode, srgb_encode};
use crate::recipe::{identity_curve, Normalized, Recipe};
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
    if let Some(feature) = unsupported_feature(r) {
        return Err(RenderError::NotYetSupported {
            feature,
            plan: "Plan 1b",
        });
    }
    let b = &r.basic;
    let out = src.map_pixels(|px| {
        let lin = px.map(srgb_decode);
        let lin = basic::apply_exposure_linear(lin, b.exposure);
        let lin = basic::apply_white_balance_linear(lin, b.temperature, b.tint);
        let enc = lin.map(srgb_encode);
        let enc = basic::apply_tone_encoded(enc, b);
        basic::apply_color_encoded(enc, b.vibrance, b.saturation)
    });
    if out.samples().iter().all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err(RenderError::NonFinite)
    }
}

/// RGBA8 in, RGBA8 out: the full-resolution render with its single final quantization
/// (master plan A3 point ①). This is what the answer PNG stores.
pub fn render_rgba8(
    src: &Rgba8,
    recipe: &Normalized,
    ctx: &RenderContext,
) -> Result<Rgba8, RenderError> {
    Ok(render(&ImageF32::from_rgba8(src), recipe, ctx)?.to_rgba8())
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core render::` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/render/mod.rs
git commit -m "feat(engine): 렌더 파이프라인 (전체 프레임, seed 컨텍스트, 1b 기능 명시 거부)"
```

---

## 분할 마무리: 인수 확인과 PR

- [ ] **Step 1: 완료 인수 조건 실행** — `01a-1-numeric-image.md` 머리의 명령 6개. Expected: 모두 성공.

- [ ] **Step 2: 푸시·PR·필수 체크 대기**

```bash
git push -u origin feat/engine-1a-2-recipe-render
gh pr create --base main --head feat/engine-1a-2-recipe-render --title "Plan 1a-2: 레시피·렌더" --body "Plan 1a-2(.omc/plans/01a-2-recipe-render.md) Task 5a~7. 레시피 스키마 v1·검증, 기본 패널, 렌더 파이프라인."
gh pr checks --watch --required
```
Expected: 필수 체크 전부 pass. 병합은 사용자 확인 후.
