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
