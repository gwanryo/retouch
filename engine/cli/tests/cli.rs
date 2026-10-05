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
