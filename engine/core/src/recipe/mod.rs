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
}
