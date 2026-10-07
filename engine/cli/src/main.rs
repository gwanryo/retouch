//! `engine-cli`: build-time answer generation, scoring and golden data (master plan A8).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod golden;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use engine_core::buffer::{ImageF32, Rgba8};
use engine_core::decode::{decode_rgba8, encode_png};
use engine_core::hash::sha256_hex;
use engine_core::recipe::{Normalized, Recipe};
use engine_core::region::RegionSpec;
use engine_core::render::{render_rgba8, RenderContext};
use engine_core::score::ScoringReference;
use engine_core::{
    scoring_image, ENGINE_VERSION, SCHEMA_VERSION, SCORING_LONG_EDGE, SCORING_VERSION,
};

/// Long edge of normalized content originals (AC-E1c, master plan §3.2).
const ORIGINAL_LONG_EDGE: u32 = 2048;

#[derive(Parser)]
#[command(name = "engine-cli", version, about = "Retouch reference engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a deterministic synthetic test image as PNG and baseline 4:2:0 JPEG q90.
    Fixture {
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 512)]
        width: u16,
        #[arg(long, default_value_t = 384)]
        height: u16,
    },
    /// Downscale a photo with the scoring-image filter and re-encode as 4:2:0 JPEG.
    PrepPhoto {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        long_edge: u32,
        #[arg(long, default_value_t = 90)]
        quality: u8,
        #[arg(long)]
        progressive: bool,
        #[arg(long)]
        output: PathBuf,
    },
    /// Render a recipe at full size, write PNG, print SHA-256 of the RGBA8.
    Render {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long, default_value_t = 0)]
        seed: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Apply `scoring_image` (master plan A4), write PNG, print `WxH sha256`.
    ScoringImage {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = SCORING_LONG_EDGE)]
        long_edge: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Write `answer_2048.png`, `answer_1024.png` and `meta.json` for one challenge.
    GenAnswer {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        seed: u32,
        #[arg(long)]
        out_dir: PathBuf,
        /// Skip the 2048 long-edge check (fixtures and tests only).
        #[arg(long)]
        allow_any_size: bool,
    },
    /// Score a full-frame player render against the original and the answer PNG (AC-S1d).
    Score {
        #[arg(long)]
        original: PathBuf,
        #[arg(long)]
        answer: PathBuf,
        #[arg(long)]
        player: PathBuf,
        /// Region spec: inline JSON or a path to a JSON file.
        #[arg(long, default_value = r#"{"kind":"full"}"#)]
        region: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Print SHA-256 of the decoded RGBA8 pixels.
    Hash {
        #[arg(long)]
        input: PathBuf,
    },
    /// Golden data: write, check or guard `engine/golden/expected.json`.
    Golden {
        #[command(subcommand)]
        cmd: golden::GoldenCmd,
    },
}

pub(crate) fn load_image(path: &Path) -> Result<Rgba8> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    decode_rgba8(&bytes).with_context(|| format!("decode {}", path.display()))
}

pub(crate) fn load_recipe(path: &Path) -> Result<Normalized> {
    let json = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let n = Recipe::from_json(&json).with_context(|| format!("recipe {}", path.display()))?;
    for note in n.normalizations() {
        eprintln!("note: {} clamped {} -> {}", note.path, note.from, note.to);
    }
    Ok(n)
}

fn write_png(path: &Path, img: &Rgba8) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, encode_png(img)?).with_context(|| format!("write {}", path.display()))
}

fn encode_jpeg(img: &Rgba8, quality: u8, progressive: bool) -> Result<Vec<u8>> {
    let w = u16::try_from(img.width()).context("width exceeds JPEG limit")?;
    let h = u16::try_from(img.height()).context("height exceeds JPEG limit")?;
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, quality);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_2_0);
    enc.set_progressive(progressive);
    enc.encode(img.data(), w, h, jpeg_encoder::ColorType::Rgba)?;
    Ok(out)
}

/// Gradient + eight saturated patches + a gray ramp: exercises tone and color paths.
fn make_fixture(width: u16, height: u16) -> Result<Rgba8> {
    const PATCHES: [[f32; 3]; 8] = [
        [0.9, 0.1, 0.1],
        [0.9, 0.5, 0.1],
        [0.9, 0.9, 0.1],
        [0.1, 0.8, 0.2],
        [0.1, 0.8, 0.8],
        [0.1, 0.2, 0.9],
        [0.5, 0.1, 0.8],
        [0.9, 0.2, 0.6],
    ];
    let (w, h) = (u32::from(width), u32::from(height));
    let t = |i: u32, n: u32| {
        if n > 1 {
            i as f32 / (n - 1) as f32
        } else {
            0.5
        }
    };
    let mut img = ImageF32::new_filled(w, h, [0.0; 3])?;
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (t(x, w), t(y, h));
            let px = if v < 0.25 {
                PATCHES[((u * 8.0) as usize).min(7)]
            } else if v < 0.4 {
                [u, u, u]
            } else {
                [0.15 + 0.7 * u, 0.25 + 0.5 * (1.0 - v), 0.35 + 0.4 * v]
            };
            img.set_pixel(x, y, px);
        }
    }
    Ok(img.to_rgba8())
}

fn parse_region(arg: &str) -> Result<RegionSpec> {
    let json = if arg.trim_start().starts_with('{') {
        arg.to_owned()
    } else {
        fs::read_to_string(arg).with_context(|| format!("read region {arg}"))?
    };
    Ok(RegionSpec::from_json(&json)?)
}

fn is_png(path: &Path) -> Result<bool> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(bytes.starts_with(&[0x89, b'P', b'N', b'G']))
}

fn gen_answer(
    input: &Path,
    recipe: &Path,
    seed: u32,
    out_dir: &Path,
    allow_any_size: bool,
) -> Result<()> {
    let t0 = Instant::now();
    let src = load_image(input)?;
    let long = src.width().max(src.height());
    if long != ORIGINAL_LONG_EDGE && !allow_any_size {
        bail!("original long edge is {long}, expected {ORIGINAL_LONG_EDGE} (AC-E1c)");
    }
    let full = render_rgba8(&src, &load_recipe(recipe)?, &RenderContext { seed })?;
    let small = scoring_image(&full, SCORING_LONG_EDGE)?;
    write_png(&out_dir.join("answer_2048.png"), &full)?;
    write_png(&out_dir.join("answer_1024.png"), &small)?;
    let meta = serde_json::json!({
        "engine_version": ENGINE_VERSION,
        "scoring_version": SCORING_VERSION,
        "schema_version": SCHEMA_VERSION,
        "seed": seed,
        "original_sha256_rgba8": sha256_hex(src.data()),
        "answer_sha256_rgba8_2048": sha256_hex(full.data()),
        "answer_sha256_rgba8_1024": sha256_hex(small.data()),
        "width": full.width(), "height": full.height(),
        "scoring_width": small.width(), "scoring_height": small.height(),
    });
    fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;
    eprintln!("gen-answer: {} ms", t0.elapsed().as_millis());
    println!("{}", sha256_hex(small.data()));
    Ok(())
}

fn score_cmd(
    original: &Path,
    answer: &Path,
    player: &Path,
    region: &str,
    output: &Path,
) -> Result<()> {
    if !is_png(answer)? {
        bail!(
            "--answer must be the lossless answer PNG, got {}",
            answer.display()
        );
    }
    let t0 = Instant::now();
    let reference = ScoringReference::new(
        &load_image(original)?,
        &load_image(answer)?,
        &parse_region(region)?,
    )?;
    let s = reference.score(&load_image(player)?, None)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_string(&s)?)?;
    eprintln!("score: {} ms", t0.elapsed().as_millis());
    println!("{}", s.correction_score);
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Fixture {
            out_dir,
            width,
            height,
        } => {
            let img = make_fixture(width, height)?;
            write_png(&out_dir.join("fixture.png"), &img)?;
            fs::write(out_dir.join("fixture.jpg"), encode_jpeg(&img, 90, false)?)?;
            println!("wrote fixture.png fixture.jpg ({width}x{height})");
        }
        Cmd::PrepPhoto {
            input,
            long_edge,
            quality,
            progressive,
            output,
        } => {
            let small = scoring_image(&load_image(&input)?, long_edge)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, encode_jpeg(&small, quality, progressive)?)?;
            println!("{}x{}", small.width(), small.height());
        }
        Cmd::Render {
            input,
            recipe,
            seed,
            output,
        } => {
            let out = render_rgba8(
                &load_image(&input)?,
                &load_recipe(&recipe)?,
                &RenderContext { seed },
            )?;
            write_png(&output, &out)?;
            println!("{}", sha256_hex(out.data()));
        }
        Cmd::ScoringImage {
            input,
            long_edge,
            output,
        } => {
            let out = scoring_image(&load_image(&input)?, long_edge)?;
            write_png(&output, &out)?;
            println!(
                "{}x{} {}",
                out.width(),
                out.height(),
                sha256_hex(out.data())
            );
        }
        Cmd::GenAnswer {
            input,
            recipe,
            seed,
            out_dir,
            allow_any_size,
        } => gen_answer(&input, &recipe, seed, &out_dir, allow_any_size)?,
        Cmd::Score {
            original,
            answer,
            player,
            region,
            output,
        } => score_cmd(&original, &answer, &player, &region, &output)?,
        Cmd::Hash { input } => println!("{}", sha256_hex(load_image(&input)?.data())),
        Cmd::Golden { cmd } => golden::run(cmd)?,
    }
    Ok(())
}
