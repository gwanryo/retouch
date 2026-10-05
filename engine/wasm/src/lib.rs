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
