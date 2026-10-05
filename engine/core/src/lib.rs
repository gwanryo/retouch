//! Retouch reference engine.
//!
//! Everything that decides a pixel value or a score lives here and is shared by the native CLI
//! and the wasm build. Design: `.omc/plans/00-master-plan.md` §2 (A1..A9) and §5.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod buffer;
pub mod color;
pub mod decode;
pub mod hash;
pub mod recipe;
pub mod region;
pub mod render;
pub mod resample;
pub mod score;

pub use resample::scoring_image;

/// Render pipeline version. Bump whenever any render math changes (golden hashes change).
pub const ENGINE_VERSION: &str = "engine-0.1.0";
/// Scoring version. Bump whenever score math, [`score::T_TILE`] or the scoring-image path changes.
pub const SCORING_VERSION: &str = "scoring-0.1.0";
/// Recipe JSON `schema_version` accepted by this build.
pub const SCHEMA_VERSION: u32 = 1;
/// Long edge of the scoring image (AC-S1h).
pub const SCORING_LONG_EDGE: u32 = 1024;
