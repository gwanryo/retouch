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
