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
        let hash = ok(&[
            "render",
            "--input",
            s(&jpg),
            "--recipe",
            s(&r),
            "--output",
            s(&out),
        ]);
        (hash, out)
    };
    let (h1, p1) = run("a.png");
    let (h2, _) = run("b.png");
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64, "{h1}");
    assert!(
        h1.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "{h1}"
    );
    assert_eq!(h1, ok(&["hash", "--input", s(&p1)]));
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
