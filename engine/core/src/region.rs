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

/// Rasterized region: one alpha byte per pixel, row-major. A pixel is inside when alpha >= 128.
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

    /// Maps the mask to `width`x`height` (scoring resolution). Plan 1a rule, part of the score
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

    /// Rasterize at `width`x`height` (the original's resolution).
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
        // 4x1 source with alpha [0, 255, 255, 0] -> 2x1: centers fall in source px 1 and 3.
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
