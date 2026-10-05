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
}
