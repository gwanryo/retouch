# Plan 1a-4: CLI·골든·CI (Task 11a~14)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development(권장) 또는 superpowers:executing-plans. Rust 코드 전에 `rust-best-practices` 로드. 공통 계약·사전 조건·TDD 예외는 `01-engine-foundation.md`에 있다.

**목표:** 빌드 타임 CLI(`fixture`, `prep-photo`, `render`, `scoring-image`, `hash`, `gen-answer`, `score`, `golden`), CC0 실사진 골든(98·99·100 경계 포함)과 버전 가드, wasm 골든 완전 일치 테스트, CI 골든·가드 단계, 성능 기준선 기록.

**선행 조건:** 분할 3(`01a-3-region-score-wasm.md`) PR이 `main`에 병합되어 있다. 사용하는 것: core 전체, wasm API(§3.4), `engine/core/tests/smoke.rs`.

**브랜치 / PR:** `feat/engine-1a-4-cli-golden-ci` / "Plan 1a-4: CLI·골든·CI"

**커버 AC:** E1a(CI 런너, 플레이어별 렌더·채점 이미지 해시), E1c(meta 해시), S1b(guard, 채점 프로브), S1c(경계 골든), S1d, S1h(골든), S3(골든), S5a(기준선), 마스터 A8 중 CLI·골든 부분(콘텐츠 manifest는 Plan 3)·§5.6.

**완료 인수 조건:** 분할 1·3의 완료 명령 전부 + 아래가 성공하고 CI 녹색.
```bash
cargo run --locked --manifest-path engine/Cargo.toml -p engine-cli -- golden check --dir engine/golden
cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden check --dir engine/golden
node engine/golden/verify-sources.mjs
node --test engine/golden/test/verify-sources.test.mjs
node --test "engine/wasm/test/*.test.mjs"
git diff --exit-code -- engine/Cargo.lock
```
참고(검증 실행): CLI 단위 테스트 30개(guard·판독·Cargo.lock 크레이트 버전), CLI 통합 14개(골든 write/check 계약 5개 포함), wasm node 8개, `verify-sources` node 7개.

**Review Focus (이 분할):** 다른 인코더가 만든 실제 JPEG(카메라 4:2:2, progressive 4:2:0 세로)와 비정수 배율 2048×1365→1024×683(Task 12b), 저장된 PNG만으로 채점 입력 재현(Task 11b), 같은 버전에서의 결과 변경·삭제와 채점 프로브(Task 12a), CLI 산출물을 wasm이 그대로 재현하는지(Task 13), `ImageData` 해제 후의 메모리 기준선(Task 13).

---

## Task 11a: CLI 공통부와 이미지 명령 (fixture · prep-photo · render · scoring-image · hash)

**Files:**
- Modify: `engine/cli/src/main.rs`
- Create: `engine/cli/tests/cli.rs`

**Interfaces:**
- Consumes: core 전체, `jpeg-encoder`.
- Produces: `pub(crate) load_image(&Path)->Result<Rgba8>`, `pub(crate) load_recipe(&Path)->Result<Normalized>`(정규화 보고를 stderr로), `write_png`, `encode_jpeg(&Rgba8, quality, progressive)`(4:2:0, RGBA 입력이라 알파는 인코더가 무시), 서브커맨드 `fixture --width --height`(u16), `prep-photo --long-edge --quality [--progressive]`(출력 `WxH`), `render --seed`(출력 SHA-256), `scoring-image [--long-edge 1024]`(출력 `WxH sha256`), `hash`.

- [ ] **Step 1: 브랜치와 실패하는 테스트** — `git switch main && git pull && git switch -c feat/engine-1a-4-cli-golden-ci`. `engine/cli/tests/cli.rs`:

```rust
//! End-to-end through the binary: fixture → gen-answer → score, plus argument guards.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = cli(args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{args:?} failed: {stderr}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn err(args: &[&str]) -> String {
    let out = cli(args);
    assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retouch-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn fixture_with_recipe(name: &str, recipe: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = tmp(name);
    ok(&["fixture", "--out-dir", s(&dir)]);
    let r = dir.join("r.json");
    fs::write(&r, recipe).unwrap();
    (dir.clone(), dir.join("fixture.jpg"), r)
}

#[test]
fn render_rejects_masks_instead_of_ignoring_them() {
    let rec = r#"{"schema_version":1,"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.3,"ry":0.3,"feather":0.5}]}"#;
    let (dir, jpg, r) = fixture_with_recipe("masks", rec);
    let out = dir.join("a.png");
    let e = err(&[
        "render",
        "--input",
        s(&jpg),
        "--recipe",
        s(&r),
        "--output",
        s(&out),
    ]);
    assert!(e.contains("masks"), "{e}");
}

#[test]
fn render_hash_is_stable_across_runs() {
    let rec = r#"{"schema_version":1,"basic":{"exposure":-0.5,"saturation":40}}"#;
    let (dir, jpg, r) = fixture_with_recipe("stable", rec);
    let run = |name: &str| {
        let out = dir.join(name);
        ok(&[
            "render",
            "--input",
            s(&jpg),
            "--recipe",
            s(&r),
            "--output",
            s(&out),
        ])
    };
    assert_eq!(run("a.png"), run("b.png"));
}

#[test]
fn prep_photo_downscales_and_can_write_progressive_jpeg() {
    let (dir, jpg, _) = fixture_with_recipe("prep", r#"{"schema_version":1}"#);
    let out = dir.join("p.jpg");
    let dims = ok(&[
        "prep-photo",
        "--input",
        s(&jpg),
        "--long-edge",
        "256",
        "--progressive",
        "--output",
        s(&out),
    ]);
    assert_eq!(dims, "256x192");
    let bytes = fs::read(&out).unwrap();
    assert!(
        bytes.windows(2).any(|w| w == [0xFF, 0xC2]),
        "no SOF2 (progressive) marker"
    );
    assert_eq!(ok(&["hash", "--input", s(&out)]).len(), 64);
}

#[test]
fn scoring_image_line_matches_hash_of_its_output() {
    let (dir, jpg, _) = fixture_with_recipe("scoring", r#"{"schema_version":1}"#);
    let out = dir.join("s.png");
    let line = ok(&[
        "scoring-image",
        "--input",
        s(&jpg),
        "--long-edge",
        "300",
        "--output",
        s(&out),
    ]);
    let hash = ok(&["hash", "--input", s(&out)]);
    assert_eq!(line, format!("300x225 {hash}"));
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli` / Expected: FAIL(서브커맨드 없음).

- [ ] **Step 3: 구현** — `engine/cli/src/main.rs`:

```rust
//! `engine-cli`: build-time answer generation, scoring and golden data (master plan A8).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use engine_core::buffer::{ImageF32, Rgba8};
use engine_core::decode::{decode_rgba8, encode_png};
use engine_core::hash::sha256_hex;
use engine_core::recipe::{Normalized, Recipe};
use engine_core::render::{render_rgba8, RenderContext};
use engine_core::{scoring_image, SCORING_LONG_EDGE};

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
    /// Print SHA-256 of the decoded RGBA8 pixels.
    Hash {
        #[arg(long)]
        input: PathBuf,
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
        Cmd::Hash { input } => println!("{}", sha256_hex(load_image(&input)?.data())),
    }
    Ok(())
}
```

- [ ] **Step 4: 통과 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli` / Expected: 4개 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-cli --all-targets -- -D warnings
git add engine/cli
git commit -m "feat(engine): CLI 이미지 명령 (fixture, prep-photo, render, scoring-image, hash)"
```

---

## Task 11b: CLI 정답 생성과 채점 (gen-answer · score)

**Files:**
- Modify: `engine/cli/src/main.rs`, `engine/cli/tests/cli.rs`

**Interfaces:**
- Produces: `gen-answer --input --recipe --seed --out-dir [--allow-any-size]`: 원본 장변이 2048이 아니면 오류, `answer_2048.png`·`answer_1024.png`·`meta.json`(시간 값 없음, 소요 시간은 stderr), stdout은 1024 해시. `score --original --answer(PNG만) --player [--region JSON|파일, 기본 full] --output`: `ScoringReference::new(원본, 정답)` → `score(플레이어, None)`, Score JSON 파일과 stdout 점수.
- CLI → wasm 계약 산출물: `gen_answer_pngs_and_meta_reproduce_the_scoring_input`이 2048×1365 원본으로 `gen-answer`·플레이어 `render`·`score`를 실행하고 결과를 `engine/target/cli-contract/`(`fixture.jpg`, `r.json`, `p.json`, `ans/{answer_2048.png, answer_1024.png, meta.json}`, `player.png`, `score.json`)에 남긴다. Task 13의 `cli_contract.test.mjs`가 이 파일들을 읽는다.

- [ ] **Step 1: 실패하는 테스트 작성** — `cli.rs` 끝에 추가:

```rust
// --- Task 11b: gen-answer and score ---

#[test]
fn original_scores_0_and_answer_scores_100() {
    let rec = r#"{"schema_version":1,"basic":{"exposure":0.8,"contrast":25,"temperature":30}}"#;
    let (dir, jpg, r) = fixture_with_recipe("e2e", rec);
    let ans = dir.join("ans");
    let args = [
        "gen-answer",
        "--input",
        s(&jpg),
        "--recipe",
        s(&r),
        "--seed",
        "1",
    ];
    let hash = ok(&[&args[..], &["--out-dir", s(&ans), "--allow-any-size"]].concat());
    assert_eq!(hash.len(), 64);
    let answer = ans.join("answer_2048.png");
    let score = |player: &Path, out: &str| {
        let out_path = dir.join(out);
        ok(&[
            "score",
            "--original",
            s(&jpg),
            "--answer",
            s(&answer),
            "--player",
            s(player),
            "--output",
            s(&out_path),
        ])
    };
    assert_eq!(score(&jpg, "s0.json"), "0");
    assert_eq!(score(&answer, "s1.json"), "100");
}

#[test]
fn gen_answer_pngs_and_meta_reproduce_the_scoring_input() {
    // 2048x1365 original → answer_2048.png, answer_1024.png, meta.json (AC-S1d, A4).
    // The files stay in engine/target/cli-contract: engine/wasm/test/cli_contract.test.mjs
    // reads them and must reproduce every hash and the Score JSON string.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/cli-contract");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    ok(&[
        "fixture",
        "--out-dir",
        s(&dir),
        "--width",
        "2048",
        "--height",
        "1365",
    ]);
    let r = dir.join("r.json");
    fs::write(
        &r,
        r#"{"schema_version":1,"basic":{"exposure":0.4,"vibrance":30}}"#,
    )
    .unwrap();
    let ans = dir.join("ans");
    let jpg = dir.join("fixture.jpg");
    let printed = ok(&[
        "gen-answer",
        "--input",
        s(&jpg),
        "--recipe",
        s(&r),
        "--seed",
        "9",
        "--out-dir",
        s(&ans),
    ]);
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(ans.join("meta.json")).unwrap()).unwrap();
    let a2048 = ans.join("answer_2048.png");
    let a1024 = ans.join("answer_1024.png");
    assert_eq!(
        ok(&["hash", "--input", s(&a2048)]),
        meta["answer_sha256_rgba8_2048"]
    );
    assert_eq!(
        ok(&["hash", "--input", s(&a1024)]),
        meta["answer_sha256_rgba8_1024"]
    );
    assert_eq!(printed, meta["answer_sha256_rgba8_1024"]);
    assert_eq!(
        (
            meta["scoring_width"].as_u64(),
            meta["scoring_height"].as_u64()
        ),
        (Some(1024), Some(683))
    );
    assert_eq!(meta["seed"], 9);
    assert!(
        meta.get("render_ms").is_none(),
        "meta.json must be deterministic"
    );
    // Re-deriving the scoring image from the saved 2048 PNG gives the saved 1024 PNG.
    let again = dir.join("again.png");
    let line = ok(&["scoring-image", "--input", s(&a2048), "--output", s(&again)]);
    assert_eq!(
        line,
        format!(
            "1024x683 {}",
            meta["answer_sha256_rgba8_1024"].as_str().unwrap()
        )
    );
    // A non-trivial player through the CLI: render → PNG → score against the saved answer PNG.
    let p = dir.join("p.json");
    fs::write(
        &p,
        r#"{"schema_version":1,"basic":{"exposure":0.2,"vibrance":10,"contrast":-5}}"#,
    )
    .unwrap();
    let player = dir.join("player.png");
    let player_hash = ok(&[
        "render",
        "--input",
        s(&jpg),
        "--recipe",
        s(&p),
        "--seed",
        "9",
        "--output",
        s(&player),
    ]);
    assert_eq!(ok(&["hash", "--input", s(&player)]), player_hash);
    let printed_score = ok(&[
        "score",
        "--original",
        s(&jpg),
        "--answer",
        s(&a2048),
        "--player",
        s(&player),
        "--output",
        s(&dir.join("score.json")),
    ]);
    let n: u8 = printed_score.parse().unwrap();
    assert!((1..=99).contains(&n), "{printed_score}");
}

#[test]
fn gen_answer_rejects_non_2048_originals_without_the_flag() {
    let (dir, jpg, r) = fixture_with_recipe("size", r#"{"schema_version":1}"#);
    let out = dir.join("ans");
    let e = err(&[
        "gen-answer",
        "--input",
        s(&jpg),
        "--recipe",
        s(&r),
        "--seed",
        "0",
        "--out-dir",
        s(&out),
    ]);
    assert!(e.contains("expected 2048"), "{e}");
}

#[test]
fn score_rejects_a_jpeg_answer() {
    let (dir, jpg, _) = fixture_with_recipe("jpeg-answer", r#"{"schema_version":1}"#);
    let out = dir.join("s.json");
    let e = err(&[
        "score",
        "--original",
        s(&jpg),
        "--answer",
        s(&jpg),
        "--player",
        s(&jpg),
        "--output",
        s(&out),
    ]);
    assert!(e.contains("lossless answer PNG"), "{e}");
}

#[test]
fn score_rejects_an_unknown_region_kind() {
    let (dir, jpg, _) = fixture_with_recipe("region", r#"{"schema_version":1}"#);
    let png = dir.join("fixture.png");
    let out = dir.join("s.json");
    let e = err(&[
        "score",
        "--original",
        s(&jpg),
        "--answer",
        s(&png),
        "--player",
        s(&jpg),
        "--output",
        s(&out),
        "--region",
        r#"{"kind":"masks"}"#,
    ]);
    assert!(e.contains("region"), "{e}");
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli` / Expected: 새 5개 FAIL.

- [ ] **Step 3: 구현** — `main.rs`를 다음과 같이 고친다(11a 본문은 유지).

(1) `use std::fs;`부터 `#[derive(Parser)]` 앞까지를 다음으로 교체(새 import와 상수):
```rust
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
```

(2) `enum Cmd`의 `Hash` 변형 앞에 추가:
```rust
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
```

(3) `fn main()` 앞에 추가:
```rust
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
```

(4) `main`의 `match`에서 `Cmd::Hash` 갈래 앞에 추가:
```rust
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
```

- [ ] **Step 4: 통과 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli` / Expected: 9개 PASS(debug에서 약 20초: 2048×1365 계약 테스트 포함). `engine/target/cli-contract/score.json`이 생긴다.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-cli --all-targets -- -D warnings
git add engine/cli
git commit -m "feat(engine): CLI gen-answer(2048 검증, PNG·meta 계약)·score(ScoringReference, PNG 정답)"
```

---

## Task 12a: 골든 도구와 버전 가드

**Files:**
- Create: `engine/cli/src/golden.rs`
- Modify: `engine/cli/src/main.rs`

**Interfaces:**
- Consumes: Task 11 `load_image`, `load_recipe`; core `ScoringReference`, `scoring_params`, `scoring_image`.
- Produces: `engine-cli golden write [--dir engine/golden]`(계산·속성 검사 후, 기존 파일 대비 미범프 변경이면 거부하고 파일을 건드리지 않음, 임시 파일→rename 원자 교체), `golden check [--dir] [--actual-out]`, `golden guard --base --head [--base-lock --head-lock]`, `unbumped_changes(&Expected,&Expected)->Vec<String>`, `crate_changes(base_lock,head_lock,&Expected,&Expected)->Vec<String>`.
- `expected.json` 형식: `{engine_version, scoring_version, schema_version, scoring_params, long_edge, images:{이름:{width,height,sha256_rgba8}}, cases:{이름:{answer_sha256, answer_scoring_sha256, scoring_width, scoring_height, players:{이름:{render_sha256, scoring_sha256, score_json}}}}, probes:{이름:{scoring_width, scoring_height, answer_scoring_sha256, player_scoring_sha256, score_json}}}`. `players`에는 예약 이름 `answer`·`original`도 들어간다. 플레이어마다 2048 렌더 해시·채점 이미지 해시·Score 문자열을 모두 저장한다(Score 통계가 같아도 픽셀이 바뀌면 잡는다).
- 과거 스냅숏 판독: `Expected`는 guard의 base와 `golden write`의 기존 파일도 읽으므로 현재 타입에 묶지 않는다. `scoring_params`는 타입 없는 JSON(`Params`, 숫자는 값으로 비교: JavaScript를 거치면 `2.0`이 `2`가 된다), 나중에 추가하는 필드는 `#[serde(default)]`(예: `probes`). 그래서 1b가 채점 매개변수를 추가·삭제해도 base를 읽을 수 있고, 범프 여부만 판정한다.
- 실패 보고서(`--actual-out`): 계산·`expected.json` 판독 실패는 `{"failure":{case,player,stage,error}}`(stage 예: `load player recipe`, `render player`, `score`, `read expected.json`), 불일치는 다시 계산한 `expected.json` 전체(그대로 diff 가능). stderr에는 불일치한 JSON 경로 목록.
- `cases.json` 형식: `{"images":{이름:경로}, "cases":[{name,image,answer,seed,eligible,players:{이름:{recipe, score:[min,max]}}}], "probes":[{name,width,height,shift,noise,score:[min,max]}]}`. 속성 검사: `eligible`이 선언과 일치, 적격 케이스는 정답 100·perfect·원본 0, 부적격은 0점, 플레이어·프로브 점수는 선언 범위 안, 플레이어 이름 `answer`·`original` 예약.
- 채점 프로브: 원본·정답·플레이어를 정수 공식(`probe_rgba8`, wasm 테스트에 같은 공식)으로 만들어 디코드·렌더를 거치지 않고 채점한다. 입력이 엔진 렌더와 무관하므로 프로브 결과가 바뀌면 그것은 채점 변경이고 `SCORING_VERSION` 범프가 필요하다. 이것이 "순수 채점 수식 변경을 `ENGINE_VERSION`만 올려 통과시키는" 구멍을 막는다. 프로브 비교는 Score 안의 버전 문자열을 빼고 한다(엔진 범프만으로는 걸리지 않게).
- 가드 규칙: 이미지·정답과 플레이어의 2048 렌더 → `ENGINE_VERSION`만 인정, 케이스의 채점 이미지·Score JSON·채점 크기 → 어느 버전이든, 프로브·`scoring_params`·`long_edge` → `SCORING_VERSION`만, 이미지·케이스·플레이어 삭제(이름 변경 포함) → 어느 버전이든 범프 필요, 프로브 삭제·이름 변경 → `SCORING_VERSION`만(엔진 범프로 프로브를 지워 채점 가드를 우회하지 못하게).
- 크레이트 버전 가드(마스터 §5.3): `PIXEL_CRATES` = `zune-jpeg`·`zune-core`·`png`·`fdeflate`·`miniz_oxide`(디코드, `ENGINE_VERSION`), `libm`(렌더와 채점 수학 모두, `ENGINE_VERSION`과 `SCORING_VERSION` 둘 다). `--base-lock`·`--head-lock`을 주면 두 `engine/Cargo.lock`에서 이 크레이트들의 버전 목록을 비교하고, 바뀌었는데 담당 버전이 그대로면 골든 출력이 같아도 거부한다. CI 가드 단계가 base 커밋의 `engine/Cargo.lock`을 넘긴다(Task 14).

- [ ] **Step 1: 실패하는 테스트 작성**: `golden.rs`를 Step 3 구현의 `fn` 본문 `todo!()`로 만들고(타입·`impl PartialEq for Params`·`impl Display for Failure`는 그대로) 아래 단위 테스트를 붙이고, `engine/cli/tests/cli.rs` 끝에 아래 통합 테스트(골든 write/check 계약)를 붙인다. `main.rs`에는 `use std::fs;` 위에 `mod golden;`, `enum Cmd`의 끝에 아래 변형, `match`에 `Cmd::Golden { cmd } => golden::run(cmd)?,`를 추가한다.

```rust
    /// Golden data: write, check or guard `engine/golden/expected.json`.
    Golden {
        #[command(subcommand)]
        cmd: golden::GoldenCmd,
    },
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn player(tag: &str) -> PlayerEntry {
        PlayerEntry {
            render_sha256: format!("render-{tag}"),
            scoring_sha256: format!("scoring-{tag}"),
            score_json: format!(
                "{{\"correction_score\":50,\"engine_version\":\"engine-0.1.0\",\"scoring_version\":\"scoring-0.1.0\",\"tag\":\"{tag}\"}}"
            ),
        }
    }

    fn sample() -> Expected {
        let case = CaseEntry {
            answer_sha256: "a".into(),
            answer_scoring_sha256: "b".into(),
            scoring_width: 4,
            scoring_height: 3,
            players: BTreeMap::from([("half".to_owned(), player("half"))]),
        };
        let probe = ProbeEntry {
            scoring_width: 4,
            scoring_height: 3,
            answer_scoring_sha256: "pa".into(),
            player_scoring_sha256: "pp".into(),
            score_json: player("probe").score_json,
        };
        Expected {
            engine_version: "engine-0.1.0".into(),
            scoring_version: "scoring-0.1.0".into(),
            schema_version: 1,
            scoring_params: Params(serde_json::to_value(scoring_params()).unwrap()),
            long_edge: 1024,
            images: BTreeMap::from([(
                "img".to_owned(),
                ImageEntry {
                    width: 4,
                    height: 3,
                    sha256_rgba8: "c".into(),
                },
            )]),
            cases: BTreeMap::from([("case".to_owned(), case)]),
            probes: BTreeMap::from([("probe".to_owned(), probe)]),
        }
    }

    fn case(e: &mut Expected) -> &mut CaseEntry {
        e.cases.get_mut("case").unwrap()
    }

    fn half(e: &mut Expected) -> &mut PlayerEntry {
        case(e).players.get_mut("half").unwrap()
    }

    fn probe(e: &mut Expected) -> &mut ProbeEntry {
        e.probes.get_mut("probe").unwrap()
    }

    fn params(e: &mut Expected) -> &mut serde_json::Map<String, Value> {
        e.scoring_params.0.as_object_mut().unwrap()
    }

    fn bump_engine(e: &mut Expected) {
        e.engine_version = "engine-0.2.0".into();
    }

    fn bump_scoring(e: &mut Expected) {
        e.scoring_version = "scoring-0.2.0".into();
    }

    fn problems(edit: impl FnOnce(&mut Expected)) -> usize {
        let mut head = sample();
        edit(&mut head);
        unbumped_changes(&sample(), &head).len()
    }

    #[test]
    fn identical_results_pass() {
        assert_eq!(problems(|_| {}), 0);
    }

    #[test]
    fn render_change_needs_engine_bump() {
        assert_eq!(problems(|h| case(h).answer_sha256 = "x".into()), 1);
        assert_eq!(
            problems(|h| {
                bump_engine(h);
                case(h).answer_sha256 = "x".into();
            }),
            0
        );
    }

    #[test]
    fn render_change_is_not_excused_by_a_scoring_bump() {
        let n = problems(|h| {
            bump_scoring(h);
            case(h).answer_sha256 = "x".into();
        });
        assert_eq!(n, 1);
    }

    #[test]
    fn player_render_change_needs_engine_bump_not_scoring_bump() {
        assert_eq!(problems(|h| half(h).render_sha256 = "x".into()), 1);
        assert_eq!(
            problems(|h| {
                bump_scoring(h);
                half(h).render_sha256 = "x".into();
            }),
            1
        );
        assert_eq!(
            problems(|h| {
                bump_engine(h);
                half(h).render_sha256 = "x".into();
            }),
            0
        );
    }

    #[test]
    fn player_scoring_image_change_needs_a_bump() {
        assert_eq!(problems(|h| half(h).scoring_sha256 = "x".into()), 1);
        assert_eq!(
            problems(|h| {
                bump_scoring(h);
                half(h).scoring_sha256 = "x".into();
            }),
            0
        );
    }

    #[test]
    fn image_change_needs_engine_bump() {
        assert_eq!(
            problems(|h| h.images.get_mut("img").unwrap().sha256_rgba8 = "x".into()),
            1
        );
    }

    #[test]
    fn score_change_needs_a_bump_and_either_bump_excuses_it() {
        let edit = |h: &mut Expected| half(h).score_json = "{}".into();
        assert_eq!(problems(edit), 1);
        assert_eq!(
            problems(|h| {
                bump_scoring(h);
                edit(h);
            }),
            0
        );
    }

    #[test]
    fn scoring_dimension_change_needs_a_bump() {
        assert_eq!(problems(|h| case(h).scoring_width = 5), 1);
    }

    #[test]
    fn scoring_params_change_needs_scoring_bump_even_with_engine_bump() {
        let n = problems(|h| {
            bump_engine(h);
            params(h).insert("t_tile".into(), json!(3.0));
        });
        assert_eq!(n, 1);
    }

    #[test]
    fn scoring_params_change_with_scoring_bump_passes() {
        let n = problems(|h| {
            bump_scoring(h);
            params(h).insert("t_tile".into(), json!(3.0));
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn added_scoring_param_needs_scoring_bump() {
        let add = |h: &mut Expected| {
            params(h).insert("leak_threshold".into(), json!(0.5));
        };
        assert_eq!(problems(add), 1);
        assert_eq!(
            problems(|h| {
                bump_scoring(h);
                add(h);
            }),
            0
        );
    }

    #[test]
    fn removed_scoring_param_needs_scoring_bump() {
        let remove = |h: &mut Expected| {
            params(h).remove("t_tile");
        };
        assert_eq!(problems(remove), 1);
        assert_eq!(
            problems(|h| {
                bump_scoring(h);
                remove(h);
            }),
            0
        );
    }

    #[test]
    fn snapshot_with_other_scoring_params_fields_still_parses() {
        // A base written by an older or newer build: unknown keys kept, missing keys allowed.
        let mut v = serde_json::to_value(sample()).unwrap();
        let p = v["scoring_params"].as_object_mut().unwrap();
        p.remove("t_tile");
        p.insert("retired_knob".into(), json!(7));
        let old: Expected = serde_json::from_value(v).unwrap();
        assert_eq!(unbumped_changes(&old, &sample()).len(), 1);
    }

    #[test]
    fn scoring_params_compare_numbers_by_value() {
        // expected.json rewritten by JavaScript has `"t_tile": 2` where serde wrote `2.0`.
        assert_eq!(
            problems(|h| {
                params(h).insert("t_tile".into(), json!(2));
            }),
            0
        );
    }

    #[test]
    fn snapshot_without_probes_still_parses() {
        let mut v = serde_json::to_value(sample()).unwrap();
        v.as_object_mut().unwrap().remove("probes");
        let old: Expected = serde_json::from_value(v).unwrap();
        assert!(old.probes.is_empty());
    }

    #[test]
    fn long_edge_change_needs_scoring_bump() {
        assert_eq!(problems(|h| h.long_edge = 512), 1);
    }

    #[test]
    fn pure_scoring_change_with_only_an_engine_bump_is_caught_by_the_probes() {
        // The hole AC-S1b closes: a scoring formula change shipped as an ENGINE bump.
        let n = problems(|h| {
            bump_engine(h);
            half(h).score_json = "{\"correction_score\":51}".into();
            probe(h).score_json = probe(h).score_json.replace(":50,", ":51,");
        });
        assert_eq!(n, 1);
        let n = problems(|h| {
            bump_scoring(h);
            probe(h).score_json = probe(h).score_json.replace(":50,", ":51,");
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn probe_scoring_image_change_needs_scoring_bump() {
        assert_eq!(
            problems(|h| {
                bump_engine(h);
                probe(h).player_scoring_sha256 = "x".into();
            }),
            1
        );
    }

    #[test]
    fn engine_bump_alone_does_not_trip_the_probes() {
        // Score JSON carries engine_version; the probe comparison ignores the version fields.
        let n = problems(|h| {
            bump_engine(h);
            probe(h).score_json = probe(h).score_json.replace("engine-0.1.0", "engine-0.2.0");
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn removed_case_player_image_or_probe_needs_a_bump() {
        assert_eq!(problems(|h| h.cases.clear()), 1);
        assert_eq!(problems(|h| case(h).players.clear()), 1);
        assert_eq!(problems(|h| h.images.clear()), 1);
        assert_eq!(problems(|h| h.probes.clear()), 1);
    }

    #[test]
    fn removed_or_renamed_probe_needs_scoring_bump_not_engine_bump() {
        let rename = |h: &mut Expected| {
            let p = h.probes.remove("probe").unwrap();
            h.probes.insert("renamed".into(), p);
        };
        let remove = |h: &mut Expected| h.probes.clear();
        for edit in [&rename as &dyn Fn(&mut Expected), &remove] {
            assert_eq!(
                problems(|h| {
                    bump_engine(h);
                    edit(h);
                }),
                1
            );
            assert_eq!(
                problems(|h| {
                    bump_scoring(h);
                    edit(h);
                }),
                0
            );
        }
    }

    const LOCK: &str = "[[package]]\nname = \"libm\"\nversion = \"0.2.16\"\n\n[[package]]\nname = \"png\"\nversion = \"0.18.1\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.229\"\n\n[[package]]\nname = \"zune-jpeg\"\nversion = \"0.5.15\"\n";

    fn crate_problems(from: &str, to: &str, edit: impl FnOnce(&mut Expected)) -> usize {
        let mut head = sample();
        edit(&mut head);
        crate_changes(LOCK, &LOCK.replace(from, to), &sample(), &head).len()
    }

    #[test]
    fn lock_versions_reads_every_version_of_a_crate() {
        let lock = format!("{LOCK}\n[[package]]\nname = \"png\"\nversion = \"0.17.0\"\n");
        assert_eq!(lock_versions(&lock, "png"), ["0.17.0", "0.18.1"]);
        assert!(lock_versions(LOCK, "fdeflate").is_empty());
    }

    #[test]
    fn decoder_version_change_with_same_golden_and_no_bump_fails() {
        let to = "name = \"zune-jpeg\"\nversion = \"0.5.16\"";
        let from = "name = \"zune-jpeg\"\nversion = \"0.5.15\"";
        assert_eq!(crate_problems(from, to, |_| {}), 1);
        assert_eq!(crate_problems(from, to, bump_scoring), 1);
        assert_eq!(crate_problems(from, to, bump_engine), 0);
    }

    #[test]
    fn libm_version_change_needs_both_bumps() {
        let (from, to) = ("version = \"0.2.16\"", "version = \"0.2.17\"");
        assert_eq!(crate_problems(from, to, bump_engine), 1);
        assert_eq!(crate_problems(from, to, bump_scoring), 1);
        assert_eq!(
            crate_problems(from, to, |h| {
                bump_engine(h);
                bump_scoring(h);
            }),
            0
        );
    }

    #[test]
    fn removed_pixel_crate_counts_as_a_change() {
        let png = "[[package]]\nname = \"png\"\nversion = \"0.18.1\"\n\n";
        assert_eq!(crate_problems(png, "", |_| {}), 1);
    }

    #[test]
    fn other_crate_updates_do_not_need_a_bump() {
        assert_eq!(crate_problems("1.0.229", "1.0.230", |_| {}), 0);
    }

    #[test]
    fn renamed_case_counts_as_removal() {
        let n = problems(|h| {
            let c = h.cases.remove("case").unwrap();
            h.cases.insert("renamed".into(), c);
        });
        assert_eq!(n, 1);
    }

    #[test]
    fn new_cases_players_and_probes_do_not_need_a_bump() {
        let n = problems(|h| {
            let extra = h.cases["case"].clone();
            h.cases.insert("new".into(), extra);
            case(h).players.insert("extra".into(), player("extra"));
            let p = h.probes["probe"].clone();
            h.probes.insert("new".into(), p);
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn failure_display_names_case_player_and_stage() {
        let f = at(Some("lake"), Some("b98"), "render player").fail("boom".into());
        assert_eq!(
            f.to_string(),
            "case lake / player b98 / render player: boom"
        );
    }

    #[test]
    fn differences_lists_changed_and_missing_paths() {
        let mut out = Vec::new();
        differences(
            &json!({"a": 1, "b": {"c": 2, "d": 3}}),
            &json!({"a": 1, "b": {"c": 9}, "e": 0}),
            "",
            &mut out,
        );
        assert_eq!(out, [".b.c", ".b.d", ".e"]);
    }
}
```

`engine/cli/tests/cli.rs` 끝에 추가:
```rust
// --- Task 12a: golden write/check contract ---

/// A tiny golden directory: one 64x48 image, one case with one player, one probe.
fn golden_dir(name: &str) -> PathBuf {
    let dir = tmp(name);
    ok(&[
        "fixture",
        "--out-dir",
        s(&dir),
        "--width",
        "64",
        "--height",
        "48",
    ]);
    fs::write(
        dir.join("answer.json"),
        r#"{"schema_version":1,"basic":{"exposure":1.0}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("half.json"),
        r#"{"schema_version":1,"basic":{"exposure":0.5}}"#,
    )
    .unwrap();
    let cases = r#"{
  "images": { "fx": "fixture.jpg" },
  "cases": [ { "name": "c", "image": "fx", "answer": "answer.json", "seed": 0, "eligible": true,
               "players": { "half": { "recipe": "half.json", "score": [1, 99] } } } ],
  "probes": [ { "name": "p", "width": 40, "height": 30, "shift": 40, "noise": 20, "score": [0, 100] } ]
}"#;
    fs::write(dir.join("cases.json"), cases).unwrap();
    ok(&["golden", "write", "--dir", s(&dir)]);
    dir
}

fn edit_expected(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = dir.join("expected.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    edit(&mut v);
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn golden_write_then_check_passes() {
    let dir = golden_dir("golden-ok");
    assert_eq!(
        ok(&["golden", "check", "--dir", s(&dir)]),
        "golden: ok (1 cases, 1 probes)"
    );
}

#[test]
fn golden_write_refuses_an_unbumped_change_and_keeps_the_file() {
    let dir = golden_dir("golden-refuse");
    edit_expected(&dir, |v| {
        v["cases"]["c"]["answer_sha256"] = "0".repeat(64).into();
    });
    let before = fs::read(dir.join("expected.json")).unwrap();
    let e = err(&["golden", "write", "--dir", s(&dir)]);
    assert!(
        e.contains("cases.c.answer_sha256 changed without ENGINE_VERSION bump"),
        "{e}"
    );
    assert_eq!(fs::read(dir.join("expected.json")).unwrap(), before);
    assert!(!dir.join("expected.json.tmp").exists());
}

#[test]
fn golden_check_reports_a_mismatch_with_the_actual_result() {
    let dir = golden_dir("golden-mismatch");
    let written = fs::read_to_string(dir.join("expected.json")).unwrap();
    edit_expected(&dir, |v| {
        v["cases"]["c"]["players"]["half"]["score_json"] = "{}".into();
    });
    let report = dir.join("actual.json");
    let e = err(&[
        "golden",
        "check",
        "--dir",
        s(&dir),
        "--actual-out",
        s(&report),
    ]);
    assert!(
        e.contains("golden mismatch") && e.contains(".cases.c.players.half.score_json"),
        "{e}"
    );
    // The report is the recomputed expected.json, byte for byte what golden write wrote.
    assert_eq!(fs::read_to_string(&report).unwrap(), written);
}

#[test]
fn golden_compute_failure_is_reported_with_context_and_writes_nothing() {
    let dir = golden_dir("golden-fail");
    fs::remove_file(dir.join("half.json")).unwrap();
    let before = fs::read(dir.join("expected.json")).unwrap();
    let report = dir.join("actual.json");
    let e = err(&[
        "golden",
        "check",
        "--dir",
        s(&dir),
        "--actual-out",
        s(&report),
    ]);
    assert!(
        e.contains("case c / player half / load player recipe"),
        "{e}"
    );
    let failure = &read_json(&report)["failure"];
    assert_eq!(
        (&failure["case"], &failure["player"], &failure["stage"]),
        (
            &serde_json::json!("c"),
            &serde_json::json!("half"),
            &serde_json::json!("load player recipe")
        )
    );
    let e = err(&["golden", "write", "--dir", s(&dir)]);
    assert!(
        e.contains("case c / player half / load player recipe"),
        "{e}"
    );
    assert_eq!(fs::read(dir.join("expected.json")).unwrap(), before);
}

#[test]
fn golden_check_reports_an_unreadable_expected_json() {
    let dir = golden_dir("golden-unreadable");
    fs::write(dir.join("expected.json"), "{ not json").unwrap();
    let report = dir.join("actual.json");
    let e = err(&[
        "golden",
        "check",
        "--dir",
        s(&dir),
        "--actual-out",
        s(&report),
    ]);
    assert!(e.contains("read expected.json"), "{e}");
    let failure = &read_json(&report)["failure"];
    assert_eq!(failure["stage"], "read expected.json");
    assert!(failure["case"].is_null(), "{failure}");
}
```

- [ ] **Step 2: 실패 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli golden` / Expected: FAIL(단위 테스트는 `todo!()` 패닉, 통합 테스트는 `golden write` 실패).

- [ ] **Step 3: 구현** — `engine/cli/src/golden.rs`:

```rust
//! Golden data (master plan §5.6). `cases.json` lists images, answer recipes, player recipes
//! with the score range each must land in, and scoring probes; `expected.json` stores hashes
//! and full Score JSON strings. Native (this module) and wasm (`engine/wasm/test/golden.test.mjs`)
//! must reproduce it exactly.
//!
//! Version ownership (enforced by [`unbumped_changes`]):
//! - decoded images and every 2048 render (answer and players) → `ENGINE_VERSION`
//! - scoring images, Score JSON and scoring sizes of cases → either version (a render change
//!   moves them too)
//! - probes, `scoring_params`, `long_edge` → `SCORING_VERSION` only. Probe inputs are generated
//!   from integer formulas, never decoded or rendered, so any change in a probe result is a
//!   scoring change.
//! - a removed (or renamed) image, case or player → any version bump; a removed (or renamed)
//!   probe → `SCORING_VERSION`
//! - a version change of a pixel-deciding crate in `engine/Cargo.lock` ([`PIXEL_CRATES`]) →
//!   the version that owns it, even when every golden output stays the same

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use engine_core::buffer::Rgba8;
use engine_core::hash::sha256_hex;
use engine_core::region::RegionSpec;
use engine_core::render::{render_rgba8, RenderContext};
use engine_core::score::{scoring_params, ScoringReference};
use engine_core::{
    scoring_image, ENGINE_VERSION, SCHEMA_VERSION, SCORING_LONG_EDGE, SCORING_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{load_image, load_recipe};

#[derive(Subcommand)]
pub(crate) enum GoldenCmd {
    /// Recompute and write `expected.json`. Refuses changes that keep the same versions.
    Write {
        #[arg(long, default_value = "engine/golden")]
        dir: PathBuf,
    },
    /// Recompute and compare with `expected.json`; on any failure write `--actual-out`.
    Check {
        #[arg(long, default_value = "engine/golden")]
        dir: PathBuf,
        #[arg(long)]
        actual_out: Option<PathBuf>,
    },
    /// Fail when `head` changes or drops results relative to `base` without a version bump.
    Guard {
        #[arg(long)]
        base: PathBuf,
        #[arg(long)]
        head: PathBuf,
        /// `engine/Cargo.lock` at the base commit (with `--head-lock`: crate version check).
        #[arg(long, requires = "head_lock")]
        base_lock: Option<PathBuf>,
        /// `engine/Cargo.lock` at the head commit.
        #[arg(long, requires = "base_lock")]
        head_lock: Option<PathBuf>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CaseFile {
    images: BTreeMap<String, String>,
    cases: Vec<Case>,
    probes: Vec<Probe>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    image: String,
    answer: String,
    seed: u32,
    /// Expected `D_E >= 3`. Ineligible cases document the rule (score 0, never 100).
    eligible: bool,
    players: BTreeMap<String, Player>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Player {
    recipe: String,
    /// Inclusive `[min, max]` the correction score must land in.
    score: [u8; 2],
}

/// A scoring-only input: original, answer and player are generated by [`probe_rgba8`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    name: String,
    width: u32,
    height: u32,
    /// Original = answer shifted by ±`shift` per channel.
    shift: u32,
    /// Player = answer plus pseudo-noise in `[-noise, noise]`.
    noise: u32,
    /// Inclusive `[min, max]` the correction score must land in.
    score: [u8; 2],
}

#[derive(Clone, Copy)]
enum ProbeRole {
    Original,
    Answer,
    Player,
}

/// Integer-only probe pixels; `engine/wasm/test/golden.test.mjs` has the same formula.
fn probe_rgba8(p: &Probe, role: ProbeRole) -> Result<Rgba8> {
    let (w, h) = (p.width, p.height);
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let answer = [
                x * 255 / w.saturating_sub(1).max(1),
                y * 255 / h.saturating_sub(1).max(1),
                ((x / 16 + y / 16) % 2) * 160 + 48,
            ];
            for (c, a) in (0u32..).zip(answer) {
                let a = i64::from(a);
                let v = match role {
                    ProbeRole::Answer => a,
                    ProbeRole::Original if c == 1 => a - i64::from(p.shift),
                    ProbeRole::Original => a + i64::from(p.shift),
                    ProbeRole::Player => {
                        let span = 2 * p.noise + 1;
                        a + i64::from((x * 31 + y * 17 + c * 7) % span) - i64::from(p.noise)
                    }
                };
                data.push(v.clamp(0, 255) as u8);
            }
            data.push(255);
        }
    }
    Ok(Rgba8::new(w, h, data)?)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ImageEntry {
    width: u32,
    height: u32,
    sha256_rgba8: String,
}

/// One submission's results: its 2048 render, its scoring image and its Score JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlayerEntry {
    /// SHA-256 of the full-resolution RGBA8 that was submitted.
    render_sha256: String,
    /// SHA-256 of its 1024 scoring image.
    scoring_sha256: String,
    /// Exact `serde_json::to_string(&Score)`.
    score_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CaseEntry {
    answer_sha256: String,
    answer_scoring_sha256: String,
    scoring_width: u32,
    scoring_height: u32,
    /// Player name → results. Includes the reserved `original` and `answer`.
    players: BTreeMap<String, PlayerEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProbeEntry {
    scoring_width: u32,
    scoring_height: u32,
    answer_scoring_sha256: String,
    player_scoring_sha256: String,
    score_json: String,
}

/// `scoring_params` as stored: untyped JSON, so snapshots written by other builds still parse.
/// Numbers compare by value (`5` equals `5.0`) because a round trip through JavaScript drops
/// the `.0` that serde writes for floats.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Params(Value);

impl PartialEq for Params {
    fn eq(&self, other: &Self) -> bool {
        json_eq(&self.0, &other.0)
    }
}

/// JSON equality with numbers compared as f64.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, w)| json_eq(v, w))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

/// `expected.json`. Also the reader for OLD snapshots (guard base, `golden write`), so it must
/// not depend on today's types: `scoring_params` stays untyped JSON, and fields added later
/// must be `#[serde(default)]` so older files still parse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Expected {
    engine_version: String,
    scoring_version: String,
    schema_version: u32,
    scoring_params: Params,
    long_edge: u32,
    images: BTreeMap<String, ImageEntry>,
    cases: BTreeMap<String, CaseEntry>,
    #[serde(default)]
    probes: BTreeMap<String, ProbeEntry>,
}

/// Where a golden computation failed. Written to `--actual-out` as `{"failure": …}`.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Failure {
    case: Option<String>,
    player: Option<String>,
    stage: &'static str,
    error: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let case = self.case.as_deref().unwrap_or("-");
        let player = self.player.as_deref().unwrap_or("-");
        write!(
            f,
            "case {case} / player {player} / {}: {}",
            self.stage, self.error
        )
    }
}

impl std::error::Error for Failure {}

/// Failure context for one step.
#[derive(Clone, Copy)]
struct At<'a> {
    case: Option<&'a str>,
    player: Option<&'a str>,
    stage: &'static str,
}

fn at<'a>(case: Option<&'a str>, player: Option<&'a str>, stage: &'static str) -> At<'a> {
    At {
        case,
        player,
        stage,
    }
}

impl At<'_> {
    fn fail(self, error: String) -> Failure {
        Failure {
            case: self.case.map(str::to_owned),
            player: self.player.map(str::to_owned),
            stage: self.stage,
            error,
        }
    }

    fn on<T, E: Into<anyhow::Error>>(self, r: Result<T, E>) -> Result<T, Failure> {
        r.map_err(|e| self.fail(format!("{:#}", e.into())))
    }

    fn check(self, ok: bool, msg: impl FnOnce() -> String) -> Result<(), Failure> {
        if ok {
            Ok(())
        } else {
            Err(self.fail(msg()))
        }
    }
}

fn entry(submitted: &Rgba8, score: &engine_core::score::Score) -> Result<PlayerEntry> {
    Ok(PlayerEntry {
        render_sha256: sha256_hex(submitted.data()),
        scoring_sha256: sha256_hex(scoring_image(submitted, SCORING_LONG_EDGE)?.data()),
        score_json: serde_json::to_string(score)?,
    })
}

fn compute(dir: &Path) -> Result<Expected, Failure> {
    let top = at(None, None, "read cases.json");
    let text = top.on(fs::read_to_string(dir.join("cases.json")))?;
    let spec: CaseFile = top.on(serde_json::from_str(&text))?;
    top.check(!spec.images.is_empty() && !spec.cases.is_empty(), || {
        "cases.json has no images or cases".to_owned()
    })?;

    let mut decoded = BTreeMap::new();
    let mut images = BTreeMap::new();
    for (name, file) in &spec.images {
        let img = at(None, None, "decode image").on(load_image(&dir.join(file)))?;
        images.insert(
            name.clone(),
            ImageEntry {
                width: img.width(),
                height: img.height(),
                sha256_rgba8: sha256_hex(img.data()),
            },
        );
        decoded.insert(name.clone(), img);
    }

    let mut cases = BTreeMap::new();
    for case in &spec.cases {
        let src = at(Some(&case.name), None, "image").on(decoded
            .get(&case.image)
            .ok_or_else(|| anyhow::anyhow!("unknown image `{}`", case.image)))?;
        cases.insert(case.name.clone(), compute_case(dir, case, src)?);
    }
    let mut probes = BTreeMap::new();
    for probe in &spec.probes {
        probes.insert(probe.name.clone(), compute_probe(probe)?);
    }

    let params = at(None, None, "scoring params").on(serde_json::to_value(scoring_params()))?;
    Ok(Expected {
        engine_version: ENGINE_VERSION.to_owned(),
        scoring_version: SCORING_VERSION.to_owned(),
        schema_version: SCHEMA_VERSION,
        scoring_params: Params(params),
        long_edge: SCORING_LONG_EDGE,
        images,
        cases,
        probes,
    })
}

/// Answer, original and every player of one case, with the property checks.
fn compute_case(dir: &Path, case: &Case, src: &Rgba8) -> Result<CaseEntry, Failure> {
    let full = RegionSpec::Full {};
    let n = case.name.as_str();
    let here = |stage| at(Some(n), None, stage);
    here("properties").check(!case.eligible || !case.players.is_empty(), || {
        "eligible cases need player recipes".to_owned()
    })?;
    let ctx = RenderContext { seed: case.seed };
    let recipe = here("load answer recipe").on(load_recipe(&dir.join(&case.answer)))?;
    let answer = here("render answer").on(render_rgba8(src, &recipe, &ctx))?;
    let reference = here("scoring reference").on(ScoringReference::new(src, &answer, &full))?;

    let s_answer = at(Some(n), Some("answer"), "score").on(reference.score(&answer, None))?;
    let s_original = at(Some(n), Some("original"), "score").on(reference.score(src, None))?;
    let props = here("properties");
    props.check(s_answer.eligible == case.eligible, || {
        format!("eligible={}", s_answer.eligible)
    })?;
    if case.eligible {
        props.check(s_answer.perfect && s_answer.correction_score == 100, || {
            "answer != 100".to_owned()
        })?;
        props.check(s_original.correction_score == 0, || {
            "original != 0".to_owned()
        })?;
    } else {
        props.check(s_answer.correction_score == 0, || {
            "ineligible must score 0".to_owned()
        })?;
    }
    let mut players = BTreeMap::new();
    let record = here("hash results");
    players.insert("answer".to_owned(), record.on(entry(&answer, &s_answer))?);
    players.insert("original".to_owned(), record.on(entry(src, &s_original))?);
    for (name, player) in &case.players {
        let p_at = |stage| at(Some(n), Some(name.as_str()), stage);
        p_at("properties").check(!players.contains_key(name), || {
            format!("player name `{name}` is reserved")
        })?;
        let recipe = p_at("load player recipe").on(load_recipe(&dir.join(&player.recipe)))?;
        let p = p_at("render player").on(render_rgba8(src, &recipe, &ctx))?;
        let s = p_at("score").on(reference.score(&p, None))?;
        let [lo, hi] = player.score;
        p_at("properties").check((lo..=hi).contains(&s.correction_score), || {
            format!("score {} outside [{lo}, {hi}]", s.correction_score)
        })?;
        players.insert(name.clone(), p_at("hash results").on(entry(&p, &s))?);
    }
    let a = reference.answer_scoring();
    Ok(CaseEntry {
        answer_sha256: sha256_hex(answer.data()),
        answer_scoring_sha256: sha256_hex(a.data()),
        scoring_width: a.width(),
        scoring_height: a.height(),
        players,
    })
}

/// One scoring probe: generated inputs, no decode and no render.
fn compute_probe(probe: &Probe) -> Result<ProbeEntry, Failure> {
    let full = RegionSpec::Full {};
    let here = |stage| at(None, Some(probe.name.as_str()), stage);
    let gen = here("generate probe");
    let original = gen.on(probe_rgba8(probe, ProbeRole::Original))?;
    let answer = gen.on(probe_rgba8(probe, ProbeRole::Answer))?;
    let player = gen.on(probe_rgba8(probe, ProbeRole::Player))?;
    let reference =
        here("scoring reference").on(ScoringReference::new(&original, &answer, &full))?;
    let s = here("score").on(reference.score(&player, None))?;
    let [lo, hi] = probe.score;
    here("properties").check((lo..=hi).contains(&s.correction_score), || {
        format!("score {} outside [{lo}, {hi}]", s.correction_score)
    })?;
    let player_scoring = here("scoring image").on(scoring_image(&player, SCORING_LONG_EDGE))?;
    let a = reference.answer_scoring();
    Ok(ProbeEntry {
        scoring_width: a.width(),
        scoring_height: a.height(),
        answer_scoring_sha256: sha256_hex(a.data()),
        player_scoring_sha256: sha256_hex(player_scoring.data()),
        score_json: here("serialize").on(serde_json::to_string(&s))?,
    })
}

fn read_expected(path: &Path) -> Result<Expected, Failure> {
    let here = at(None, None, "read expected.json");
    let text = here.on(fs::read_to_string(path))?;
    here.on(serde_json::from_str(&text))
}

/// Score JSON without its version fields: probes compare scores across an `ENGINE_VERSION`
/// bump, which rewrites `engine_version` inside every Score.
fn score_without_versions(json: &str) -> Value {
    let mut v: Value = serde_json::from_str(json).unwrap_or(Value::String(json.to_owned()));
    if let Value::Object(map) = &mut v {
        map.remove("engine_version");
        map.remove("scoring_version");
    }
    v
}

/// Results that changed or disappeared between `base` and `head` without the version bump
/// that owns them (see the module docs for the rules).
pub(crate) fn unbumped_changes(base: &Expected, head: &Expected) -> Vec<String> {
    let engine_same = base.engine_version == head.engine_version;
    let scoring_same = base.scoring_version == head.scoring_version;
    let any_same = engine_same && scoring_same;
    let mut problems = Vec::new();
    let mut flag = |cond: bool, what: String, need: &str| {
        if cond {
            problems.push(format!("{what} without {need} bump"));
        }
    };

    flag(
        scoring_same && base.scoring_params != head.scoring_params,
        "scoring_params changed".into(),
        "SCORING_VERSION",
    );
    flag(
        scoring_same && base.long_edge != head.long_edge,
        "long_edge changed".into(),
        "SCORING_VERSION",
    );
    for (name, b) in &base.images {
        match head.images.get(name) {
            None => flag(any_same, format!("images.{name} removed"), "a version"),
            Some(h) => flag(
                engine_same && h != b,
                format!("images.{name} changed"),
                "ENGINE_VERSION",
            ),
        }
    }
    for (name, b) in &base.cases {
        let Some(h) = head.cases.get(name) else {
            flag(any_same, format!("cases.{name} removed"), "a version");
            continue;
        };
        flag(
            engine_same && h.answer_sha256 != b.answer_sha256,
            format!("cases.{name}.answer_sha256 changed"),
            "ENGINE_VERSION",
        );
        let scoring_changed = h.answer_scoring_sha256 != b.answer_scoring_sha256
            || (h.scoring_width, h.scoring_height) != (b.scoring_width, b.scoring_height);
        flag(
            any_same && scoring_changed,
            format!("cases.{name} scoring image changed"),
            "a version",
        );
        for (player, bp) in &b.players {
            let path = format!("cases.{name}.players.{player}");
            let Some(hp) = h.players.get(player) else {
                flag(any_same, format!("{path} removed"), "a version");
                continue;
            };
            flag(
                engine_same && hp.render_sha256 != bp.render_sha256,
                format!("{path}.render_sha256 changed"),
                "ENGINE_VERSION",
            );
            flag(
                any_same && hp.scoring_sha256 != bp.scoring_sha256,
                format!("{path}.scoring_sha256 changed"),
                "a version",
            );
            flag(
                any_same && hp.score_json != bp.score_json,
                format!("{path}.score_json changed"),
                "a version",
            );
        }
    }
    for (name, b) in &base.probes {
        let Some(h) = head.probes.get(name) else {
            flag(
                scoring_same,
                format!("probes.{name} removed"),
                "SCORING_VERSION",
            );
            continue;
        };
        let changed = (h.scoring_width, h.scoring_height) != (b.scoring_width, b.scoring_height)
            || h.answer_scoring_sha256 != b.answer_scoring_sha256
            || h.player_scoring_sha256 != b.player_scoring_sha256
            || score_without_versions(&h.score_json) != score_without_versions(&b.score_json);
        flag(
            scoring_same && changed,
            format!("probes.{name} changed"),
            "SCORING_VERSION",
        );
    }
    problems
}

/// Which version owns a crate's output.
#[derive(Clone, Copy)]
enum Owner {
    /// Decoding: changes decoded pixels and every render.
    Engine,
    /// Used by both the render and the scoring math.
    Both,
}

/// Crates whose code decides pixel or score values (master plan §5.3). Their version in
/// `engine/Cargo.lock` may only change together with the owning version bump.
const PIXEL_CRATES: [(&str, Owner); 6] = [
    ("zune-jpeg", Owner::Engine),
    ("zune-core", Owner::Engine),
    ("png", Owner::Engine),
    ("fdeflate", Owner::Engine),
    ("miniz_oxide", Owner::Engine),
    ("libm", Owner::Both),
];

/// Sorted versions of `name` in a Cargo.lock (several versions can coexist).
fn lock_versions(lock: &str, name: &str) -> Vec<String> {
    let wanted = format!("name = \"{name}\"");
    let mut lines = lock.lines().map(str::trim);
    let mut versions = Vec::new();
    while let Some(line) = lines.next() {
        if line == wanted {
            if let Some(v) = lines
                .next()
                .and_then(|l| l.strip_prefix("version = \""))
                .and_then(|l| l.strip_suffix('"'))
            {
                versions.push(v.to_owned());
            }
        }
    }
    versions.sort();
    versions
}

/// [`PIXEL_CRATES`] whose locked version changed between `base` and `head` without the
/// owning version bump. Catches dependency updates that keep today's golden outputs.
pub(crate) fn crate_changes(
    base_lock: &str,
    head_lock: &str,
    base: &Expected,
    head: &Expected,
) -> Vec<String> {
    let engine_same = base.engine_version == head.engine_version;
    let scoring_same = base.scoring_version == head.scoring_version;
    PIXEL_CRATES
        .iter()
        .filter_map(|&(name, owner)| {
            let (b, h) = (
                lock_versions(base_lock, name),
                lock_versions(head_lock, name),
            );
            let need = match owner {
                Owner::Engine if engine_same => "ENGINE_VERSION",
                Owner::Both if engine_same || scoring_same => "ENGINE_VERSION and SCORING_VERSION",
                _ => return None,
            };
            (b != h).then(|| {
                format!(
                    "Cargo.lock {name} {} -> {} without {need} bump",
                    b.join(","),
                    h.join(",")
                )
            })
        })
        .collect()
}

/// JSON paths where `actual` differs from `expected` (for the failure log).
fn differences(expected: &Value, actual: &Value, path: &str, out: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for key in e.keys().chain(a.keys().filter(|k| !e.contains_key(*k))) {
                let p = format!("{path}.{key}");
                match (e.get(key), a.get(key)) {
                    (Some(ev), Some(av)) => differences(ev, av, &p, out),
                    _ => out.push(p),
                }
            }
        }
        _ if !json_eq(expected, actual) => out.push(path.to_owned()),
        _ => {}
    }
}

fn write_atomically(path: &Path, value: &Expected) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(value)? + "\n")?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn check(dir: &Path, actual_out: Option<&Path>) -> Result<()> {
    let report = |text: String| -> Result<()> {
        if let Some(out) = actual_out {
            fs::write(out, text)?;
            eprintln!("actual written to {}", out.display());
        }
        Ok(())
    };
    let failure = |f: Failure| -> Result<()> {
        report(serde_json::to_string_pretty(
            &serde_json::json!({ "failure": &f }),
        )?)?;
        Err(f.into())
    };
    let actual = match compute(dir) {
        Ok(a) => a,
        Err(f) => return failure(f),
    };
    let expected = match read_expected(&dir.join("expected.json")) {
        Ok(e) => e,
        Err(f) => return failure(f),
    };
    if actual != expected {
        report(serde_json::to_string_pretty(&actual)? + "\n")?;
        let mut diff = Vec::new();
        differences(
            &serde_json::to_value(&expected)?,
            &serde_json::to_value(&actual)?,
            "",
            &mut diff,
        );
        bail!(
            "golden mismatch: engine output differs from expected.json at\n{}",
            diff.join("\n")
        );
    }
    println!(
        "golden: ok ({} cases, {} probes)",
        expected.cases.len(),
        expected.probes.len()
    );
    Ok(())
}

pub(crate) fn run(cmd: GoldenCmd) -> Result<()> {
    match cmd {
        GoldenCmd::Write { dir } => {
            let head = compute(&dir)?;
            let path = dir.join("expected.json");
            if path.exists() {
                let problems = unbumped_changes(&read_expected(&path)?, &head);
                if !problems.is_empty() {
                    bail!("refusing to write:\n{}", problems.join("\n"));
                }
            }
            write_atomically(&path, &head)?;
            println!("wrote {}", path.display());
        }
        GoldenCmd::Check { dir, actual_out } => check(&dir, actual_out.as_deref())?,
        GoldenCmd::Guard {
            base,
            head,
            base_lock,
            head_lock,
        } => {
            let (base, head) = (read_expected(&base)?, read_expected(&head)?);
            let mut problems = unbumped_changes(&base, &head);
            if let (Some(bl), Some(hl)) = (base_lock, head_lock) {
                let read = |p: &Path| {
                    fs::read_to_string(p).with_context(|| format!("read {}", p.display()))
                };
                problems.extend(crate_changes(&read(&bl)?, &read(&hl)?, &base, &head));
            }
            if !problems.is_empty() {
                bail!("{}", problems.join("\n"));
            }
            println!("golden guard: ok");
        }
    }
    Ok(())
}
```

- [ ] **Step 4: 통과 확인**: Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli golden` / Expected: 단위 30개와 통합 5개(`golden_*`) PASS. 참고(검증 실행): 실제 수식 변이로도 확인했다. `provisional_diag`의 반올림을 내림으로 바꾸고 `ENGINE_VERSION`만 올리면 `golden write`가 `probes.noisy_1100x700 changed without SCORING_VERSION bump`로 거부하고, `SCORING_VERSION`을 올리면 통과한다.

- [ ] **Step 5: fmt·clippy·커밋** (데이터는 다음 태스크에서 따로 커밋한다)

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-cli --all-targets -- -D warnings
git add engine/cli
git commit -m "feat(engine): golden write/check/guard (플레이어 해시·채점 프로브, 미범프 변경·삭제 거부, 문맥 있는 실패 보고서)"
```

---

## Task 12b: 골든 데이터 (CC0 실사진, 레시피, cases.json, expected.json)

각 블록은 독립적으로 실행된다(셸 변수 공유 없음). 내려받은 원본과 중간 파일은 `engine/target/golden-src/`(git 무시)에 둔다. 여러 명령을 묶은 블록은 `bash -euo pipefail <<'SH'`로 실행해 첫 실패에서 멈추고 `exit=1`을 남긴다. 호출한 셸이 `&&` 목록 안이면 `( set -e; … )`는 `set -e`가 무시되어 실패 뒤에도 계속 진행한다(확인함).

**TDD와 예외:** 이 태스크의 코드인 `verify-sources.mjs`는 Step 1에서 테스트 먼저(RED→GREEN)로 만든다. 예외는 데이터 생성(Step 2~5)뿐이다. 판정 로직은 Task 12a가 RED→GREEN으로 검증했고, 데이터 단계의 RED는 Step 5의 속성 검사(선언 범위 밖 점수는 `golden write`가 거부)와 Step 6의 변조 거부 확인이 대신한다.

**Files:**
- Create: `engine/golden/{sources.json, verify-sources.mjs, cases.json, expected.json}`, `engine/golden/test/verify-sources.test.mjs`, `engine/golden/images/*.jpg`(4), `engine/golden/recipes/*.json`(11)

- [ ] **Step 1: 출처 검증 스크립트(TDD)와 출처 기록**

(a) 실패하는 테스트. 임시 git 저장소를 만들어 정상, 누락 파일, 잘못된 해시, 미기입 generator, 없는 커밋, 커밋의 `Cargo.lock`과 다른 해시, 내려받은 원본의 SHA-1 검사를 확인한다. `engine/golden/test/verify-sources.test.mjs`:
```js
// Tests for engine/golden/verify-sources.mjs on a throwaway git repository:
//   node --test engine/golden/test/verify-sources.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../verify-sources.mjs')
const sha = (algo, data) => createHash(algo).update(data).digest('hex')
const LOCK = '# lockfile\nversion = 4\n'
const IMAGE = 'pixels'

/** A git repo with engine/Cargo.lock committed and a golden dir whose sources.json is `edit(spec)`. */
function fixture(edit = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'verify-sources-'))
  const g = (...a) => execFileSync('git', ['-C', root, ...a], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()
  g('init', '-q')
  mkdirSync(join(root, 'engine/golden/images'), { recursive: true })
  writeFileSync(join(root, 'engine/Cargo.lock'), LOCK)
  g('add', '.')
  g('-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'lock')
  writeFileSync(join(root, 'engine/golden/images/a.jpg'), IMAGE)
  const spec = {
    generator: { commit: g('rev-parse', 'HEAD'), cargo_lock_sha256: sha('sha256', LOCK) },
    files: { 'images/a.jpg': { sha256: sha('sha256', IMAGE) } },
    sources: { a: { sha1: sha('sha1', IMAGE), sha256: sha('sha256', IMAGE) } },
  }
  edit(spec, root)
  writeFileSync(join(root, 'engine/golden/sources.json'), JSON.stringify(spec))
  return root
}

function run(root, ...extra) {
  const r = spawnSync(process.execPath, [script, '--dir', join(root, 'engine/golden'), ...extra], { encoding: 'utf8' })
  rmSync(root, { recursive: true, force: true })
  return { code: r.status, out: r.stdout + r.stderr }
}

test('passes for intact files and a verifiable generator record', () => {
  const r = run(fixture())
  assert.equal(r.code, 0, r.out)
  assert.match(r.out, /ok images\/a\.jpg/)
  assert.match(r.out, /ok generator [0-9a-f]{12}/)
})

test('fails on a missing file', () => {
  const r = run(fixture((s) => (s.files['images/b.jpg'] = { sha256: '0'.repeat(64) })))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISSING images\/b\.jpg/)
})

test('fails on a wrong file hash', () => {
  const r = run(fixture((s) => (s.files['images/a.jpg'].sha256 = '0'.repeat(64))))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISMATCH images\/a\.jpg sha256/)
})

test('fails on an unfilled generator record', () => {
  const r = run(fixture((s) => (s.generator.commit = 'PENDING')))
  assert.equal(r.code, 1)
  assert.match(r.out, /UNFILLED generator record/)
})

test('fails on a generator commit that does not exist', () => {
  const r = run(fixture((s) => (s.generator.commit = '0'.repeat(40))))
  assert.equal(r.code, 1)
  assert.match(r.out, /UNKNOWN generator commit/)
})

test('fails when the lockfile hash does not match the generator commit', () => {
  const r = run(fixture((s) => (s.generator.cargo_lock_sha256 = '0'.repeat(64))))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISMATCH generator cargo_lock_sha256/)
})

test('checks a downloaded source by SHA-1 and SHA-256', () => {
  let root = fixture()
  const ok = run(root, 'a', join(root, 'engine/golden/images/a.jpg'))
  assert.equal(ok.code, 0, ok.out)
  root = fixture((s) => (s.sources.a.sha1 = '0'.repeat(40)))
  const bad = run(root, 'a', join(root, 'engine/golden/images/a.jpg'))
  assert.equal(bad.code, 1)
  assert.match(bad.out, /MISMATCH source a sha1/)
})
```

(b) RED: `engine/golden/verify-sources.mjs`를 `process.exit(0)` 한 줄 스텁으로 두고 실행한다.
```bash
node --test engine/golden/test/verify-sources.test.mjs
```
Expected: 7개 모두 FAIL(정상 경우는 `ok` 출력이 없어서, 나머지는 exit 0이라서). 파일이 없어서 실패하는 것이 아니라 동작이 없어서 실패한다(확인함).

(c) 구현: 아래 `sources.json`과 `verify-sources.mjs`.

`engine/golden/sources.json`(라이선스는 Wikimedia Commons API `extmetadata.LicenseShortName == "CC0"`로 2026-10-04 확인, 리비전은 `imageinfo.timestamp`). `generator`는 가공 파일(`fixture`, `prep-photo` 산출물)을 만든 engine-cli의 커밋과 그 커밋의 `engine/Cargo.lock` SHA-256이며 Step 3이 채운다. 채우기 전에는 `verify-sources.mjs`가 실패한다. 검증은 형식만 보지 않는다: 커밋이 이 저장소에 있어야 하고(`git cat-file`), `git show <commit>:engine/Cargo.lock`의 해시가 기록과 같아야 한다. 현재 HEAD의 lockfile과 비교하지 않으므로 이후의 정상 의존성 갱신은 막지 않는다. CI checkout은 `fetch-depth: 0`(01a-1 Task 0)이라 커밋을 찾을 수 있다. 분할 4 PR은 squash하지 않고 merge commit으로 병합한다(squash하면 generator 커밋이 `main` 이력에서 사라진다):
```json
{
  "license_check": "Wikimedia Commons API extmetadata.LicenseShortName == \"CC0\" (CC0 1.0), read 2026-10-04",
  "generator": {
    "about": "engine-cli build that produced the processed files below (fixture, prep-photo)",
    "commit": "PENDING",
    "cargo_lock_sha256": "PENDING"
  },
  "files": {
    "images/flowers_1600x1200_baseline422.jpg": {
      "sha256": "9402a680387806d83fc98118a4a432225740bae3f4ee22eb7b5f8119b4e97657",
      "source": "flowers",
      "processing": "none: original bytes (camera baseline JPEG, 4:2:2, EXIF orientation 1)"
    },
    "images/portrait_1530x2040_progressive420.jpg": {
      "sha256": "44dd6ec29078e88ae2f11af9433f2d527109bd9dbed8a7308b566e1cbc21b2e8",
      "source": "portrait",
      "processing": "none: original bytes (progressive JPEG, 4:2:0)"
    },
    "images/lake_2048x1365_baseline420.jpg": {
      "sha256": "11a19957737f821fa2cedc488ed4663048e7f74358d7935e151c92f5aa562a0f",
      "source": "lake",
      "processing": "engine-cli prep-photo --input <lake source> --long-edge 2048 --quality 90 --output images/lake_2048x1365_baseline420.jpg"
    },
    "images/synthetic_512x384_baseline420.jpg": {
      "sha256": "db96b85ade61f3ae2a2604453c31bbaaf83cc2cf946164348ee68cf5d4bce0f8",
      "source": null,
      "processing": "engine-cli fixture --out-dir <tmp> --width 512 --height 384, then fixture.jpg"
    }
  },
  "sources": {
    "flowers": {
      "page": "https://commons.wikimedia.org/wiki/File:Garden_flowers_in_Thouars_10.jpg",
      "url": "https://upload.wikimedia.org/wikipedia/commons/d/d3/Garden_flowers_in_Thouars_10.jpg",
      "author": "Syced",
      "revision": "2022-02-05T08:58:21Z",
      "sha1": "e83ca40ce0bcfcda41ae1c81e47bc68982419a61",
      "sha256": "9402a680387806d83fc98118a4a432225740bae3f4ee22eb7b5f8119b4e97657"
    },
    "portrait": {
      "page": "https://commons.wikimedia.org/wiki/File:Photo_of_Anthony_van_Dyck%27s_%22Portrait_of_Cornelis_van_der_Geest%22.jpg",
      "url": "https://upload.wikimedia.org/wikipedia/commons/b/b1/Photo_of_Anthony_van_Dyck%27s_%22Portrait_of_Cornelis_van_der_Geest%22.jpg",
      "author": "PotatoCow25",
      "revision": "2026-02-09T23:34:10Z",
      "sha1": "ebd47d04f96b04cfc7dbd00833022ad7c20aeeaa",
      "sha256": "44dd6ec29078e88ae2f11af9433f2d527109bd9dbed8a7308b566e1cbc21b2e8"
    },
    "lake": {
      "page": "https://commons.wikimedia.org/wiki/File:Lake_Mountain_Landscape.jpg",
      "url": "https://upload.wikimedia.org/wikipedia/commons/7/75/Lake_Mountain_Landscape.jpg",
      "author": "Bonnie Moreland (via isorepublic.com)",
      "revision": "2022-05-03T14:24:14Z",
      "sha1": "60eab74e7704db6375f1426fb11421ba29d7603e",
      "sha256": "32c63883fbedbd2da1590245b1ccaa55bdb26ffa63bf3328456202356ed307d6"
    }
  }
}
```

`engine/golden/verify-sources.mjs`:
```js
// Verifies golden image provenance against <dir>/sources.json (default dir: this file's folder).
//   node engine/golden/verify-sources.mjs [--dir <golden dir>]
//       committed files (SHA-256) and the generator record: the commit must exist in this
//       repository and its engine/Cargo.lock must hash to cargo_lock_sha256
//   node engine/golden/verify-sources.mjs [--dir <golden dir>] <name> <path>
//       a downloaded source (SHA-1 + SHA-256)
// Exits 1 on any mismatch, missing file, unfilled or unverifiable generator record.
// The generator check needs the full history (CI checks out with fetch-depth: 0).
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const args = process.argv.slice(2)
let dir = dirname(fileURLToPath(import.meta.url))
if (args[0] === '--dir') {
  dir = resolve(args[1])
  args.splice(0, 2)
}
const spec = JSON.parse(readFileSync(resolve(dir, 'sources.json'), 'utf8'))
const digest = (algo, bytes) => createHash(algo).update(bytes).digest('hex')
let failed = false
const fail = (msg) => {
  console.error(msg)
  failed = true
}
const check = (label, path, want) => {
  if (!existsSync(path)) return fail(`MISSING ${label}: ${path}`)
  const bytes = readFileSync(path)
  let ok = true
  for (const [algo, value] of Object.entries(want)) {
    const got = digest(algo, bytes)
    if (got !== value) {
      fail(`MISMATCH ${label} ${algo}: got ${got}, want ${value}`)
      ok = false
    }
  }
  if (ok) console.log(`ok ${label}`)
}

/** git output as bytes, or null when the command fails. */
const git = (...gitArgs) => {
  try {
    return execFileSync('git', ['-C', dir, ...gitArgs], { stdio: ['ignore', 'pipe', 'ignore'] })
  } catch {
    return null
  }
}

/** Provenance of the processed files: which engine-cli commit and lockfile produced them. */
const checkGenerator = (gen = {}) => {
  const { commit = '', cargo_lock_sha256: lockSha = '' } = gen
  if (!/^[0-9a-f]{40}$/.test(commit) || !/^[0-9a-f]{64}$/.test(lockSha)) {
    return fail(`UNFILLED generator record: commit=${commit} cargo_lock_sha256=${lockSha}`)
  }
  if (!git('cat-file', '-e', `${commit}^{commit}`)) {
    return fail(`UNKNOWN generator commit ${commit} (not in this repository; shallow clone?)`)
  }
  const lock = git('show', `${commit}:engine/Cargo.lock`)
  if (!lock) return fail(`MISSING generator engine/Cargo.lock at ${commit}`)
  const got = digest('sha256', lock)
  if (got !== lockSha) return fail(`MISMATCH generator cargo_lock_sha256: got ${got} at ${commit}, want ${lockSha}`)
  console.log(`ok generator ${commit.slice(0, 12)}`)
}

const [name, path] = args
if (name) {
  const src = spec.sources[name]
  if (!src) throw new Error(`unknown source ${name}`)
  check(`source ${name}`, resolve(path), { sha1: src.sha1, sha256: src.sha256 })
} else {
  for (const [file, meta] of Object.entries(spec.files)) {
    check(file, resolve(dir, file), { sha256: meta.sha256 })
  }
  checkGenerator(spec.generator)
}
process.exit(failed ? 1 : 0)
```

(d) GREEN:
```bash
node --test engine/golden/test/verify-sources.test.mjs
```
Expected: 7개 PASS. 실제 `node engine/golden/verify-sources.mjs`는 이미지와 generator가 아직 없으므로 Step 3 전까지 실패하는 것이 정상이다.

- [ ] **Step 2: CC0 사진 받기와 자동 검증**

```bash
bash -euo pipefail <<'SH'
WORK=engine/target/golden-src
mkdir -p "$WORK" engine/golden/images engine/golden/recipes
UA="retouch-golden/0.1 (https://github.com/gwanryo/retouch)"
get() { curl --fail --location --show-error --silent -A "$UA" -o "$2" "$1"; }
get "https://upload.wikimedia.org/wikipedia/commons/d/d3/Garden_flowers_in_Thouars_10.jpg" engine/golden/images/flowers_1600x1200_baseline422.jpg
get "https://upload.wikimedia.org/wikipedia/commons/b/b1/Photo_of_Anthony_van_Dyck%27s_%22Portrait_of_Cornelis_van_der_Geest%22.jpg" engine/golden/images/portrait_1530x2040_progressive420.jpg
get "https://upload.wikimedia.org/wikipedia/commons/7/75/Lake_Mountain_Landscape.jpg" "$WORK/lake.jpg"
node engine/golden/verify-sources.mjs flowers engine/golden/images/flowers_1600x1200_baseline422.jpg
node engine/golden/verify-sources.mjs portrait engine/golden/images/portrait_1530x2040_progressive420.jpg
node engine/golden/verify-sources.mjs lake "$WORK/lake.jpg"
SH
echo "exit=$?"
```
Expected: `ok source flowers`, `ok source portrait`, `ok source lake`, `exit=0`. 다운로드나 검증이 하나라도 실패하면 블록이 그 자리에서 멈추고 `exit=1`이다(파일이 바뀐 것, 중단).

- [ ] **Step 3: 합성·가공 픽스처 생성과 검증**

```bash
bash -euo pipefail <<'SH'
WORK=engine/target/golden-src
# The generator record must name a committed engine-cli: no uncommitted engine changes.
git diff --quiet HEAD -- engine/core engine/cli engine/Cargo.toml engine/Cargo.lock
cargo build --locked --manifest-path engine/Cargo.toml --release -p engine-cli
CLI=engine/target/release/engine-cli
$CLI fixture --out-dir "$WORK" --width 512 --height 384
cp "$WORK/fixture.jpg" engine/golden/images/synthetic_512x384_baseline420.jpg
$CLI prep-photo --input "$WORK/lake.jpg" --long-edge 2048 --quality 90 --output engine/golden/images/lake_2048x1365_baseline420.jpg
node -e '
  const fs = require("fs"), { createHash } = require("crypto"), { execSync } = require("child_process")
  const f = "engine/golden/sources.json", s = JSON.parse(fs.readFileSync(f, "utf8"))
  s.generator.commit = execSync("git rev-parse HEAD").toString().trim()
  s.generator.cargo_lock_sha256 = createHash("sha256").update(execSync("git show HEAD:engine/Cargo.lock")).digest("hex")
  fs.writeFileSync(f, JSON.stringify(s, null, 2) + "\n")'
node engine/golden/verify-sources.mjs
SH
echo "exit=$?"
```
Expected: `prep-photo`가 `2048x1365`, 검증 4줄 모두 `ok`, `ok generator <커밋 앞 12자리>`, `exit=0`(이 두 파일의 SHA-256은 GNU·MSVC 빌드에서 같았다). `generator.commit`은 Task 12a 커밋(HEAD)이고, `cargo_lock_sha256`은 그 커밋에 저장된 `engine/Cargo.lock` 바이트(작업 트리 줄바꿈과 무관)의 해시다. 엔진 코드에 커밋되지 않은 변경이 있으면 첫 `git diff --quiet`에서 멈춘다. 원본 lake.jpg(2.9MB, progressive 4:4:4)는 커밋하지 않는다.

- [ ] **Step 4: 레시피와 cases.json**

`engine/golden/recipes/cool_fade.json`:
```json
{ "schema_version": 1, "basic": { "exposure": -0.2, "contrast": -25, "blacks": 30, "temperature": -30, "tint": -10, "saturation": -35 } }
```

`engine/golden/recipes/exposure_plus1.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 1.0 } }
```

`engine/golden/recipes/identity.json`:
```json
{ "schema_version": 1 }
```

`engine/golden/recipes/player_cool_half.json`:
```json
{ "schema_version": 1, "basic": { "contrast": -10, "temperature": -15, "saturation": -15 } }
```

`engine/golden/recipes/player_exposure_half.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.5 } }
```

`engine/golden/recipes/player_warm_b100.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.32, "contrast": 35, "highlights": -40, "shadows": 25, "whites": 10, "blacks": -15, "temperature": 35, "tint": 8, "vibrance": 20, "saturation": -10 } }
```

`engine/golden/recipes/player_warm_b98.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.39, "contrast": 35, "highlights": -40, "shadows": 25, "whites": 10, "blacks": -15, "temperature": 35, "tint": 8, "vibrance": 20, "saturation": -10 } }
```

`engine/golden/recipes/player_warm_b99.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.36, "contrast": 35, "highlights": -40, "shadows": 25, "whites": 10, "blacks": -15, "temperature": 35, "tint": 8, "vibrance": 20, "saturation": -10 } }
```

`engine/golden/recipes/player_warm_half.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.15, "contrast": 15, "temperature": 15, "vibrance": 10 } }
```

`engine/golden/recipes/player_warm_near.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.25, "contrast": 30, "highlights": -30, "shadows": 20, "temperature": 30, "tint": 5, "vibrance": 15, "saturation": -5 } }
```

`engine/golden/recipes/warm_contrast.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.3, "contrast": 35, "highlights": -40, "shadows": 25, "whites": 10, "blacks": -15, "temperature": 35, "tint": 8, "vibrance": 20, "saturation": -10 } }
```

`player_warm_b98/b99/b100`은 `lake_warm`의 경계 사례다(검증 실행: b98은 raw 98.03, b99는 raw 100이지만 타일 p95 2.35 > `T_TILE`, b100은 ΔE 0.39로 perfect). 프로브 4개는 만점(100), 타일 게이트만 실패(99, 1100×700 → 1024×652 비정수 축소), 중간 점수, 부적격(0)을 덮는다.

`engine/golden/cases.json`:
```json
{
  "images": {
    "synthetic": "images/synthetic_512x384_baseline420.jpg",
    "lake": "images/lake_2048x1365_baseline420.jpg",
    "flowers": "images/flowers_1600x1200_baseline422.jpg",
    "portrait": "images/portrait_1530x2040_progressive420.jpg"
  },
  "cases": [
    { "name": "synthetic_identity", "image": "synthetic", "answer": "recipes/identity.json", "seed": 0, "eligible": false, "players": {} },
    { "name": "synthetic_exposure", "image": "synthetic", "answer": "recipes/exposure_plus1.json", "seed": 1, "eligible": true,
      "players": { "half": { "recipe": "recipes/player_exposure_half.json", "score": [1, 99] } } },
    { "name": "lake_warm", "image": "lake", "answer": "recipes/warm_contrast.json", "seed": 2, "eligible": true,
      "players": {
        "half": { "recipe": "recipes/player_warm_half.json", "score": [1, 99] },
        "near": { "recipe": "recipes/player_warm_near.json", "score": [1, 99] },
        "b98": { "recipe": "recipes/player_warm_b98.json", "score": [98, 98] },
        "b99": { "recipe": "recipes/player_warm_b99.json", "score": [99, 99] },
        "b100": { "recipe": "recipes/player_warm_b100.json", "score": [100, 100] }
      } },
    { "name": "flowers_cool", "image": "flowers", "answer": "recipes/cool_fade.json", "seed": 3, "eligible": true,
      "players": { "half": { "recipe": "recipes/player_cool_half.json", "score": [1, 99] } } },
    { "name": "portrait_bright", "image": "portrait", "answer": "recipes/exposure_plus1.json", "seed": 4, "eligible": true,
      "players": { "half": { "recipe": "recipes/player_exposure_half.json", "score": [1, 99] } } }
  ],
  "probes": [
    { "name": "perfect_300x200", "width": 300, "height": 200, "shift": 40, "noise": 1, "score": [100, 100] },
    { "name": "tile_gate_1100x700", "width": 1100, "height": 700, "shift": 40, "noise": 5, "score": [99, 99] },
    { "name": "noisy_1100x700", "width": 1100, "height": 700, "shift": 40, "noise": 20, "score": [1, 98] },
    { "name": "ineligible_64x48", "width": 64, "height": 48, "shift": 1, "noise": 3, "score": [0, 0] }
  ]
}
```

- [ ] **Step 5: 속성 검사를 통과하며 골든 생성**

```bash
cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden write --dir engine/golden
cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden check --dir engine/golden
cargo run --locked --manifest-path engine/Cargo.toml -p engine-cli -- golden check --dir engine/golden
```
Expected: `wrote engine/golden/expected.json`, release·debug 모두 `golden: ok (5 cases, 4 probes)`. release는 약 25초, debug는 약 3분 30초 걸린다(검증 실행). 참고값(검증 실행): 채점 크기 lake 1024×683, flowers 1024×768, portrait 768×1024, synthetic 512×384. 점수 lake_warm half=68·near=96·b98=98·b99=99·b100=100, flowers_cool half=42, portrait_bright half=53, synthetic_exposure half=56, synthetic_identity는 eligible=false·0. 프로브 perfect_300x200=100, tile_gate_1100x700=99(raw 100, 타일 p95 4.29), noisy_1100x700=85, ineligible_64x48=0(`d_e_original` 0.54). 범위를 벗어나면 `golden write`가 `case … / player … / properties: score … outside […]`로 알리고 실패한다.

- [ ] **Step 6: guard가 실제로 거부하는지 확인**

```bash
TMP=$(mktemp -d)
CLI=engine/target/release/engine-cli
node -e 'const fs=require("fs");const e=JSON.parse(fs.readFileSync("engine/golden/expected.json","utf8"));e.cases.lake_warm.players.b98.render_sha256="0".repeat(64);fs.writeFileSync(process.argv[1],JSON.stringify(e))' "$TMP/tampered.json"
$CLI golden guard --base engine/golden/expected.json --head "$TMP/tampered.json"; echo "exit=$?"
node -e 'const fs=require("fs");fs.writeFileSync(process.argv[1],fs.readFileSync("engine/Cargo.lock","utf8").replace(/(name = "zune-jpeg"\r?\nversion = ")[^"]+/,(m,k)=>k+"9.9.9"))' "$TMP/head.lock"
$CLI golden guard --base engine/golden/expected.json --head engine/golden/expected.json --base-lock engine/Cargo.lock --head-lock "$TMP/head.lock"; echo "exit=$?"
```
Expected: `Error: cases.lake_warm.players.b98.render_sha256 changed without ENGINE_VERSION bump`, `exit=1`(플레이어 렌더 해시도 가드 대상). 이어서 `Error: Cargo.lock zune-jpeg 0.5.15 -> 9.9.9 without ENGINE_VERSION bump`, `exit=1`(골든이 그대로여도 크레이트 버전만 바뀌면 거부). JavaScript로 다시 쓴 파일은 `2.0`이 `2`가 되지만 `scoring_params`는 값으로 비교하므로 오탐이 없다(확인함).

- [ ] **Step 7: 커밋**

```bash
git add engine/golden
git commit -m "test(engine): CC0 실사 골든 데이터 (경계 98·99·100, 출처·해시 검증)"
```

---

## Task 13: wasm 골든 테스트와 성능 기준선 스크립트

**Files:**
- Create: `engine/wasm/test/golden.test.mjs`, `engine/wasm/test/cli_contract.test.mjs`, `engine/wasm/bench/perf.mjs`

**Interfaces:**
- Consumes: Task 10 wasm API, Task 11b `engine/target/cli-contract/`, Task 12b `cases.json`·`expected.json`.
- Produces: `golden.test.mjs`는 wasm으로 `expected.json`과 같은 객체(플레이어별 렌더·채점 이미지 해시와 Score 문자열, 프로브 포함)를 계산해 `deepStrictEqual`. 실패 시 `engine/golden/actual-wasm.json`에 불일치면 계산 결과 전체, 예외(`expected.json` 판독 실패 포함)면 `{"failure":{case,player,stage,error}}`. `cli_contract.test.mjs`는 CLI가 쓴 원본·정답 PNG 2개·`meta.json`·플레이어 PNG·Score JSON을 읽어 디코드 해시, 정답 렌더, 2048→1024 재축소 해시, 플레이어 렌더, Score 문자열을 대조한다. 모든 `ImageData`·`ScoringReference`는 `try/finally`로 `free()`한다. `perf.mjs`: 로드(원본 JPEG 디코드 → CLI가 쓴 정답 PNG 디코드 → 참조 생성, A8 경로), 참조를 재사용하는 제출, 챌린지 전환(로드+제출+해제 반복, 메모리 증가 확인)을 따로 잰다. 각 워밍업 1회 + 7회, p50/p95.

- [ ] **Step 1: 테스트와 스크립트 작성**

`engine/wasm/test/golden.test.mjs`:
```js
// wasm half of AC-E1a: the wasm build must reproduce engine/golden/expected.json exactly
// (hashes of every render and scoring image, and full Score JSON strings). On any failure the
// wasm result (mismatch) or `{"failure":{case,player,stage,error}}` (exception, including an
// unreadable expected.json) is written to engine/golden/actual-wasm.json for the CI artifact.
// Never copy it into expected.json.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const golden = resolve(here, '../../golden')
const actualPath = resolve(golden, 'actual-wasm.json')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (p) => readFileSync(resolve(golden, p), 'utf8')
const FULL = '{"kind":"full"}'

class GoldenFailure extends Error {
  constructor(failure) {
    super(`case ${failure.case ?? '-'} / player ${failure.player ?? '-'} / ${failure.stage}: ${failure.error}`)
    this.failure = failure
  }
}

/** Run `f`; on an exception, rethrow it with its {case, player, stage} context. */
const at = (ctx, stage, f) => {
  try {
    return f()
  } catch (err) {
    if (err instanceof GoldenFailure) throw err
    throw new GoldenFailure({ case: ctx.case ?? null, player: ctx.player ?? null, stage, error: String(err?.message ?? err) })
  }
}

/** Copy an engine-owned ImageData out once and free it (each `.rgba` read copies). */
const take = (img) => {
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

const sha = (bytes) => wasm.sha256_hex(bytes)
const scoringSha = (w, h, rgba) => sha(take(wasm.scoring_image(w, h, rgba, 1024)).rgba)

/** Same integer formula as `probe_rgba8` in engine/cli/src/golden.rs. */
function probeRgba(p, role) {
  const { width: w, height: h } = p
  const out = new Uint8Array(w * h * 4)
  let i = 0
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const answer = [
        Math.floor((x * 255) / Math.max(w - 1, 1)),
        Math.floor((y * 255) / Math.max(h - 1, 1)),
        ((Math.floor(x / 16) + Math.floor(y / 16)) % 2) * 160 + 48,
      ]
      for (let c = 0; c < 3; c++) {
        const a = answer[c]
        let v = a
        if (role === 'original') v = c === 1 ? a - p.shift : a + p.shift
        if (role === 'player') v = a + ((x * 31 + y * 17 + c * 7) % (2 * p.noise + 1)) - p.noise
        out[i++] = Math.min(255, Math.max(0, v))
      }
      out[i++] = 255
    }
  }
  return out
}

function computeCase(c, src) {
  const ctx = { case: c.name }
  const { width: w, height: h, rgba } = src
  const render = (pctx, recipe) =>
    at(pctx, 'render', () => wasm.render_rgba8(w, h, rgba, read(recipe), c.seed))
  const answer = render({ ...ctx, player: 'answer' }, c.answer)
  const reference = at(ctx, 'scoring reference', () => new wasm.ScoringReference(w, h, rgba, answer, FULL))
  try {
    const players = {}
    const submit = (player, pixels) => {
      const pctx = { ...ctx, player }
      players[player] = {
        render_sha256: sha(pixels),
        scoring_sha256: at(pctx, 'scoring image', () => scoringSha(w, h, pixels)),
        score_json: at(pctx, 'score', () => reference.score(pixels, null)),
      }
    }
    submit('answer', answer)
    submit('original', rgba)
    for (const [player, p] of Object.entries(c.players)) submit(player, render({ ...ctx, player }, p.recipe))
    const a = take(reference.answer_scoring())
    return {
      answer_sha256: sha(answer),
      answer_scoring_sha256: sha(a.rgba),
      scoring_width: a.width,
      scoring_height: a.height,
      players: Object.fromEntries(Object.entries(players).sort(([x], [y]) => (x < y ? -1 : 1))),
    }
  } finally {
    reference.free()
  }
}

function computeProbe(p) {
  const ctx = { player: p.name }
  const [original, answer, player] = ['original', 'answer', 'player'].map((role) => probeRgba(p, role))
  const reference = at(ctx, 'scoring reference', () => new wasm.ScoringReference(p.width, p.height, original, answer, FULL))
  try {
    const a = take(reference.answer_scoring())
    return {
      scoring_width: a.width,
      scoring_height: a.height,
      answer_scoring_sha256: sha(a.rgba),
      player_scoring_sha256: at(ctx, 'scoring image', () => scoringSha(p.width, p.height, player)),
      score_json: at(ctx, 'score', () => reference.score(player, null)),
    }
  } finally {
    reference.free()
  }
}

function compute(spec) {
  const v = JSON.parse(wasm.versions())
  const out = {
    engine_version: v.engine,
    scoring_version: v.scoring,
    schema_version: v.schema,
    scoring_params: v.scoring_params,
    long_edge: v.scoring_params.long_edge,
    images: {},
    cases: {},
    probes: {},
  }
  const decoded = {}
  for (const [name, file] of Object.entries(spec.images)) {
    const img = at({}, 'decode image', () => take(wasm.decode_image(new Uint8Array(readFileSync(resolve(golden, file))))))
    decoded[name] = img
    out.images[name] = { width: img.width, height: img.height, sha256_rgba8: sha(img.rgba) }
  }
  for (const c of spec.cases) out.cases[c.name] = computeCase(c, decoded[c.image])
  for (const p of spec.probes) out.probes[p.name] = computeProbe(p)
  // Key order of expected.json (serde BTreeMap): sorted.
  for (const k of ['images', 'cases', 'probes']) out[k] = Object.fromEntries(Object.entries(out[k]).sort(([x], [y]) => (x < y ? -1 : 1)))
  return out
}

test('wasm reproduces engine/golden/expected.json exactly', () => {
  let actual
  try {
    const spec = at({}, 'read cases.json', () => JSON.parse(read('cases.json')))
    actual = compute(spec)
    const expected = at({}, 'read expected.json', () => JSON.parse(read('expected.json')))
    assert.deepStrictEqual(actual, expected)
  } catch (err) {
    const report = err instanceof GoldenFailure ? { failure: err.failure } : actual ?? { failure: { case: null, player: null, stage: 'unknown', error: String(err) } }
    writeFileSync(actualPath, JSON.stringify(report, null, 2) + '\n')
    throw err
  }
})
```

`engine/wasm/test/cli_contract.test.mjs`:
```js
// CLI -> files -> wasm (plan 01a-4 Task 13): the files `engine-cli` wrote in
// engine/target/cli-contract (by `cargo test -p engine-cli --test cli`,
// gen_answer_pngs_and_meta_reproduce_the_scoring_input) must be reproduced by the wasm build:
// decode, answer render, 2048 -> 1024 re-derivation, player render and the Score JSON string.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const dir = resolve(here, '../../target/cli-contract')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const bytes = (name) => new Uint8Array(readFileSync(resolve(dir, name)))
const text = (name) => readFileSync(resolve(dir, name), 'utf8')

/** Copy an engine-owned ImageData out once and free it (each `.rgba` read copies). */
const take = (img) => {
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

test('wasm reproduces the CLI gen-answer, render and score files', () => {
  assert.ok(existsSync(resolve(dir, 'score.json')), `${dir} incomplete: run cargo test -p engine-cli --test cli first`)
  const meta = JSON.parse(text('ans/meta.json'))
  const original = take(wasm.decode_image(bytes('fixture.jpg')))
  const { width: w, height: h } = original
  assert.equal(wasm.sha256_hex(original.rgba), meta.original_sha256_rgba8, 'original decode')

  const answer = take(wasm.decode_image(bytes('ans/answer_2048.png')))
  assert.equal(wasm.sha256_hex(answer.rgba), meta.answer_sha256_rgba8_2048, 'answer_2048.png')
  const rendered = wasm.render_rgba8(w, h, original.rgba, text('r.json'), meta.seed)
  assert.equal(wasm.sha256_hex(rendered), meta.answer_sha256_rgba8_2048, 'wasm answer render')

  const small = take(wasm.decode_image(bytes('ans/answer_1024.png')))
  assert.equal(wasm.sha256_hex(small.rgba), meta.answer_sha256_rgba8_1024, 'answer_1024.png')
  const again = take(wasm.scoring_image(w, h, answer.rgba, 1024))
  assert.deepEqual([again.width, again.height], [meta.scoring_width, meta.scoring_height])
  assert.equal(wasm.sha256_hex(again.rgba), meta.answer_sha256_rgba8_1024, '2048 -> 1024 re-derivation')

  const player = take(wasm.decode_image(bytes('player.png')))
  const playerRender = wasm.render_rgba8(w, h, original.rgba, text('p.json'), meta.seed)
  assert.equal(wasm.sha256_hex(playerRender), wasm.sha256_hex(player.rgba), 'player render')

  const reference = new wasm.ScoringReference(w, h, original.rgba, answer.rgba, '{"kind":"full"}')
  try {
    assert.equal(wasm.sha256_hex(take(reference.answer_scoring()).rgba), meta.answer_sha256_rgba8_1024, 'reference scoring image')
    assert.equal(reference.score(player.rgba, null), text('score.json'), 'Score JSON string')
  } finally {
    reference.free()
  }
})
```

`engine/wasm/bench/perf.mjs`:
```js
// Performance baseline for AC-S5a (Plan 1a measures only; the budget gate is Plan 1b's first
// task). Three measurements, each with one warm-up run and RUNS timed runs, p50/p95 by nearest
// rank:
//   load    challenge load as in A8: decode the 2048 original JPEG, decode the answer PNG that
//           engine-cli wrote, build the ScoringReference
//   submit  one ScoringReference reused: render a 2048 player, score it
//   switch  load + one submit + free, repeated; wasm linear memory must not keep growing
// Every ImageData and ScoringReference is freed in `finally`, so memory numbers are working
// memory, not garbage waiting for finalizers.
// Usage (repository root):
//   wasm-pack build engine/wasm --target nodejs --release -- --locked
//   cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer \
//     --input engine/golden/images/lake_2048x1365_baseline420.jpg \
//     --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf
//   node engine/wasm/bench/perf.mjs
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const RUNS = 7
const here = dirname(fileURLToPath(import.meta.url))
const engine = resolve(here, '../..')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (p) => new Uint8Array(readFileSync(resolve(engine, p)))
const answerPng = 'target/perf/answer_2048.png'
if (!existsSync(resolve(engine, answerPng))) throw new Error(`engine/${answerPng} missing: run engine-cli gen-answer first (see header)`)
const jpeg = read('golden/images/lake_2048x1365_baseline420.jpg')
const png = read(answerPng)
const playerRecipe = readFileSync(resolve(engine, 'golden/recipes/player_warm_near.json'), 'utf8')
const MiB = (bytes) => (bytes / 2 ** 20).toFixed(1)

const times = {}
const time = (label, f) => {
  const t0 = performance.now()
  const r = f()
  ;(times[label] ??= []).push(performance.now() - t0)
  return r
}

/** Decode with the engine, copy the pixels out once, free the wasm object. */
const decode = (bytes) => {
  const img = wasm.decode_image(bytes)
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

/** Challenge load (A8). Returns the reference and the original pixels the submit step needs. */
const load = () =>
  time('load: total', () => {
    const src = time('load: decode original jpeg', () => decode(jpeg))
    const answer = time('load: decode answer png', () => decode(png))
    const reference = time('load: ScoringReference::new', () =>
      new wasm.ScoringReference(src.width, src.height, src.rgba, answer.rgba, '{"kind":"full"}'))
    return { src, reference }
  })

const submit = ({ src, reference }) =>
  time('submit: total', () => {
    const player = time('submit: render player 2048', () =>
      wasm.render_rgba8(src.width, src.height, src.rgba, playerRecipe, 0))
    return time('submit: score', () => reference.score(player, null))
  })

const repeat = (f) => {
  f() // warm-up (JIT, memory growth); not recorded
  for (const k of Object.keys(times)) delete times[k]
  for (let i = 0; i < RUNS; i++) f()
  return { ...times }
}

const freeAfter = (f) => () => {
  const challenge = load()
  try {
    f(challenge)
  } finally {
    challenge.reference.free()
  }
}

const results = {}
Object.assign(results, repeat(freeAfter(() => {})))
const memAfterLoad = wasm.memory_bytes()
const challenge = load()
try {
  Object.assign(results, repeat(() => submit(challenge)))
} finally {
  challenge.reference.free()
}
const memBeforeSwitch = wasm.memory_bytes()
const switchTimes = repeat(() => time('switch: load+submit+free', freeAfter(submit)))
results['switch: load+submit+free'] = switchTimes['switch: load+submit+free']
const memAfterSwitch = wasm.memory_bytes()

const pct = (xs, p) => {
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.max(1, Math.ceil(p * s.length)) - 1]
}
console.log(`runs=${RUNS} node=${process.version}`)
for (const [label, xs] of Object.entries(results)) {
  console.log(`${label.padEnd(30)} p50 ${pct(xs, 0.5).toFixed(0).padStart(5)} ms   p95 ${pct(xs, 0.95).toFixed(0).padStart(5)} ms`)
}
console.log(`wasm memory after load runs    ${MiB(memAfterLoad)} MiB`)
console.log(`wasm memory after submit runs  ${MiB(memBeforeSwitch)} MiB`)
console.log(`wasm memory after switch runs  ${MiB(memAfterSwitch)} MiB (growth during switch ${MiB(memAfterSwitch - memBeforeSwitch)} MiB)`)
console.log(`process rss                    ${MiB(process.memoryUsage().rss)} MiB`)
```

- [ ] **Step 2: 실행**

```bash
cargo test --locked --manifest-path engine/Cargo.toml -p engine-cli --test cli
wasm-pack build engine/wasm --target nodejs --release -- --locked
node --test "engine/wasm/test/*.test.mjs"
```
Expected: `✔ wasm reproduces the CLI gen-answer, render and score files`, `✔ wasm reproduces engine/golden/expected.json exactly` 포함 8개 통과(골든과 CLI 산출물이 이미 있으므로 처음부터 통과가 정상이고, 실패는 곧 결정성 위반이다). 골든 테스트는 약 30초 걸린다.

- [ ] **Step 3: RED - 고의 불일치가 잡히고 보고서가 남는지 확인**

```bash
TMP=$(mktemp -d)
cp engine/golden/expected.json "$TMP/expected.bak"
node -e 'const fs=require("fs");const f="engine/golden/expected.json";const e=JSON.parse(fs.readFileSync(f,"utf8"));const p=e.cases.lake_warm.players;p.b98.score_json=p.b99.score_json;fs.writeFileSync(f,JSON.stringify(e,null,2)+"\n")'
node --test engine/wasm/test/golden.test.mjs; echo "exit=$?"
ls engine/golden/actual-wasm.json
cp "$TMP/expected.bak" engine/golden/expected.json && rm engine/golden/actual-wasm.json
cp engine/target/cli-contract/score.json "$TMP/score.bak"
node -e 'const fs=require("fs");const f="engine/target/cli-contract/score.json";fs.writeFileSync(f,fs.readFileSync(f,"utf8").replace(/("correction_score":)\d+/,(m,k)=>k+1))'
node --test engine/wasm/test/cli_contract.test.mjs; echo "exit=$?"
cp "$TMP/score.bak" engine/target/cli-contract/score.json
git diff --exit-code -- engine/golden
node --test engine/wasm/test/golden.test.mjs engine/wasm/test/cli_contract.test.mjs
```
Expected: `✖ wasm reproduces …`·`exit=1`·`actual-wasm.json` 존재, `✖ wasm reproduces the CLI …`(`Score JSON string`)·`exit=1`, 복구 후 diff 없음, 마지막 재실행은 2개 `✔`(확인함). `expected.json`을 깨진 JSON으로 바꾸면 `actual-wasm.json`이 `{"failure":{"stage":"read expected.json",…}}`가 된다(확인함).

- [ ] **Step 4: 성능 기준선**

```bash
cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer --input engine/golden/images/lake_2048x1365_baseline420.jpg --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf
node engine/wasm/bench/perf.mjs
```
Expected: 단계별 p50/p95와 메모리 출력, `growth during switch 0.0 MiB`. 참고값(i7-10700K, Windows, Node 24.16, MSVC 툴체인 빌드 wasm, 2026-10-05): `load: total` p50 1031ms(그중 `ScoringReference::new` 945ms, 정답 PNG 디코드 50ms), `submit: total` p50 1767·p95 1776ms(렌더 1088 + 채점 678), `switch` p50 2805ms, wasm 메모리 140.8MiB(로드만 108.8MiB), 전환 7회 동안 증가 0. 같은 날 같은 PC에서 해제 누락이 있던 이전 스크립트는 제출 p50 1786ms, 메모리 223.5MiB였다. 즉 223.5MiB 중 약 83MiB는 해제하지 않은 `ImageData`였고, 이전 기록 2038ms와의 차이는 측정 환경 차이다.

- [ ] **Step 5: 커밋**

```bash
git add engine/wasm/test/golden.test.mjs engine/wasm/test/cli_contract.test.mjs engine/wasm/bench/perf.mjs
git commit -m "test(engine): wasm 골든 완전 일치 테스트와 성능 기준선 스크립트(p50/p95·메모리)"
```

---

## Task 14: CI 골든·가드 단계, 성능 기록, PR

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `engine/golden/PERF.md`

- [ ] **Step 1: CI 골든 단계 추가** — engine 잡의 `      # >>> golden steps (Plan 1a-4 Task 14)` 줄을 아래로 바꾼다. 가드는 PR이면 base SHA, push면 `before`, 새 브랜치(zero SHA)면 `merge-base origin/main`과 비교한다. base 커밋을 찾지 못하면 실패하고, base에 `expected.json`이 없을 때(최초 도입)만 건너뛴다. 가드는 base 커밋의 `engine/Cargo.lock`도 받아 픽셀 결정 크레이트의 버전 변경을 검사한다. 테스트 파일 존재 검사가 wasm·출처 테스트 파일 누락을 잡는다.

```yaml
      - name: Golden test files present
        shell: bash
        run: |
          # The wasm glob step must not silently lose a test file (e.g. one left out of a commit).
          for f in engine/wasm/test/smoke.test.mjs engine/wasm/test/golden.test.mjs engine/wasm/test/cli_contract.test.mjs engine/golden/test/verify-sources.test.mjs; do
            test -f "$f" || { echo "missing $f"; exit 1; }
          done
      - name: Golden sources script tests
        run: node --test engine/golden/test/verify-sources.test.mjs
      - name: Golden sources intact (needs fetch-depth 0 for the generator commit)
        run: node engine/golden/verify-sources.mjs
      - name: Golden (native, debug)
        run: cargo run --locked --manifest-path engine/Cargo.toml -p engine-cli -- golden check --dir engine/golden --actual-out engine/golden/actual-native-debug.json
      - name: Golden (native, release)
        run: cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden check --dir engine/golden --actual-out engine/golden/actual-native-release.json
      - name: Golden version guard (vs base)
        if: matrix.os == 'ubuntu-latest'
        shell: bash
        env:
          PR_BASE: ${{ github.event.pull_request.base.sha }}
          PUSH_BEFORE: ${{ github.event.before }}
        run: |
          set -euo pipefail
          BASE="${PR_BASE:-${PUSH_BEFORE:-}}"
          if [ -z "$BASE" ] || [ "$BASE" = "0000000000000000000000000000000000000000" ]; then
            # New branch push: compare with the default branch.
            git fetch --no-tags origin main
            BASE=$(git merge-base origin/main HEAD)
          fi
          git cat-file -e "$BASE^{commit}" # unreachable base is an error, not a pass
          if git cat-file -e "$BASE:engine/golden/expected.json" 2>/dev/null; then
            git show "$BASE:engine/golden/expected.json" > "$RUNNER_TEMP/base-expected.json"
            git show "$BASE:engine/Cargo.lock" > "$RUNNER_TEMP/base-Cargo.lock"
            cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- \
              golden guard --base "$RUNNER_TEMP/base-expected.json" --head engine/golden/expected.json \
              --base-lock "$RUNNER_TEMP/base-Cargo.lock" --head-lock engine/Cargo.lock
          else
            echo "expected.json does not exist at $BASE: first introduction, guard not applicable"
          fi
      - name: Upload golden failure reports
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: golden-actual-${{ matrix.os }}
          path: engine/golden/actual-*.json
          if-no-files-found: ignore
          retention-days: 7
```

- [ ] **Step 2: RED - CI가 쓰는 명령이 고의 불일치를 잡는지 확인**

```bash
TMP=$(mktemp -d)
cp engine/golden/expected.json "$TMP/expected.bak"
node -e 'const fs=require("fs");const f="engine/golden/expected.json";const e=JSON.parse(fs.readFileSync(f,"utf8"));const p=e.cases.lake_warm.players;p.b98.score_json=p.b99.score_json;fs.writeFileSync(f,JSON.stringify(e,null,2)+"\n")'
cargo run -q --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden check --dir engine/golden --actual-out engine/golden/actual-native-release.json; echo "exit=$?"
cp "$TMP/expected.bak" engine/golden/expected.json && rm engine/golden/actual-native-release.json
git diff --exit-code -- engine/golden
```
Expected: `actual written to …`, `Error: golden mismatch: engine output differs from expected.json at` 다음 줄 `.cases.lake_warm.players.b98.score_json` 하나만, `exit=1`, 복구 후 diff 없음(확인함).

- [ ] **Step 3: 로컬 전체 검증**

```bash
bash scripts/lint-determinism.sh
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path engine/Cargo.toml --workspace --all-targets -- -D warnings
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-wasm --all-targets --target wasm32-unknown-unknown -- -D warnings
cargo test --locked --manifest-path engine/Cargo.toml --workspace
cargo run --locked --manifest-path engine/Cargo.toml -p engine-cli -- golden check --dir engine/golden
cargo run --locked --manifest-path engine/Cargo.toml --release -p engine-cli -- golden check --dir engine/golden
node --test engine/golden/test/verify-sources.test.mjs
node engine/golden/verify-sources.mjs
wasm-pack test --node engine/wasm --locked
wasm-pack build engine/wasm --target nodejs --release -- --locked
node --test "engine/wasm/test/*.test.mjs"
git diff --exit-code -- engine/Cargo.lock
```
Expected: 전부 성공(MSVC 툴체인에서 확인함).

- [ ] **Step 4: 성능 기록**: Task 13 Step 4의 두 명령을 다시 실행하고 출력을 `engine/golden/PERF.md`에 기기·OS·Node·툴체인과 함께 표로 적는다(로드·제출·전환 단계별 p50/p95, 메모리 3줄). 제출 p50이 2000ms 미만이어도 "데스크톱 Node 기준이며 모바일·브라우저 Worker 적합성은 입증하지 않음. Plan 1b 첫 태스크 게이트 입력"이라고 명시한다(검증 실행 제출 p50 1767ms, 로드 1031ms, 메모리 140.8MiB).

- [ ] **Step 5: 커밋·푸시·PR·필수 체크 대기**

```bash
git add .github/workflows/ci.yml engine/golden/PERF.md
git commit -m "ci(engine): 3-OS debug·release·wasm 골든, base 대비 버전 가드, 실패 보고서 업로드"
git push -u origin feat/engine-1a-4-cli-golden-ci
gh pr create --base main --head feat/engine-1a-4-cli-golden-ci --title "Plan 1a-4: CLI·골든·CI" --body "Plan 1a-4(.omc/plans/01a-4-cli-golden-ci.md) Task 11a~14. 골든: 네이티브 debug·release·wasm 완전 일치, 버전 가드, 성능 기준선."
gh pr checks --watch --required
```
병합은 `gh pr merge --merge`(squash 금지: `sources.json`의 generator 커밋이 `main`에 남아야 한다).

Expected: `Engine · ubuntu-latest / windows-latest / macos-latest`와 `Web` 필수 체크 pass. CI의 `Test (wasm == native)` 단계는 앞선 `cargo test --workspace` 단계가 만든 `engine/target/{smoke,cli-contract}`를 읽는다. 실패 시 아티팩트 `golden-actual-<os>`의 `actual-*.json`을 `expected.json`과 비교한다. **값을 덮어써서 고치지 않는다.** 병합은 사용자 확인 후.
