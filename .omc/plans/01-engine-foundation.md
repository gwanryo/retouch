# Plan 1a: 엔진 기반 (Engine Foundation) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Rust 코드를 쓰기 전에 `rust-best-practices` 스킬을 로드한다.

**Goal:** 브라우저·OS가 달라도 같은 픽셀과 같은 점수를 내는 참조 엔진의 뼈대를 만든다: 색 과학(ΔE00), 결정적 JPEG/PNG 디코드, 선형광 Lanczos3 축소, 레시피 스키마, 기본 패널 렌더, 채점 핵심, 네이티브 CLI, WASM 바인딩, 그리고 3-OS 네이티브 + wasm 골든 해시 CI.

**Architecture:** 하나의 Rust 라이브러리 `engine-core`가 모든 수학을 소유하고, `engine-cli`(네이티브 바이너리)와 `engine-wasm`(wasm-bindgen cdylib)이 얇은 껍데기로 감싼다. 초월함수는 순수 Rust `libm`만 사용해 플랫폼 libm 의존을 제거한다. 렌더는 f32, Lab·ΔE00은 f64. 8bit 양자화는 `buffer::quantize` 한 함수에서만 일어난다. 마스크·크롭·HSL·커브·디테일은 **스키마에는 있지만 렌더는 Plan 1b** — 1a의 `render()`는 이 기능이 들어 있는 레시피에 명시적 `NotYetSupported` 에러를 낸다(조용히 무시 금지).

**Tech Stack:** Rust 1.98 stable (edition 2021), `serde`/`serde_json`, `libm 0.2`, `sha2 0.10`, `thiserror 2`, `image 0.25`(default-features 끄고 `jpeg`,`png`만), `clap 4`, `anyhow 1`(CLI만), `wasm-bindgen 0.2`, `wasm-pack 0.15`, Node 24 `node --test`.

**스펙 커버리지:** AC-E1a, E1c, E1e, E3c, E3d(부분: JSON 검증), S1a, S1b, S1c, S1d, S1e, S1f, S1g, S1h, S1j(E=전체), S2(톤·색 진단), S3, S5a(측정 기반), 마스터 플랜 A1, A2, A3, A4, A6, A8, §5 결정성 규율.

**사전 조건(확인됨 2026-10-01):** rustc 1.98.1, cargo 1.98.1, rustup 1.29.1, wasm-pack 0.15.0, `wasm32-unknown-unknown` 타깃, Node 24.16. 셸에서 `cargo`가 안 보이면 `export PATH="$HOME/.cargo/bin:$PATH"`.

---

## 파일 구조

```
engine/
├─ Cargo.toml                 워크스페이스, 공유 의존성·lint
├─ rustfmt.toml
├─ core/                      crate `engine-core` (lib)
│  ├─ Cargo.toml
│  ├─ src/lib.rs              모듈 선언, ENGINE_VERSION/SCORING_VERSION/SCHEMA_VERSION
│  ├─ src/color.rs            sRGB↔linear, linear sRGB→Lab D50(Bradford), CIEDE2000
│  ├─ src/buffer.rs           ImageF32(RGB f32 interleaved), quantize/dequantize, RGBA8 변환
│  ├─ src/resample.rs         fit_long_edge, Lanczos3 선형광 축소
│  ├─ src/decode.rs           JPEG/PNG → RGBA8 (image 크레이트), PNG 인코드
│  ├─ src/recipe.rs           Recipe/Basic/Mask/Crop, normalize(), RecipeError
│  ├─ src/render/mod.rs       render(): 순서 고정, NotYetSupported
│  ├─ src/render/basic.rs     노출·WB(선형) / 대비·하이라이트·쉐도우·화이트·블랙·생동감·채도(인코딩)
│  ├─ src/score.rs            Region, Score, Diagnostics, score(), relative_score, 타일 통계
│  ├─ src/hash.rs             sha256_hex
│  └─ tests/golden.rs         골든 해시 통합 테스트 (engine/golden/expected.json)
├─ cli/                       crate `engine-cli` (bin)
│  ├─ Cargo.toml
│  ├─ src/main.rs             clap: fixture | render | downscale | gen-answer | score | hash
│  └─ tests/cli.rs            CARGO_BIN_EXE_engine-cli 로 end-to-end
├─ wasm/                      crate `engine-wasm` (cdylib)
│  ├─ Cargo.toml
│  ├─ src/lib.rs              decode_image, render_rgba8, downscale_rgba8, score_rgba8, sha256_hex, versions
│  └─ test/determinism.test.mjs   wasm 출력 == expected.json
└─ golden/
   ├─ fixture.jpg             `engine-cli fixture`가 만든 합성 이미지 (커밋)
   ├─ recipes/identity.json, exposure_plus1.json, warm_contrast.json
   ├─ expected.json           regen.sh 산출물 (커밋)
   └─ regen.sh                CLI로 expected.json 재생성
scripts/
└─ lint-determinism.sh        std 수학 함수 사용 차단
```

---

## Task 0: 워크스페이스 스캐폴드

**Files:**
- Create: `engine/Cargo.toml`, `engine/rustfmt.toml`, `engine/core/Cargo.toml`, `engine/core/src/lib.rs`
- Create: `scripts/lint-determinism.sh`

- [ ] **Step 1: 워크스페이스 Cargo.toml 작성**

`engine/Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["core", "cli", "wasm"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
rust-version = "1.80"
repository = "https://github.com/gwanryo/retouch"

[workspace.dependencies]
engine-core = { path = "core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
libm = "0.2"
sha2 = "0.10"
thiserror = "2"
image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
wasm-bindgen = "0.2"

[workspace.lints.rust]
missing_docs = "warn"
unsafe_code = "forbid"

[workspace.lints.clippy]
all = { level = "deny", priority = 10 }
unwrap_used = { level = "deny", priority = 9 }
expect_used = { level = "deny", priority = 9 }
pedantic = { level = "warn", priority = 3 }

# 결정성: LTO는 수치 결과를 바꾸지 않는다(부동소수 재결합은 fast-math 없이는 일어나지 않음).
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
```

`engine/rustfmt.toml`:
```toml
edition = "2021"
max_width = 100
```

- [ ] **Step 2: core 크레이트 작성**

`engine/core/Cargo.toml`:
```toml
[package]
name = "engine-core"
description = "Retouch reference engine: deterministic decode, render, resample and score"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
libm.workspace = true
sha2.workspace = true
thiserror.workspace = true
image.workspace = true

[lints]
workspace = true
```

`engine/core/src/lib.rs`:
```rust
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
pub mod render;
pub mod resample;
pub mod score;

/// Render pipeline version. Bump whenever any render math changes (golden hashes change).
pub const ENGINE_VERSION: &str = "engine-0.1.0";
/// Scoring version. Bump whenever score math or [`score::T_TILE`] changes.
pub const SCORING_VERSION: &str = "scoring-0.1.0";
/// Recipe JSON `schema_version` accepted by this build.
pub const SCHEMA_VERSION: u32 = 1;
```

모듈 파일은 아직 없으므로 컴파일이 실패한다. 다음 스텝에서 빈 모듈을 만든다.

- [ ] **Step 3: 빈 모듈 파일 생성 후 컴파일 확인**

```bash
cd engine
mkdir -p core/src/render cli/src wasm/src
for m in buffer color decode hash recipe resample score; do printf '//! %s\n' "$m" > core/src/$m.rs; done
printf '//! render\npub mod basic;\n' > core/src/render/mod.rs
printf '//! basic\n' > core/src/render/basic.rs
```

`cli`와 `wasm`은 Task 11/12에서 만들지만 워크스페이스 멤버라 빈 크레이트가 필요하다:

`engine/cli/Cargo.toml`:
```toml
[package]
name = "engine-cli"
description = "Retouch engine command line"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true

[[bin]]
name = "engine-cli"
path = "src/main.rs"

[dependencies]
engine-core.workspace = true
anyhow.workspace = true
clap.workspace = true
serde_json.workspace = true
image.workspace = true

[lints]
workspace = true
```

`engine/cli/src/main.rs`:
```rust
//! Retouch engine CLI (filled in Task 11).
fn main() {}
```

`engine/wasm/Cargo.toml`:
```toml
[package]
name = "engine-wasm"
description = "Retouch engine wasm bindings"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
engine-core.workspace = true
wasm-bindgen.workspace = true
serde_json.workspace = true

[lints]
workspace = true
```

`engine/wasm/src/lib.rs`:
```rust
//! Retouch engine wasm bindings (filled in Task 12).
```

Run: `cd engine && cargo build --workspace`
Expected: `Finished` (경고 없음). `Cargo.lock`이 생성된다 — 커밋 대상.

- [ ] **Step 4: 결정성 린트 스크립트**

`scripts/lint-determinism.sh`:
```bash
#!/usr/bin/env bash
# Fails if engine code calls std float transcendental methods. Those dispatch to the platform
# libm (native) or compiler-builtins (wasm) and are NOT guaranteed bit-identical across targets.
# All transcendental math must go through the pure-Rust `libm` crate (libm::powf, libm::exp, ...).
# Allowed std methods: abs, floor, ceil, round, trunc, sqrt, clamp, min, max, mul (IEEE exact).
set -euo pipefail
cd "$(dirname "$0")/.."

pattern='\.(powf|powi|exp|exp2|exp_m1|ln|ln_1p|log|log2|log10|sin|cos|tan|asin|acos|atan|atan2|sinh|cosh|tanh|asinh|acosh|atanh|cbrt|hypot|mul_add|sin_cos|to_degrees|to_radians)\('
if grep -rnE --include='*.rs' "$pattern" engine/core/src engine/cli/src engine/wasm/src; then
  echo "determinism lint: std float method found; use libm::* instead" >&2
  exit 1
fi

if grep -rnE 'target-cpu|fast-math|ffast-math' engine/.cargo 2>/dev/null; then
  echo "determinism lint: target-cpu / fast-math flags are forbidden" >&2
  exit 1
fi

echo "determinism lint: ok"
```

Run: `bash scripts/lint-determinism.sh`
Expected: `determinism lint: ok`

- [ ] **Step 5: 커밋**

```bash
git add engine scripts
git commit -m "feat(engine): 워크스페이스 스캐폴드와 결정성 린트"
```

---

## Task 1: sRGB 전달 함수

**Files:**
- Modify: `engine/core/src/color.rs`

- [ ] **Step 1: 실패하는 테스트 작성**

`engine/core/src/color.rs`:
```rust
//! Color science. All transcendental math goes through `libm` so native and wasm agree bit-for-bit.

/// sRGB electro-optical transfer: encoded [0,1] → linear [0,1]. Input is clamped.
pub fn srgb_decode(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.040_45 {
        v / 12.92
    } else {
        libm::powf((v + 0.055) / 1.055, 2.4)
    }
}

/// sRGB opto-electronic transfer: linear [0,1] → encoded [0,1]. Input is clamped.
pub fn srgb_encode(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * libm::powf(v, 1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod srgb_decode {
        use super::*;

        #[test]
        fn maps_zero_to_zero() {
            assert_eq!(srgb_decode(0.0), 0.0);
        }

        #[test]
        fn maps_one_to_one() {
            assert!((srgb_decode(1.0) - 1.0).abs() < 1e-6, "got {}", srgb_decode(1.0));
        }

        #[test]
        fn maps_mid_gray_128_to_known_linear_value() {
            // 128/255 = 0.50196 → linear 0.21586 (standard sRGB table value)
            let lin = srgb_decode(128.0 / 255.0);
            assert!((lin - 0.215_86).abs() < 1e-4, "got {lin}");
        }

        #[test]
        fn clamps_out_of_range_input() {
            assert_eq!(srgb_decode(1.7), srgb_decode(1.0));
        }
    }

    mod srgb_encode {
        use super::*;

        #[test]
        fn is_inverse_of_decode_within_f32_precision() {
            for i in 0..=255u32 {
                let v = i as f32 / 255.0;
                let back = srgb_encode(srgb_decode(v));
                assert!((back - v).abs() < 2e-6, "i={i} v={v} back={back}");
            }
        }

        #[test]
        fn maps_linear_black_to_zero() {
            assert_eq!(srgb_encode(0.0), 0.0);
        }
    }
}
```

- [ ] **Step 2: 테스트 실행 → 통과 확인** (구현이 테스트와 함께 들어갔으므로 바로 실행)

Run: `cd engine && cargo test -p engine-core color::`
Expected: `test result: ok. 6 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/color.rs
git commit -m "feat(engine): sRGB 전달 함수"
```

---

## Task 2: linear sRGB → Lab D50 (Bradford)

**Files:**
- Modify: `engine/core/src/color.rs`

- [ ] **Step 1: 실패하는 테스트 추가** (`tests` 모듈 안에 추가)

```rust
    mod linear_srgb_to_lab_d50 {
        use super::*;

        #[test]
        fn white_maps_to_l_100() {
            let lab = linear_srgb_to_lab_d50(1.0, 1.0, 1.0);
            assert!((lab.l - 100.0).abs() < 0.01, "got {lab:?}");
        }

        #[test]
        fn white_is_neutral_after_bradford_adaptation() {
            let lab = linear_srgb_to_lab_d50(1.0, 1.0, 1.0);
            assert!(lab.a.abs() < 0.1 && lab.b.abs() < 0.1, "got {lab:?}");
        }

        #[test]
        fn black_maps_to_l_0() {
            let lab = linear_srgb_to_lab_d50(0.0, 0.0, 0.0);
            assert!(lab.l.abs() < 1e-9, "got {lab:?}");
        }

        #[test]
        fn pure_red_has_positive_a() {
            let lab = linear_srgb_to_lab_d50(1.0, 0.0, 0.0);
            assert!(lab.a > 60.0, "got {lab:?}");
        }

        #[test]
        fn pure_blue_has_negative_b() {
            let lab = linear_srgb_to_lab_d50(0.0, 0.0, 1.0);
            assert!(lab.b < -60.0, "got {lab:?}");
        }
    }
```

Run: `cd engine && cargo test -p engine-core color::linear_srgb_to_lab_d50`
Expected: 컴파일 에러 `cannot find function linear_srgb_to_lab_d50`

- [ ] **Step 2: 구현** (`srgb_encode` 아래에 추가)

```rust
/// CIE L*a*b* under the D50 illuminant (ICC PCS). Used for CIEDE2000.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    /// Lightness, 0..100.
    pub l: f64,
    /// Green–red axis.
    pub a: f64,
    /// Blue–yellow axis.
    pub b: f64,
}

// Linear sRGB → XYZ (D65), IEC 61966-2-1.
const M_RGB_TO_XYZ_D65: [[f64; 3]; 3] = [
    [0.412_456_4, 0.357_576_1, 0.180_437_5],
    [0.212_672_9, 0.715_152_2, 0.072_175_0],
    [0.019_333_9, 0.119_192_0, 0.950_304_1],
];

// Bradford chromatic adaptation D65 → D50 (Lindbloom).
const M_BRADFORD_D65_TO_D50: [[f64; 3]; 3] = [
    [1.047_811_2, 0.022_886_6, -0.050_127_0],
    [0.029_542_4, 0.990_484_4, -0.017_049_1],
    [-0.009_234_5, 0.015_043_6, 0.752_131_6],
];

const D50_WHITE: [f64; 3] = [0.964_22, 1.0, 0.825_21];

fn mul3(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn lab_f(t: f64) -> f64 {
    const DELTA: f64 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        libm::cbrt(t)
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

/// Linear sRGB (D65) → XYZ D65 → Bradford → XYZ D50 → Lab D50 (AC-S1f).
pub fn linear_srgb_to_lab_d50(r: f32, g: f32, b: f32) -> Lab {
    let xyz65 = mul3(&M_RGB_TO_XYZ_D65, [f64::from(r), f64::from(g), f64::from(b)]);
    let xyz50 = mul3(&M_BRADFORD_D65_TO_D50, xyz65);
    let fx = lab_f(xyz50[0] / D50_WHITE[0]);
    let fy = lab_f(xyz50[1] / D50_WHITE[1]);
    let fz = lab_f(xyz50[2] / D50_WHITE[2]);
    Lab { l: 116.0 * fy - 16.0, a: 500.0 * (fx - fy), b: 200.0 * (fy - fz) }
}
```

- [ ] **Step 3: 테스트 통과 확인**

Run: `cd engine && cargo test -p engine-core color::`
Expected: `test result: ok. 11 passed`

- [ ] **Step 4: 커밋**

```bash
git add engine/core/src/color.rs
git commit -m "feat(engine): linear sRGB → Lab D50 (Bradford)"
```

---

## Task 3: CIEDE2000

**Files:**
- Modify: `engine/core/src/color.rs`

- [ ] **Step 1: 실패하는 테스트 추가** — Sharma·Wu·Dalal(2005) 공개 테스트 벡터 34쌍. 허용 오차 1e-4 (AC-S1g).

```rust
    mod delta_e00 {
        use super::*;

        // Sharma, Wu, Dalal (2005) "The CIEDE2000 color-difference formula: Implementation notes,
        // supplementary test data, and mathematical observations". Columns: L1 a1 b1 L2 a2 b2 dE00.
        const SHARMA: [[f64; 7]; 34] = [
            [50.0000, 2.6772, -79.7751, 50.0000, 0.0000, -82.7485, 2.0425],
            [50.0000, 3.1571, -77.2803, 50.0000, 0.0000, -82.7485, 2.8615],
            [50.0000, 2.8361, -74.0200, 50.0000, 0.0000, -82.7485, 3.4412],
            [50.0000, -1.3802, -84.2814, 50.0000, 0.0000, -82.7485, 1.0000],
            [50.0000, -1.1848, -84.8006, 50.0000, 0.0000, -82.7485, 1.0000],
            [50.0000, -0.9009, -85.5211, 50.0000, 0.0000, -82.7485, 1.0000],
            [50.0000, 0.0000, 0.0000, 50.0000, -1.0000, 2.0000, 2.3669],
            [50.0000, -1.0000, 2.0000, 50.0000, 0.0000, 0.0000, 2.3669],
            [50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0009, 7.1792],
            [50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0010, 7.1792],
            [50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0011, 7.2195],
            [50.0000, 2.4900, -0.0010, 50.0000, -2.4900, 0.0012, 7.2195],
            [50.0000, -0.0010, 2.4900, 50.0000, 0.0009, -2.4900, 4.8045],
            [50.0000, -0.0010, 2.4900, 50.0000, 0.0010, -2.4900, 4.8045],
            [50.0000, -0.0010, 2.4900, 50.0000, 0.0011, -2.4900, 4.7461],
            [50.0000, 2.5000, 0.0000, 50.0000, 0.0000, -2.5000, 4.3065],
            [50.0000, 2.5000, 0.0000, 73.0000, 25.0000, -18.0000, 27.1492],
            [50.0000, 2.5000, 0.0000, 61.0000, -5.0000, 29.0000, 22.8977],
            [50.0000, 2.5000, 0.0000, 56.0000, -27.0000, -3.0000, 31.9030],
            [50.0000, 2.5000, 0.0000, 58.0000, 24.0000, 15.0000, 19.4535],
            [50.0000, 2.5000, 0.0000, 50.0000, 3.1736, 0.5854, 1.0000],
            [50.0000, 2.5000, 0.0000, 50.0000, 3.2972, 0.0000, 1.0000],
            [50.0000, 2.5000, 0.0000, 50.0000, 1.8634, 0.5757, 1.0000],
            [50.0000, 2.5000, 0.0000, 50.0000, 3.2592, 0.3350, 1.0000],
            [60.2574, -34.0099, 36.2677, 60.4626, -34.1751, 39.4387, 1.2644],
            [63.0109, -31.0961, -5.8663, 62.8187, -29.7946, -4.0864, 1.2630],
            [61.2901, 3.7196, -5.3901, 61.4292, 2.2480, -4.9620, 1.8731],
            [35.0831, -44.1164, 3.7933, 35.0232, -40.0716, 1.5901, 1.8645],
            [22.7233, 20.0904, -46.6940, 23.0331, 14.9730, -42.5619, 2.0373],
            [36.4612, 47.8580, 18.3852, 36.2715, 50.5065, 21.2231, 1.4146],
            [90.8027, -2.0831, 1.4410, 91.1528, -1.6435, 0.0447, 1.4441],
            [90.9257, -0.5406, -0.9208, 88.6381, -0.8985, -0.7239, 1.5381],
            [6.7747, -0.2908, -2.4247, 5.8714, -0.0985, -2.2286, 0.6377],
            [2.0776, 0.0795, -1.1350, 0.9033, -0.0636, -0.5514, 0.9082],
        ];

        #[test]
        fn matches_all_sharma_test_vectors_within_1e_4() {
            for (i, row) in SHARMA.iter().enumerate() {
                let c1 = Lab { l: row[0], a: row[1], b: row[2] };
                let c2 = Lab { l: row[3], a: row[4], b: row[5] };
                let de = delta_e00(c1, c2);
                assert!((de - row[6]).abs() <= 1e-4, "pair {}: expected {} got {de}", i + 1, row[6]);
            }
        }

        #[test]
        fn is_symmetric() {
            let c1 = Lab { l: 50.0, a: 2.5, b: 0.0 };
            let c2 = Lab { l: 73.0, a: 25.0, b: -18.0 };
            assert!((delta_e00(c1, c2) - delta_e00(c2, c1)).abs() < 1e-12);
        }

        #[test]
        fn identical_colors_have_zero_difference() {
            let c = Lab { l: 40.0, a: -12.0, b: 30.0 };
            assert_eq!(delta_e00(c, c), 0.0);
        }
    }
```

Run: `cd engine && cargo test -p engine-core color::delta_e00`
Expected: 컴파일 에러 `cannot find function delta_e00`

- [ ] **Step 2: 구현**

```rust
/// CIEDE2000 color difference with `kL = kC = kH = 1` (AC-S1f). f64 + `libm` throughout.
pub fn delta_e00(c1: Lab, c2: Lab) -> f64 {
    use libm::{atan2, cos, exp, hypot, pow, sin, sqrt};
    use std::f64::consts::PI;
    const TWO_PI: f64 = 2.0 * PI;
    const POW25_7: f64 = 6_103_515_625.0; // 25^7

    let c1c = hypot(c1.a, c1.b);
    let c2c = hypot(c2.a, c2.b);
    let cbar = (c1c + c2c) / 2.0;
    let cbar7 = pow(cbar, 7.0);
    let g = 0.5 * (1.0 - sqrt(cbar7 / (cbar7 + POW25_7)));

    let a1p = (1.0 + g) * c1.a;
    let a2p = (1.0 + g) * c2.a;
    let c1p = hypot(a1p, c1.b);
    let c2p = hypot(a2p, c2.b);

    let hue = |a: f64, b: f64| -> f64 {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            let h = atan2(b, a);
            if h < 0.0 { h + TWO_PI } else { h }
        }
    };
    let h1p = hue(a1p, c1.b);
    let h2p = hue(a2p, c2.b);

    let dlp = c2.l - c1.l;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d.abs() <= PI { d } else if d > PI { d - TWO_PI } else { d + TWO_PI }
    };
    let dhp_big = 2.0 * sqrt(c1p * c2p) * sin(dhp / 2.0);

    let lbp = (c1.l + c2.l) / 2.0;
    let cbp = (c1p + c2p) / 2.0;
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else {
        let s = h1p + h2p;
        if (h1p - h2p).abs() <= PI {
            s / 2.0
        } else if s < TWO_PI {
            (s + TWO_PI) / 2.0
        } else {
            (s - TWO_PI) / 2.0
        }
    };

    let t = 1.0 - 0.17 * cos(hbp - PI / 6.0)
        + 0.24 * cos(2.0 * hbp)
        + 0.32 * cos(3.0 * hbp + PI / 30.0)
        - 0.20 * cos(4.0 * hbp - 63.0 * PI / 180.0);
    let hbp_deg = hbp * 180.0 / PI;
    let dtheta = (30.0 * PI / 180.0) * exp(-pow((hbp_deg - 275.0) / 25.0, 2.0));
    let cbp7 = pow(cbp, 7.0);
    let rc = 2.0 * sqrt(cbp7 / (cbp7 + POW25_7));
    let lbp50 = pow(lbp - 50.0, 2.0);
    let sl = 1.0 + 0.015 * lbp50 / sqrt(20.0 + lbp50);
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let rt = -sin(2.0 * dtheta) * rc;

    let tl = dlp / sl;
    let tc = dcp / sc;
    let th = dhp_big / sh;
    sqrt(tl * tl + tc * tc + th * th + rt * tc * th)
}
```

- [ ] **Step 3: 테스트 통과 확인**

Run: `cd engine && cargo test -p engine-core color::`
Expected: `test result: ok. 14 passed`. 벡터 중 하나라도 1e-4를 넘으면 표를 원 논문(Table 1)과 재대조한다 — 구현이 아니라 표 전사 오류일 수 있다.

- [ ] **Step 4: 커밋**

```bash
git add engine/core/src/color.rs
git commit -m "feat(engine): CIEDE2000 (Sharma 34 벡터 검증)"
```

---

## Task 4: 이미지 버퍼와 양자화 규칙

**Files:**
- Modify: `engine/core/src/buffer.rs`

- [ ] **Step 1: 실패하는 테스트 작성**

`engine/core/src/buffer.rs`:
```rust
//! Float image buffer and THE single 8-bit quantization rule (AC-E1e).

use thiserror::Error;

/// Errors from buffer construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BufferError {
    /// RGBA byte length does not match `width * height * 4`.
    #[error("rgba buffer has {actual} bytes, expected {expected} for {width}x{height}")]
    LengthMismatch {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
        /// Bytes expected.
        expected: usize,
        /// Bytes received.
        actual: usize,
    },
}

/// RGB image, interleaved `f32`, row-major. Whether values are encoded sRGB or linear is a
/// convention of the call site (render works on encoded, resample on linear).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageF32 {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

/// Encoded [0,1] → 8-bit: clamp, scale, round half up. The ONLY quantization in the engine.
pub fn quantize(v: f32) -> u8 {
    // `as u8` truncates toward zero; adding 0.5 first gives round-half-up for non-negative input.
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// 8-bit → encoded [0,1].
pub fn dequantize(v: u8) -> f32 {
    f32::from(v) / 255.0
}

impl ImageF32 {
    /// All-zero image.
    pub fn zeroed(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize * 3;
        Self { width, height, data: vec![0.0; n] }
    }

    /// From RGBA8 (alpha dropped), values dequantized to encoded sRGB.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Self, BufferError> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(BufferError::LengthMismatch { width, height, expected, actual: rgba.len() });
        }
        let data = rgba
            .chunks_exact(4)
            .flat_map(|px| [dequantize(px[0]), dequantize(px[1]), dequantize(px[2])])
            .collect();
        Ok(Self { width, height, data })
    }

    /// To RGBA8 with alpha 255, via [`quantize`].
    pub fn to_rgba8(&self) -> Vec<u8> {
        self.data
            .chunks_exact(3)
            .flat_map(|px| [quantize(px[0]), quantize(px[1]), quantize(px[2]), 255])
            .collect()
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Interleaved RGB samples.
    pub fn samples(&self) -> &[f32] {
        &self.data
    }

    /// Pixel at (x, y). Caller guarantees bounds.
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Set pixel at (x, y). Caller guarantees bounds.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: [f32; 3]) {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        self.data[i..i + 3].copy_from_slice(&px);
    }

    /// New image with `f` applied to every pixel, in row-major order.
    pub fn map_pixels(&self, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        let data = self.data.chunks_exact(3).flat_map(|px| f([px[0], px[1], px[2]])).collect();
        Self { width: self.width, height: self.height, data }
    }

    /// Iterate pixels in row-major order.
    pub fn pixels(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.data.chunks_exact(3).map(|px| [px[0], px[1], px[2]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod quantize {
        use super::*;

        #[test]
        fn rounds_half_up() {
            // 0.5/255 * 255 = 0.5 → +0.5 = 1.0 → 1
            assert_eq!(quantize(0.5 / 255.0), 1);
        }

        #[test]
        fn maps_one_to_255() {
            assert_eq!(quantize(1.0), 255);
        }

        #[test]
        fn clamps_above_one() {
            assert_eq!(quantize(1.5), 255);
        }

        #[test]
        fn clamps_below_zero() {
            assert_eq!(quantize(-0.2), 0);
        }

        #[test]
        fn round_trips_every_byte() {
            for v in 0..=255u8 {
                assert_eq!(quantize(dequantize(v)), v, "byte {v}");
            }
        }
    }

    mod from_rgba8 {
        use super::*;

        #[test]
        fn rejects_length_mismatch() {
            let err = ImageF32::from_rgba8(2, 1, &[0; 7]).unwrap_err();
            assert_eq!(
                err,
                BufferError::LengthMismatch { width: 2, height: 1, expected: 8, actual: 7 }
            );
        }

        #[test]
        fn drops_alpha_and_dequantizes() {
            let img = ImageF32::from_rgba8(1, 1, &[255, 0, 128, 7]).unwrap();
            assert_eq!(img.pixel(0, 0), [1.0, 0.0, 128.0 / 255.0]);
        }
    }

    mod to_rgba8 {
        use super::*;

        #[test]
        fn round_trips_rgba8_exactly() {
            let src: Vec<u8> = (0..64u8).flat_map(|i| [i, 255 - i, i.wrapping_mul(3), 255]).collect();
            let img = ImageF32::from_rgba8(8, 2, &src).unwrap();
            assert_eq!(img.to_rgba8(), src);
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core buffer::`
Expected: `test result: ok. 8 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/buffer.rs
git commit -m "feat(engine): ImageF32 버퍼와 단일 양자화 규칙"
```

---

## Task 5: 선형광 Lanczos3 축소

**Files:**
- Modify: `engine/core/src/resample.rs`

- [ ] **Step 1: 실패하는 테스트 작성**

`engine/core/src/resample.rs`:
```rust
//! Downscaling for the scoring path (AC-S1h): Lanczos3, linear light, fixed rules.
//!
//! Rules that are part of the score contract:
//! - kernel `a = 3`, evaluated at `(j - center) / scale`, radius `3 * scale` source pixels
//! - pixel centers at `i + 0.5`; `center = (i + 0.5) * scale - 0.5`
//! - source index clamped to the edge (clamp-to-edge boundary)
//! - weights normalized to sum 1, accumulated in ascending source index order
//! - separable: horizontal pass then vertical pass

use crate::buffer::ImageF32;
use crate::color::{srgb_decode, srgb_encode};
use thiserror::Error;

/// Errors from resampling.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResampleError {
    /// Zero-sized source or destination.
    #[error("zero-sized image: {0}x{1}")]
    ZeroSize(u32, u32),
    /// Destination larger than source on some axis (this module only downscales).
    #[error("upscaling is not supported: {src_w}x{src_h} -> {dst_w}x{dst_h}")]
    Upscale {
        /// Source width.
        src_w: u32,
        /// Source height.
        src_h: u32,
        /// Destination width.
        dst_w: u32,
        /// Destination height.
        dst_h: u32,
    },
}

/// Target size so that the longer edge equals `long_edge` (never upscales; min 1px).
/// The short edge is `round_half_up(short * long_edge / long)`.
pub fn fit_long_edge(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= long_edge {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(long);
    let short = (f64::from(width.min(height)) * scale + 0.5).floor().max(1.0) as u32;
    if width >= height { (long_edge, short) } else { (short, long_edge) }
}

fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f32::consts::PI * x;
        libm::sinf(px) / px
    }
}

fn lanczos3(x: f32) -> f32 {
    let ax = x.abs();
    if ax >= 3.0 { 0.0 } else { sinc(x) * sinc(x / 3.0) }
}

/// Per destination index: first source index and normalized weights.
fn axis_weights(src_len: u32, dst_len: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = src_len as f32 / dst_len as f32; // >= 1.0
    let radius = 3.0 * scale;
    let last = i64::from(src_len) - 1;
    (0..dst_len)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale - 0.5;
            let lo = ((center - radius).floor() as i64).clamp(0, last);
            let hi = ((center + radius).ceil() as i64).clamp(0, last);
            let mut w: Vec<f32> = (lo..=hi).map(|j| lanczos3((j as f32 - center) / scale)).collect();
            let sum: f32 = w.iter().sum();
            for x in &mut w {
                *x /= sum;
            }
            (lo as usize, w)
        })
        .collect()
}

/// Lanczos3 downscale of a **linear-light** image.
pub fn downscale_lanczos3(
    src: &ImageF32,
    dst_w: u32,
    dst_h: u32,
) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    if sw == 0 || sh == 0 || dst_w == 0 || dst_h == 0 {
        return Err(ResampleError::ZeroSize(dst_w, dst_h));
    }
    if dst_w > sw || dst_h > sh {
        return Err(ResampleError::Upscale { src_w: sw, src_h: sh, dst_w, dst_h });
    }

    // Horizontal pass: sw x sh -> dst_w x sh
    let wx = axis_weights(sw, dst_w);
    let mut tmp = ImageF32::zeroed(dst_w, sh);
    for y in 0..sh {
        for (ox, (start, weights)) in wx.iter().enumerate() {
            let mut acc = [0.0f32; 3];
            for (k, w) in weights.iter().enumerate() {
                let px = src.pixel((start + k) as u32, y);
                acc[0] += px[0] * w;
                acc[1] += px[1] * w;
                acc[2] += px[2] * w;
            }
            tmp.set_pixel(ox as u32, y, acc);
        }
    }

    // Vertical pass: dst_w x sh -> dst_w x dst_h
    let wy = axis_weights(sh, dst_h);
    let mut out = ImageF32::zeroed(dst_w, dst_h);
    for x in 0..dst_w {
        for (oy, (start, weights)) in wy.iter().enumerate() {
            let mut acc = [0.0f32; 3];
            for (k, w) in weights.iter().enumerate() {
                let px = tmp.pixel(x, (start + k) as u32);
                acc[0] += px[0] * w;
                acc[1] += px[1] * w;
                acc[2] += px[2] * w;
            }
            out.set_pixel(x, oy as u32, acc);
        }
    }
    Ok(out)
}

/// Encoded sRGB in → decode to linear → Lanczos3 to `long_edge` → encode back. This is the
/// scoring-image path: 2048 render → 1024 (AC-S1h).
pub fn downscale_encoded_to_long_edge(
    src_encoded: &ImageF32,
    long_edge: u32,
) -> Result<ImageF32, ResampleError> {
    let (dw, dh) = fit_long_edge(src_encoded.width(), src_encoded.height(), long_edge);
    if (dw, dh) == (src_encoded.width(), src_encoded.height()) {
        return Ok(src_encoded.clone());
    }
    let linear = src_encoded.map_pixels(|p| [srgb_decode(p[0]), srgb_decode(p[1]), srgb_decode(p[2])]);
    let small = downscale_lanczos3(&linear, dw, dh)?;
    Ok(small.map_pixels(|p| [srgb_encode(p[0]), srgb_encode(p[1]), srgb_encode(p[2])]))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod fit_long_edge {
        use super::*;

        #[test]
        fn scales_landscape_2048x1365_to_1024x683() {
            assert_eq!(fit_long_edge(2048, 1365, 1024), (1024, 683));
        }

        #[test]
        fn scales_portrait_1365x2048_to_683x1024() {
            assert_eq!(fit_long_edge(1365, 2048, 1024), (683, 1024));
        }

        #[test]
        fn never_upscales() {
            assert_eq!(fit_long_edge(800, 600, 1024), (800, 600));
        }
    }

    mod downscale_lanczos3 {
        use super::*;

        #[test]
        fn keeps_a_constant_image_constant() {
            let src = ImageF32::zeroed(64, 48).map_pixels(|_| [0.25, 0.5, 0.75]);
            let out = downscale_lanczos3(&src, 32, 24).unwrap();
            for px in out.pixels() {
                assert!((px[0] - 0.25).abs() < 1e-5 && (px[1] - 0.5).abs() < 1e-5 && (px[2] - 0.75).abs() < 1e-5, "{px:?}");
            }
        }

        #[test]
        fn averages_a_1px_checkerboard_to_mid_gray_at_2x() {
            let mut src = ImageF32::zeroed(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = if (x + y) % 2 == 0 { 1.0 } else { 0.0 };
                    src.set_pixel(x, y, [v, v, v]);
                }
            }
            let out = downscale_lanczos3(&src, 32, 32).unwrap();
            let center = out.pixel(16, 16);
            assert!((center[0] - 0.5).abs() < 0.05, "{center:?}");
        }

        #[test]
        fn returns_requested_dimensions() {
            let src = ImageF32::zeroed(100, 70);
            let out = downscale_lanczos3(&src, 50, 35).unwrap();
            assert_eq!((out.width(), out.height()), (50, 35));
        }

        #[test]
        fn rejects_upscale() {
            let src = ImageF32::zeroed(10, 10);
            let err = downscale_lanczos3(&src, 20, 10).unwrap_err();
            assert_eq!(err, ResampleError::Upscale { src_w: 10, src_h: 10, dst_w: 20, dst_h: 10 });
        }

        #[test]
        fn is_bitwise_deterministic_across_calls() {
            let src = ImageF32::zeroed(64, 48).map_pixels(|_| [0.1, 0.2, 0.3]);
            let a = downscale_lanczos3(&src, 30, 20).unwrap();
            let b = downscale_lanczos3(&src, 30, 20).unwrap();
            assert_eq!(a, b);
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core resample::`
Expected: `test result: ok. 8 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/resample.rs
git commit -m "feat(engine): 선형광 Lanczos3 축소 (채점 경로)"
```

---

## Task 6: 디코드/인코드

**Files:**
- Modify: `engine/core/src/decode.rs`

- [ ] **Step 1: 테스트와 구현 작성**

`engine/core/src/decode.rs`:
```rust
//! Reference-path image decode (master plan A2). The same `image` crate build runs natively and
//! in wasm, so pixels are identical everywhere — unlike browser `<img>` decoding.

use thiserror::Error;

/// Decode/encode errors.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// The `image` crate rejected the bytes.
    #[error("image decode/encode failed: {0}")]
    Image(#[from] ::image::ImageError),
}

/// Decoded RGBA8 pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Interleaved RGBA, row-major, `width * height * 4` bytes.
    pub data: Vec<u8>,
}

/// Decode JPEG or PNG bytes to RGBA8. EXIF orientation is NOT applied here: content assets are
/// normalized (orientation baked in) at build time (AC-E1c).
pub fn decode_rgba8(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    let img = ::image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = img.dimensions();
    Ok(Rgba8 { width, height, data: img.into_raw() })
}

/// Encode RGBA8 as PNG (lossless; used for answer references, AC-S1d).
pub fn encode_png_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, DecodeError> {
    use ::image::ImageEncoder;
    let mut out = Vec::new();
    let enc = ::image::codecs::png::PngEncoder::new(&mut out);
    enc.write_image(rgba, width, height, ::image::ExtendedColorType::Rgba8)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips_pixels_exactly() {
        let rgba: Vec<u8> = (0..(4 * 3 * 4) as u8).collect();
        let png = encode_png_rgba8(4, 3, &rgba).unwrap();
        let back = decode_rgba8(&png).unwrap();
        assert_eq!(back, Rgba8 { width: 4, height: 3, data: rgba });
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(decode_rgba8(b"not an image").is_err());
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core decode::`
Expected: `test result: ok. 2 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/decode.rs
git commit -m "feat(engine): 참조 경로 JPEG/PNG 디코드·PNG 인코드"
```

---

## Task 7: 레시피 스키마와 정규화

**Files:**
- Modify: `engine/core/src/recipe.rs`

- [ ] **Step 1: 실패하는 테스트 작성** (구현과 함께)

`engine/core/src/recipe.rs`:
```rust
//! Recipe JSON schema v1 (master plan §3.1) and validation (AC-E3c/E3d, E5a, E7a/E7b).
//!
//! Plan 1a renders `basic` only. `masks` and `crop` are parsed and validated here so the file
//! format is final; rendering them is Plan 1b.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Validation failures that reject a recipe outright.
#[derive(Debug, Error, PartialEq)]
pub enum RecipeError {
    /// `schema_version` is not the one this build accepts.
    #[error("unsupported schema_version {0}, expected {expected}", expected = crate::SCHEMA_VERSION)]
    SchemaVersion(u32),
    /// A numeric field is NaN or infinite.
    #[error("field `{0}` is not a finite number")]
    NonFinite(&'static str),
    /// More than the allowed number of masks.
    #[error("{0} masks, at most 3 allowed")]
    TooManyMasks(usize),
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
    /// Crop area below 25% of the original.
    #[error("crop area {0:.3} below minimum 0.25")]
    CropTooSmall(f32),
    /// JSON parse error.
    #[error("recipe json: {0}")]
    Json(String),
}

/// A clamped field, reported back to the caller (AC-E3d).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Normalization {
    /// Dotted field path, e.g. `basic.exposure`.
    pub field: String,
    /// Value in the input.
    pub from: f32,
    /// Value after clamping.
    pub to: f32,
}

/// Global tone/color controls. Names and ranges follow Lightroom `crs:` (AC-E3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Basic {
    /// Exposure2012, EV, -5..5.
    pub exposure: f32,
    /// Contrast2012, -100..100.
    pub contrast: f32,
    /// Highlights2012, -100..100.
    pub highlights: f32,
    /// Shadows2012, -100..100.
    pub shadows: f32,
    /// Whites2012, -100..100.
    pub whites: f32,
    /// Blacks2012, -100..100.
    pub blacks: f32,
    /// Relative white-balance temperature for SDR JPEG input, -100..100 (AC-E4c).
    pub temperature: f32,
    /// Relative tint, -100..100.
    pub tint: f32,
    /// Vibrance, -100..100.
    pub vibrance: f32,
    /// Saturation, -100..100.
    pub saturation: f32,
}

/// Per-mask adjustments (AC-E5b). Rendering: Plan 1b.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjust {
    /// EV, -5..5.
    pub exposure: f32,
    /// -100..100.
    pub contrast: f32,
    /// -100..100.
    pub highlights: f32,
    /// -100..100.
    pub shadows: f32,
    /// -100..100.
    pub whites: f32,
    /// -100..100.
    pub blacks: f32,
    /// -100..100.
    pub temperature: f32,
    /// -100..100.
    pub tint: f32,
    /// -100..100.
    pub saturation: f32,
    /// 0..150.
    pub sharpen_amount: f32,
    /// -100..100.
    pub clarity: f32,
}

/// Range mask channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RangeChannel {
    /// Luminance of the *original* linear pixel (AC-E5e).
    Luminance,
    /// Hue angle of the original pixel, 0..1 = 0..360°.
    Hue,
}

/// Mask geometry in original normalized coordinates [0,1] (AC-E5). Rendering: Plan 1b.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Mask {
    /// Linear gradient from (x0,y0)=full to (x1,y1)=none.
    Linear {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
        /// Feather 0..1.
        feather: f32,
        /// Invert the mask.
        #[serde(default)]
        invert: bool,
        /// Adjustments applied inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
    /// Ellipse centered at (cx,cy) with radii (rx,ry).
    Radial {
        /// Center x.
        cx: f32,
        /// Center y.
        cy: f32,
        /// Radius x.
        rx: f32,
        /// Radius y.
        ry: f32,
        /// Feather 0..1.
        feather: f32,
        /// Invert the mask.
        #[serde(default)]
        invert: bool,
        /// Adjustments applied inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
    /// Selects pixels whose channel value falls in [lo, hi] with smooth edges.
    Range {
        /// Which channel to threshold.
        channel: RangeChannel,
        /// Lower bound 0..1.
        lo: f32,
        /// Upper bound 0..1.
        hi: f32,
        /// Smoothing width 0..1.
        smooth: f32,
        /// Invert the mask.
        #[serde(default)]
        invert: bool,
        /// Adjustments applied inside the mask.
        #[serde(default)]
        adjust: Adjust,
    },
}

/// Axis-aligned crop in original normalized coordinates (AC-E7a).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    /// Left edge 0..1.
    pub x: f32,
    /// Top edge 0..1.
    pub y: f32,
    /// Width 0..1.
    pub w: f32,
    /// Height 0..1.
    pub h: f32,
}

/// A full recipe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Must equal [`crate::SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Global controls.
    #[serde(default)]
    pub basic: Basic,
    /// Up to 3 masks (AC-E5a).
    #[serde(default)]
    pub masks: Vec<Mask>,
    /// Optional crop.
    #[serde(default)]
    pub crop: Option<Crop>,
}

impl Default for Recipe {
    fn default() -> Self {
        Self { schema_version: crate::SCHEMA_VERSION, basic: Basic::default(), masks: Vec::new(), crop: None }
    }
}

/// A recipe that passed validation, with the list of clamped fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Normalized {
    /// The clamped recipe.
    pub recipe: Recipe,
    /// Fields that were out of range and got clamped.
    pub notes: Vec<Normalization>,
}

fn clamp_field(
    field: &'static str,
    value: &mut f32,
    lo: f32,
    hi: f32,
    notes: &mut Vec<Normalization>,
) -> Result<(), RecipeError> {
    if !value.is_finite() {
        return Err(RecipeError::NonFinite(field));
    }
    let clamped = value.clamp(lo, hi);
    if clamped != *value {
        notes.push(Normalization { field: field.to_owned(), from: *value, to: clamped });
        *value = clamped;
    }
    Ok(())
}

fn normalize_basic(b: &mut Basic, notes: &mut Vec<Normalization>) -> Result<(), RecipeError> {
    clamp_field("basic.exposure", &mut b.exposure, -5.0, 5.0, notes)?;
    clamp_field("basic.contrast", &mut b.contrast, -100.0, 100.0, notes)?;
    clamp_field("basic.highlights", &mut b.highlights, -100.0, 100.0, notes)?;
    clamp_field("basic.shadows", &mut b.shadows, -100.0, 100.0, notes)?;
    clamp_field("basic.whites", &mut b.whites, -100.0, 100.0, notes)?;
    clamp_field("basic.blacks", &mut b.blacks, -100.0, 100.0, notes)?;
    clamp_field("basic.temperature", &mut b.temperature, -100.0, 100.0, notes)?;
    clamp_field("basic.tint", &mut b.tint, -100.0, 100.0, notes)?;
    clamp_field("basic.vibrance", &mut b.vibrance, -100.0, 100.0, notes)?;
    clamp_field("basic.saturation", &mut b.saturation, -100.0, 100.0, notes)
}

fn normalize_adjust(a: &mut Adjust, notes: &mut Vec<Normalization>) -> Result<(), RecipeError> {
    clamp_field("masks[].adjust.exposure", &mut a.exposure, -5.0, 5.0, notes)?;
    clamp_field("masks[].adjust.contrast", &mut a.contrast, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.highlights", &mut a.highlights, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.shadows", &mut a.shadows, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.whites", &mut a.whites, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.blacks", &mut a.blacks, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.temperature", &mut a.temperature, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.tint", &mut a.tint, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.saturation", &mut a.saturation, -100.0, 100.0, notes)?;
    clamp_field("masks[].adjust.sharpen_amount", &mut a.sharpen_amount, 0.0, 150.0, notes)?;
    clamp_field("masks[].adjust.clarity", &mut a.clarity, -100.0, 100.0, notes)
}

fn normalize_mask(m: &mut Mask, notes: &mut Vec<Normalization>) -> Result<(), RecipeError> {
    match m {
        Mask::Linear { x0, y0, x1, y1, feather, adjust, .. } => {
            clamp_field("masks[].x0", x0, 0.0, 1.0, notes)?;
            clamp_field("masks[].y0", y0, 0.0, 1.0, notes)?;
            clamp_field("masks[].x1", x1, 0.0, 1.0, notes)?;
            clamp_field("masks[].y1", y1, 0.0, 1.0, notes)?;
            clamp_field("masks[].feather", feather, 0.0, 1.0, notes)?;
            normalize_adjust(adjust, notes)
        }
        Mask::Radial { cx, cy, rx, ry, feather, adjust, .. } => {
            clamp_field("masks[].cx", cx, 0.0, 1.0, notes)?;
            clamp_field("masks[].cy", cy, 0.0, 1.0, notes)?;
            clamp_field("masks[].rx", rx, 0.0, 1.0, notes)?;
            clamp_field("masks[].ry", ry, 0.0, 1.0, notes)?;
            clamp_field("masks[].feather", feather, 0.0, 1.0, notes)?;
            normalize_adjust(adjust, notes)
        }
        Mask::Range { lo, hi, smooth, adjust, .. } => {
            clamp_field("masks[].lo", lo, 0.0, 1.0, notes)?;
            clamp_field("masks[].hi", hi, 0.0, 1.0, notes)?;
            clamp_field("masks[].smooth", smooth, 0.0, 1.0, notes)?;
            normalize_adjust(adjust, notes)
        }
    }
}

fn validate_crop(c: Crop) -> Result<(), RecipeError> {
    for (name, v) in [("crop.x", c.x), ("crop.y", c.y), ("crop.w", c.w), ("crop.h", c.h)] {
        if !v.is_finite() {
            return Err(RecipeError::NonFinite(name));
        }
    }
    let inside = c.x >= 0.0 && c.y >= 0.0 && c.w > 0.0 && c.h > 0.0 && c.x + c.w <= 1.0 + 1e-6 && c.y + c.h <= 1.0 + 1e-6;
    if !inside {
        return Err(RecipeError::CropOutOfBounds { x: c.x, y: c.y, w: c.w, h: c.h });
    }
    let area = c.w * c.h;
    if area < 0.25 {
        return Err(RecipeError::CropTooSmall(area));
    }
    Ok(())
}

impl Recipe {
    /// Parse JSON and validate. Out-of-range values are clamped and reported; structural
    /// problems (NaN, >3 masks, bad crop, unknown fields) are rejected.
    pub fn from_json(json: &str) -> Result<Normalized, RecipeError> {
        let recipe: Recipe = serde_json::from_str(json).map_err(|e| RecipeError::Json(e.to_string()))?;
        recipe.normalize()
    }

    /// Validate and clamp an in-memory recipe.
    pub fn normalize(mut self) -> Result<Normalized, RecipeError> {
        if self.schema_version != crate::SCHEMA_VERSION {
            return Err(RecipeError::SchemaVersion(self.schema_version));
        }
        if self.masks.len() > 3 {
            return Err(RecipeError::TooManyMasks(self.masks.len()));
        }
        let mut notes = Vec::new();
        normalize_basic(&mut self.basic, &mut notes)?;
        for m in &mut self.masks {
            normalize_mask(m, &mut notes)?;
        }
        if let Some(c) = self.crop {
            validate_crop(c)?;
        }
        Ok(Normalized { recipe: self, notes })
    }

    /// True when only `basic` is used (everything Plan 1a can render).
    pub fn is_basic_only(&self) -> bool {
        self.masks.is_empty() && self.crop.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod from_json {
        use super::*;

        #[test]
        fn parses_minimal_recipe_with_defaults() {
            let n = Recipe::from_json(r#"{"schema_version":1}"#).unwrap();
            assert_eq!(n.recipe, Recipe::default());
        }

        #[test]
        fn clamps_out_of_range_exposure_and_reports_it() {
            let n = Recipe::from_json(r#"{"schema_version":1,"basic":{"exposure":9}}"#).unwrap();
            assert_eq!(
                n.notes,
                vec![Normalization { field: "basic.exposure".into(), from: 9.0, to: 5.0 }]
            );
        }

        #[test]
        fn rejects_unknown_fields() {
            let err = Recipe::from_json(r#"{"schema_version":1,"basic":{"exposur":1}}"#).unwrap_err();
            assert!(matches!(err, RecipeError::Json(_)), "{err:?}");
        }

        #[test]
        fn rejects_wrong_schema_version() {
            let err = Recipe::from_json(r#"{"schema_version":2}"#).unwrap_err();
            assert_eq!(err, RecipeError::SchemaVersion(2));
        }

        #[test]
        fn rejects_more_than_three_masks() {
            let mask = r#"{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5}"#;
            let json = format!(r#"{{"schema_version":1,"masks":[{mask},{mask},{mask},{mask}]}}"#);
            assert_eq!(Recipe::from_json(&json).unwrap_err(), RecipeError::TooManyMasks(4));
        }

        #[test]
        fn rejects_crop_smaller_than_quarter_area() {
            let err = Recipe::from_json(r#"{"schema_version":1,"crop":{"x":0,"y":0,"w":0.4,"h":0.4}}"#)
                .unwrap_err();
            assert!(matches!(err, RecipeError::CropTooSmall(_)), "{err:?}");
        }

        #[test]
        fn rejects_crop_outside_unit_square() {
            let err = Recipe::from_json(r#"{"schema_version":1,"crop":{"x":0.5,"y":0,"w":0.6,"h":1}}"#)
                .unwrap_err();
            assert!(matches!(err, RecipeError::CropOutOfBounds { .. }), "{err:?}");
        }

        #[test]
        fn accepts_a_full_area_crop() {
            let n = Recipe::from_json(r#"{"schema_version":1,"crop":{"x":0,"y":0,"w":1,"h":1}}"#).unwrap();
            assert!(n.notes.is_empty());
        }

        #[test]
        fn round_trips_through_serde() {
            let src = r#"{"schema_version":1,"basic":{"exposure":0.5,"contrast":10},"masks":[{"kind":"linear","x0":0,"y0":0,"x1":0,"y1":1,"feather":0.3,"adjust":{"exposure":-1}}]}"#;
            let n = Recipe::from_json(src).unwrap();
            let json = serde_json::to_string(&n.recipe).unwrap();
            let again = Recipe::from_json(&json).unwrap();
            assert_eq!(again.recipe, n.recipe);
        }
    }

    mod normalize {
        use super::*;

        #[test]
        fn rejects_nan_field() {
            let mut r = Recipe::default();
            r.basic.contrast = f32::NAN;
            assert_eq!(r.normalize().unwrap_err(), RecipeError::NonFinite("basic.contrast"));
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core recipe::`
Expected: `test result: ok. 10 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/recipe.rs
git commit -m "feat(engine): 레시피 스키마 v1과 정규화·검증"
```

---

## Task 8: 기본 패널 연산

**Files:**
- Modify: `engine/core/src/render/basic.rs`

- [ ] **Step 1: 실패하는 테스트와 구현**

`engine/core/src/render/basic.rs`:
```rust
//! Basic panel math. These are OUR definitions — direction-compatible with Lightroom, not
//! identical (spec D8, AC-E3b). Two stages: linear-light ops, then encoded-domain ops.

use crate::recipe::Basic;

/// Rec.709 luma of an encoded pixel (used as a tonal weight, not a colorimetric quantity).
pub fn luma(px: [f32; 3]) -> f32 {
    0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear stage: exposure (2^EV gain).
pub fn apply_exposure_linear(px: [f32; 3], ev: f32) -> [f32; 3] {
    let g = libm::exp2f(ev);
    [px[0] * g, px[1] * g, px[2] * g]
}

/// Linear stage: relative white balance. +temperature = warmer (more R, less B),
/// +tint = more magenta (less G). ±100 maps to ±20% channel gain.
pub fn apply_white_balance_linear(px: [f32; 3], temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature / 100.0;
    let n = tint / 100.0;
    [px[0] * (1.0 + 0.2 * t), px[1] * (1.0 - 0.2 * n), px[2] * (1.0 - 0.2 * t)]
}

/// Encoded stage: contrast around 0.5, then luma-weighted highlights/shadows/whites/blacks.
pub fn apply_tone_encoded(px: [f32; 3], b: &Basic) -> [f32; 3] {
    let c = b.contrast / 100.0;
    let contrasted = px.map(|x| 0.5 + (x - 0.5) * (1.0 + 0.6 * c));

    let l = luma(contrasted);
    let w_hi = smoothstep(0.5, 1.0, l);
    let w_sh = 1.0 - smoothstep(0.0, 0.5, l);
    let hi = b.highlights / 100.0;
    let sh = b.shadows / 100.0;
    let wh = b.whites / 100.0;
    let bl = b.blacks / 100.0;
    let one_minus_l = 1.0 - l;

    contrasted.map(|x| {
        let y = x
            + hi * 0.25 * w_hi * (1.0 - x)
            + sh * 0.25 * w_sh * (1.0 - x)
            + wh * 0.15 * l * l
            + bl * 0.15 * one_minus_l * one_minus_l;
        y.clamp(0.0, 1.0)
    })
}

/// Encoded stage: saturation scales chroma around luma; vibrance does the same but weighted
/// toward already-unsaturated pixels.
pub fn apply_color_encoded(px: [f32; 3], vibrance: f32, saturation: f32) -> [f32; 3] {
    let y = luma(px);
    let s = saturation / 100.0;
    let v = vibrance / 100.0;
    let mx = px[0].max(px[1]).max(px[2]);
    let mn = px[0].min(px[1]).min(px[2]);
    let satnorm = mx - mn;
    let k = (1.0 + s) * (1.0 + v * (1.0 - satnorm));
    px.map(|x| (y + (x - y) * k).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: [f32; 3] = [0.4, 0.4, 0.4];
    const WARM: [f32; 3] = [0.7, 0.5, 0.3];

    mod apply_exposure_linear {
        use super::*;

        #[test]
        fn plus_one_ev_doubles_linear_values() {
            let out = apply_exposure_linear([0.25, 0.1, 0.4], 1.0);
            assert_eq!(out, [0.5, 0.2, 0.8]);
        }

        #[test]
        fn zero_ev_is_identity() {
            assert_eq!(apply_exposure_linear(WARM, 0.0), WARM);
        }
    }

    mod apply_white_balance_linear {
        use super::*;

        #[test]
        fn positive_temperature_raises_red_over_blue() {
            let out = apply_white_balance_linear(GRAY, 50.0, 0.0);
            assert!(out[0] > GRAY[0] && out[2] < GRAY[2], "{out:?}");
        }

        #[test]
        fn positive_tint_lowers_green() {
            let out = apply_white_balance_linear(GRAY, 0.0, 50.0);
            assert!(out[1] < GRAY[1], "{out:?}");
        }
    }

    mod apply_tone_encoded {
        use super::*;

        fn basic(f: impl FnOnce(&mut Basic)) -> Basic {
            let mut b = Basic::default();
            f(&mut b);
            b
        }

        #[test]
        fn defaults_are_identity() {
            assert_eq!(apply_tone_encoded(WARM, &Basic::default()), WARM);
        }

        #[test]
        fn positive_contrast_pushes_values_away_from_mid_gray() {
            let out = apply_tone_encoded([0.7, 0.7, 0.7], &basic(|b| b.contrast = 50.0));
            assert!(out[0] > 0.7, "{out:?}");
        }

        #[test]
        fn negative_highlights_darken_bright_pixels() {
            let out = apply_tone_encoded([0.9, 0.9, 0.9], &basic(|b| b.highlights = -100.0));
            assert!(out[0] < 0.9, "{out:?}");
        }

        #[test]
        fn positive_shadows_lift_dark_pixels() {
            let out = apply_tone_encoded([0.1, 0.1, 0.1], &basic(|b| b.shadows = 100.0));
            assert!(out[0] > 0.1, "{out:?}");
        }

        #[test]
        fn output_stays_in_unit_range() {
            let out = apply_tone_encoded([0.95, 0.95, 0.95], &basic(|b| { b.whites = 100.0; b.highlights = 100.0; }));
            assert!(out.iter().all(|&x| (0.0..=1.0).contains(&x)), "{out:?}");
        }
    }

    mod apply_color_encoded {
        use super::*;

        #[test]
        fn negative_saturation_reduces_chroma() {
            let out = apply_color_encoded(WARM, 0.0, -50.0);
            let chroma = |p: [f32; 3]| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
            assert!(chroma(out) < chroma(WARM), "{out:?}");
        }

        #[test]
        fn minus_100_saturation_yields_neutral_gray() {
            let out = apply_color_encoded(WARM, 0.0, -100.0);
            assert!((out[0] - out[1]).abs() < 1e-6 && (out[1] - out[2]).abs() < 1e-6, "{out:?}");
        }

        #[test]
        fn vibrance_boosts_low_saturation_pixels_more_than_saturated_ones() {
            let dull = [0.5, 0.45, 0.4];
            let vivid = [0.9, 0.3, 0.1];
            let chroma = |p: [f32; 3]| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
            let gain_dull = chroma(apply_color_encoded(dull, 100.0, 0.0)) / chroma(dull);
            let gain_vivid = chroma(apply_color_encoded(vivid, 100.0, 0.0)) / chroma(vivid);
            assert!(gain_dull > gain_vivid, "dull {gain_dull} vivid {gain_vivid}");
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core render::basic::`
Expected: `test result: ok. 12 passed`

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/render/basic.rs
git commit -m "feat(engine): 기본 패널 연산 (노출·WB·톤·생동감·채도)"
```

---

## Task 9: 렌더 파이프라인

**Files:**
- Modify: `engine/core/src/render/mod.rs`

- [ ] **Step 1: 실패하는 테스트와 구현**

`engine/core/src/render/mod.rs`:
```rust
//! Render pipeline with a fixed stage order (AC-E5d):
//! input normalization → global (linear ops, then encoded ops) → [range-mask eval → mask
//! composite → detail: Plan 1b] → final clamp. Input and output are encoded sRGB f32.

pub mod basic;

use crate::buffer::ImageF32;
use crate::color::{srgb_decode, srgb_encode};
use crate::recipe::Recipe;
use thiserror::Error;

/// Render failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    /// The recipe uses a feature this build does not render yet. Never silently ignored.
    #[error("recipe uses `{feature}` which is not rendered until {plan}")]
    NotYetSupported {
        /// Feature name.
        feature: &'static str,
        /// Plan that adds it.
        plan: &'static str,
    },
}

/// Render `recipe` onto `src` (encoded sRGB f32, any size). Output has the same size.
pub fn render(src: &ImageF32, recipe: &Recipe) -> Result<ImageF32, RenderError> {
    if !recipe.masks.is_empty() {
        return Err(RenderError::NotYetSupported { feature: "masks", plan: "Plan 1b" });
    }
    if recipe.crop.is_some() {
        return Err(RenderError::NotYetSupported { feature: "crop", plan: "Plan 1b" });
    }
    let b = &recipe.basic;
    Ok(src.map_pixels(|px| {
        // Stage 2a: linear light
        let lin = [srgb_decode(px[0]), srgb_decode(px[1]), srgb_decode(px[2])];
        let lin = basic::apply_exposure_linear(lin, b.exposure);
        let lin = basic::apply_white_balance_linear(lin, b.temperature, b.tint);
        let enc = [srgb_encode(lin[0]), srgb_encode(lin[1]), srgb_encode(lin[2])];
        // Stage 2b: encoded domain
        let enc = basic::apply_tone_encoded(enc, b);
        let enc = basic::apply_color_encoded(enc, b.vibrance, b.saturation);
        // Stage 6: final clamp (quantization happens only in buffer::quantize)
        enc.map(|x| x.clamp(0.0, 1.0))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::{Crop, Mask, RangeChannel};

    fn gradient(w: u32, h: u32) -> ImageF32 {
        let mut img = ImageF32::zeroed(w, h);
        for y in 0..h {
            for x in 0..w {
                let u = x as f32 / (w - 1) as f32;
                let v = y as f32 / (h - 1) as f32;
                img.set_pixel(x, y, [u, v, 0.5 * (u + v)]);
            }
        }
        img
    }

    #[test]
    fn identity_recipe_reproduces_input_within_f32_round_trip() {
        let src = gradient(16, 8);
        let out = render(&src, &Recipe::default()).unwrap();
        // decode→encode round trip is not bit-exact in f32, but must be sub-quantization.
        for (a, b) in src.samples().iter().zip(out.samples()) {
            assert!((a - b).abs() < 1.0 / 512.0, "{a} vs {b}");
        }
    }

    #[test]
    fn identity_recipe_quantizes_back_to_identical_bytes() {
        let src = gradient(16, 8);
        let out = render(&src, &Recipe::default()).unwrap();
        assert_eq!(out.to_rgba8(), src.to_rgba8());
    }

    #[test]
    fn plus_one_ev_brightens_mean_luma() {
        let src = gradient(16, 8);
        let mut r = Recipe::default();
        r.basic.exposure = 1.0;
        let out = render(&src, &r).unwrap();
        let mean = |img: &ImageF32| img.pixels().map(basic::luma).sum::<f32>() / (16.0 * 8.0);
        assert!(mean(&out) > mean(&src));
    }

    #[test]
    fn rendering_twice_is_bitwise_identical() {
        let src = gradient(32, 16);
        let mut r = Recipe::default();
        r.basic.exposure = 0.7;
        r.basic.contrast = 30.0;
        r.basic.temperature = -20.0;
        r.basic.saturation = 15.0;
        let a = render(&src, &r).unwrap();
        let b = render(&src, &r).unwrap();
        assert_eq!(a.to_rgba8(), b.to_rgba8());
    }

    #[test]
    fn masks_are_rejected_not_ignored_in_plan_1a() {
        let mut r = Recipe::default();
        r.masks.push(Mask::Range { channel: RangeChannel::Luminance, lo: 0.0, hi: 0.5, smooth: 0.1, invert: false, adjust: Default::default() });
        let err = render(&gradient(4, 4), &r).unwrap_err();
        assert_eq!(err, RenderError::NotYetSupported { feature: "masks", plan: "Plan 1b" });
    }

    #[test]
    fn crop_is_rejected_not_ignored_in_plan_1a() {
        let mut r = Recipe::default();
        r.crop = Some(Crop { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
        let err = render(&gradient(4, 4), &r).unwrap_err();
        assert_eq!(err, RenderError::NotYetSupported { feature: "crop", plan: "Plan 1b" });
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core render::`
Expected: `test result: ok. 18 passed` (basic 12 + mod 6). `identity_recipe_quantizes_back_to_identical_bytes`가 실패하면 `srgb_encode(srgb_decode(v))` 오차가 양자화 경계를 넘은 것 — Task 1의 round-trip 허용치(2e-6)를 다시 보고, 필요하면 인코드 상수의 f32 표현을 점검한다. 파이프라인을 바꾸지 말 것.

- [ ] **Step 3: 커밋**

```bash
git add engine/core/src/render/mod.rs
git commit -m "feat(engine): 렌더 파이프라인 (순서 고정, 미지원 기능은 명시적 에러)"
```

---

## Task 10: 해시

**Files:**
- Modify: `engine/core/src/hash.rs`

- [ ] **Step 1: 구현과 테스트**

```rust
//! SHA-256 of pixel buffers, hex-encoded — the golden fingerprint (AC-E1a).

use sha2::{Digest, Sha256};

/// Lower-case hex SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_empty_input_to_known_digest() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hashes_abc_to_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
```

Run: `cd engine && cargo test -p engine-core hash::`
Expected: `test result: ok. 2 passed`

- [ ] **Step 2: 커밋**

```bash
git add engine/core/src/hash.rs
git commit -m "feat(engine): SHA-256 hex"
```

---

## Task 11: 채점 핵심

**Files:**
- Modify: `engine/core/src/score.rs`

- [ ] **Step 1: 실패하는 테스트와 구현**

`engine/core/src/score.rs`:
```rust
//! Scoring (AC-S1a..k, S2, S3). Plan 1a: evaluation region = full image; leakage penalty = 0;
//! diagnostics = tone + color. Plan 1b adds crop/mask regions, detail/local/composition.

use crate::buffer::ImageF32;
use crate::color::{delta_e00, linear_srgb_to_lab_d50, srgb_decode, Lab};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tile edge in scoring-image pixels for the 95th-percentile perfect-score gate (AC-S1a).
pub const TILE: u32 = 64;
/// Max per-tile p95 ΔE00 for a perfect score. Fixed before the AC-S4 bench; recorded with
/// [`crate::SCORING_VERSION`] (AC-S1b). Initial value; re-tune only with a version bump.
pub const T_TILE: f64 = 2.0;
/// Pixels above this ΔE00 count as "big errors" (AC-S1a).
pub const BIG_ERROR_DE: f64 = 5.0;
/// Max fraction of big-error pixels for a perfect score.
pub const BIG_ERROR_MAX_RATIO: f64 = 0.01;

/// Which pixels the correction score is computed over (AC-S1i). Plan 1a: `Full` only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Region {
    /// Whole image.
    Full,
}

/// Scoring failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScoreError {
    /// Images differ in size.
    #[error("size mismatch: original {0}x{1}, answer {2}x{3}, player {4}x{5}")]
    SizeMismatch(u32, u32, u32, u32, u32, u32),
    /// Zero-sized input.
    #[error("empty image")]
    Empty,
}

/// Diagnostic indicators, 0..100 each; `None` = "not evaluated" for this challenge (AC-S2d).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostics {
    /// Lightness agreement (mean |ΔL| relative to the original's).
    pub tone: u8,
    /// Chroma agreement (mean √(Δa²+Δb²) relative to the original's).
    pub color: u8,
    /// Plan 1b.
    pub detail: Option<u8>,
    /// Plan 1b.
    pub local: Option<u8>,
    /// Plan 1b.
    pub composition: Option<u8>,
}

/// Score JSON (master plan §3.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// 0..100 integer shown to the player.
    pub correction_score: u8,
    /// Unrounded score before the perfect gate.
    pub raw_score: f64,
    /// All three perfect conditions met.
    pub perfect: bool,
    /// Mean ΔE00 player↔answer over the region.
    pub d_e: f64,
    /// Mean ΔE00 original↔answer over the region (the "difficulty").
    pub d_e_original: f64,
    /// Max over tiles of per-tile p95 ΔE00.
    pub tile_p95_max: f64,
    /// Fraction of region pixels with ΔE00 > [`BIG_ERROR_DE`].
    pub big_error_ratio: f64,
    /// Leakage penalty subtracted (Plan 1b; 0 here).
    pub leakage_penalty: f64,
    /// Composition score (Plan 1b).
    pub composition_score: Option<u8>,
    /// Diagnostic indicators.
    pub diagnostics: Diagnostics,
    /// [`crate::ENGINE_VERSION`].
    pub engine_version: String,
    /// [`crate::SCORING_VERSION`].
    pub scoring_version: String,
}

/// Encoded sRGB image → Lab D50 per pixel, row-major.
pub fn lab_image(img: &ImageF32) -> Vec<Lab> {
    img.pixels()
        .map(|p| linear_srgb_to_lab_d50(srgb_decode(p[0]), srgb_decode(p[1]), srgb_decode(p[2])))
        .collect()
}

/// Relative improvement score (AC-S1j, D4): `d == D` (no better than the original) → 0,
/// `d <= 1` (JND) → 100. Degenerate `D <= 1` challenges are ineligible; we still return a
/// defined value so the CLI can report them.
pub fn relative_score(d: f64, d_original: f64) -> f64 {
    if d_original <= 1.0 {
        return if d <= 1.0 { 100.0 } else { 0.0 };
    }
    (100.0 * (1.0 - (d - 1.0) / (d_original - 1.0))).clamp(0.0, 100.0)
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

/// Max over `TILE`×`TILE` tiles (partial tiles included) of the per-tile 95th percentile.
pub fn tile_p95_max(de: &[f64], width: u32, height: u32) -> f64 {
    let mut worst = 0.0f64;
    let mut ty = 0;
    while ty < height {
        let mut tx = 0;
        while tx < width {
            let mut vals: Vec<f64> = Vec::new();
            for y in ty..(ty + TILE).min(height) {
                for x in tx..(tx + TILE).min(width) {
                    vals.push(de[(y * width + x) as usize]);
                }
            }
            vals.sort_by(f64::total_cmp);
            let idx = ((0.95 * vals.len() as f64).ceil() as usize).max(1) - 1;
            worst = worst.max(vals[idx]);
            tx += TILE;
        }
        ty += TILE;
    }
    worst
}

fn round_half_up_u8(x: f64) -> u8 {
    (x.clamp(0.0, 100.0) + 0.5).floor() as u8
}

/// Score `player` against `answer`, normalized by how far `original` is from `answer`.
/// All three are encoded sRGB at scoring resolution and must share dimensions.
pub fn score(
    original: &ImageF32,
    answer: &ImageF32,
    player: &ImageF32,
    region: &Region,
) -> Result<Score, ScoreError> {
    let (w, h) = (answer.width(), answer.height());
    if (original.width(), original.height()) != (w, h) || (player.width(), player.height()) != (w, h) {
        return Err(ScoreError::SizeMismatch(
            original.width(), original.height(), w, h, player.width(), player.height(),
        ));
    }
    if w == 0 || h == 0 {
        return Err(ScoreError::Empty);
    }
    let Region::Full = region;

    let lab_o = lab_image(original);
    let lab_a = lab_image(answer);
    let lab_p = lab_image(player);

    let de_pa: Vec<f64> = lab_p.iter().zip(&lab_a).map(|(p, a)| delta_e00(*p, *a)).collect();
    let de_oa: Vec<f64> = lab_o.iter().zip(&lab_a).map(|(o, a)| delta_e00(*o, *a)).collect();

    let d_e = mean(&de_pa);
    let d_e_original = mean(&de_oa);
    let raw_score = relative_score(d_e, d_e_original);

    let tile_p95 = tile_p95_max(&de_pa, w, h);
    let big = de_pa.iter().filter(|&&d| d > BIG_ERROR_DE).count() as f64 / de_pa.len() as f64;
    let perfect = d_e <= 1.0 && tile_p95 <= T_TILE && big <= BIG_ERROR_MAX_RATIO;

    let final_score = if perfect { 100.0 } else { raw_score.min(99.0) };

    let dl_pa: Vec<f64> = lab_p.iter().zip(&lab_a).map(|(p, a)| (p.l - a.l).abs()).collect();
    let dl_oa: Vec<f64> = lab_o.iter().zip(&lab_a).map(|(o, a)| (o.l - a.l).abs()).collect();
    let dc_pa: Vec<f64> = lab_p.iter().zip(&lab_a).map(|(p, a)| libm::hypot(p.a - a.a, p.b - a.b)).collect();
    let dc_oa: Vec<f64> = lab_o.iter().zip(&lab_a).map(|(o, a)| libm::hypot(o.a - a.a, o.b - a.b)).collect();

    Ok(Score {
        correction_score: round_half_up_u8(final_score),
        raw_score,
        perfect,
        d_e,
        d_e_original,
        tile_p95_max: tile_p95,
        big_error_ratio: big,
        leakage_penalty: 0.0,
        composition_score: None,
        diagnostics: Diagnostics {
            tone: round_half_up_u8(relative_score(mean(&dl_pa), mean(&dl_oa))),
            color: round_half_up_u8(relative_score(mean(&dc_pa), mean(&dc_oa))),
            detail: None,
            local: None,
            composition: None,
        },
        engine_version: crate::ENGINE_VERSION.to_owned(),
        scoring_version: crate::SCORING_VERSION.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::Recipe;
    use crate::render::render;

    fn gradient(w: u32, h: u32) -> ImageF32 {
        let mut img = ImageF32::zeroed(w, h);
        for y in 0..h {
            for x in 0..w {
                let u = x as f32 / (w - 1) as f32;
                let v = y as f32 / (h - 1) as f32;
                img.set_pixel(x, y, [0.2 + 0.6 * u, 0.3 + 0.4 * v, 0.5]);
            }
        }
        img
    }

    fn answer_with_exposure(src: &ImageF32, ev: f32) -> ImageF32 {
        let mut r = Recipe::default();
        r.basic.exposure = ev;
        render(src, &r).unwrap()
    }

    mod relative_score {
        use super::*;

        #[test]
        fn original_distance_scores_zero() {
            assert_eq!(relative_score(6.0, 6.0), 0.0);
        }

        #[test]
        fn jnd_or_better_scores_100() {
            assert_eq!(relative_score(0.8, 6.0), 100.0);
        }

        #[test]
        fn halfway_scores_50() {
            assert!((relative_score(3.5, 6.0) - 50.0).abs() < 1e-9);
        }

        #[test]
        fn worse_than_original_clamps_to_zero() {
            assert_eq!(relative_score(9.0, 6.0), 0.0);
        }
    }

    mod tile_p95_max {
        use super::*;

        #[test]
        fn finds_a_small_bad_patch_that_the_mean_hides() {
            let (w, h) = (128u32, 128u32);
            let mut de = vec![0.2f64; (w * h) as usize];
            // 8x8 patch of ΔE 40 in the top-left tile (64 px of 4096 in that tile = 1.6%... p95 misses it)
            // so use a 16x16 patch = 256/4096 = 6.25% > 5%.
            for y in 0..16 {
                for x in 0..16 {
                    de[(y * w + x) as usize] = 40.0;
                }
            }
            let mean = de.iter().sum::<f64>() / de.len() as f64;
            assert!(mean < 1.0, "mean {mean} should look 'perfect'");
            assert!(tile_p95_max(&de, w, h) >= 40.0);
        }

        #[test]
        fn handles_partial_tiles() {
            let (w, h) = (70u32, 70u32);
            let de = vec![1.5f64; (w * h) as usize];
            assert_eq!(tile_p95_max(&de, w, h), 1.5);
        }
    }

    mod score {
        use super::*;

        #[test]
        fn submitting_the_original_scores_zero() {
            let src = gradient(96, 64);
            let ans = answer_with_exposure(&src, 1.0);
            let s = score(&src, &ans, &src, &Region::Full).unwrap();
            assert_eq!(s.correction_score, 0);
        }

        #[test]
        fn submitting_the_answer_is_perfect_100() {
            let src = gradient(96, 64);
            let ans = answer_with_exposure(&src, 1.0);
            let s = score(&src, &ans, &ans, &Region::Full).unwrap();
            assert!(s.perfect && s.correction_score == 100, "{s:?}");
        }

        #[test]
        fn half_the_correction_scores_between_1_and_99() {
            let src = gradient(96, 64);
            let ans = answer_with_exposure(&src, 1.0);
            let half = answer_with_exposure(&src, 0.5);
            let s = score(&src, &ans, &half, &Region::Full).unwrap();
            assert!((1..=99).contains(&s.correction_score), "{s:?}");
        }

        #[test]
        fn non_perfect_result_is_capped_at_99() {
            let src = gradient(96, 64);
            let ans = answer_with_exposure(&src, 1.0);
            let almost = answer_with_exposure(&src, 0.98);
            let s = score(&src, &ans, &almost, &Region::Full).unwrap();
            assert!(s.perfect || s.correction_score <= 99, "{s:?}");
        }

        #[test]
        fn rejects_size_mismatch() {
            let a = gradient(8, 8);
            let b = gradient(9, 8);
            let err = score(&a, &a, &b, &Region::Full).unwrap_err();
            assert!(matches!(err, ScoreError::SizeMismatch(..)), "{err:?}");
        }

        #[test]
        fn serializes_to_json_with_versions() {
            let src = gradient(16, 16);
            let ans = answer_with_exposure(&src, 0.5);
            let s = score(&src, &ans, &ans, &Region::Full).unwrap();
            let json = serde_json::to_string(&s).unwrap();
            assert!(json.contains("\"scoring_version\":\"scoring-0.1.0\""), "{json}");
        }
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cd engine && cargo test -p engine-core score::`
Expected: `test result: ok. 12 passed`

- [ ] **Step 3: 전체 core 검증**

Run: `cd engine && cargo test -p engine-core && cargo clippy -p engine-core --all-targets -- -D warnings && cargo fmt --all -- --check && bash ../scripts/lint-determinism.sh`
Expected: 모든 테스트 통과, clippy 경고 없음(`pedantic`은 warn이라 출력될 수 있으나 `-D warnings`에 걸리면 코드 수정 또는 근거 있는 `#[expect]`), fmt 통과, 린트 ok.

- [ ] **Step 4: 커밋**

```bash
git add engine/core/src/score.rs
git commit -m "feat(engine): 채점 핵심 (상대 척도, 3중 만점 조건, 톤·색 진단)"
```

---

## Task 12: CLI

**Files:**
- Modify: `engine/cli/src/main.rs`
- Create: `engine/cli/tests/cli.rs`

- [ ] **Step 1: CLI 구현**

`engine/cli/src/main.rs`:
```rust
//! `engine-cli`: build-time answer generation, scoring and golden hashes (master plan A8).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use engine_core::buffer::ImageF32;
use engine_core::decode::{decode_rgba8, encode_png_rgba8};
use engine_core::hash::sha256_hex;
use engine_core::recipe::Recipe;
use engine_core::render::render;
use engine_core::resample::downscale_encoded_to_long_edge;
use engine_core::score::{score, Region};
use engine_core::{ENGINE_VERSION, SCHEMA_VERSION, SCORING_VERSION};

#[derive(Parser)]
#[command(name = "engine-cli", version, about = "Retouch reference engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write a deterministic synthetic test image (PNG + JPEG q90).
    Fixture {
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 512)]
        width: u32,
        #[arg(long, default_value_t = 384)]
        height: u32,
    },
    /// Render a recipe onto an image and write a PNG.
    Render {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Lanczos3 linear-light downscale to a long edge, write PNG.
    Downscale {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 1024)]
        long_edge: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Render the answer at full size and at scoring size, write PNGs + meta.json.
    GenAnswer {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long, default_value_t = 1024)]
        long_edge: u32,
    },
    /// Score player vs answer, normalized by original. All three are downscaled to `long_edge`.
    Score {
        #[arg(long)]
        original: PathBuf,
        #[arg(long)]
        answer: PathBuf,
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 1024)]
        long_edge: u32,
    },
    /// Print SHA-256 of the decoded RGBA8 pixels.
    Hash {
        #[arg(long)]
        input: PathBuf,
    },
}

fn load_image(path: &Path) -> Result<ImageF32> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let rgba = decode_rgba8(&bytes).with_context(|| format!("decode {}", path.display()))?;
    Ok(ImageF32::from_rgba8(rgba.width, rgba.height, &rgba.data)?)
}

fn load_recipe(path: &Path) -> Result<Recipe> {
    let json = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let normalized = Recipe::from_json(&json)?;
    for n in &normalized.notes {
        eprintln!("note: {} clamped {} -> {}", n.field, n.from, n.to);
    }
    Ok(normalized.recipe)
}

fn write_png(path: &Path, img: &ImageF32) -> Result<()> {
    let png = encode_png_rgba8(img.width(), img.height(), &img.to_rgba8())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, png).with_context(|| format!("write {}", path.display()))
}

/// Gradient + eight saturated patches + a gray ramp: exercises tone and color paths.
fn make_fixture(width: u32, height: u32) -> ImageF32 {
    let mut img = ImageF32::zeroed(width, height);
    let patches: [[f32; 3]; 8] = [
        [0.9, 0.1, 0.1], [0.9, 0.5, 0.1], [0.9, 0.9, 0.1], [0.1, 0.8, 0.2],
        [0.1, 0.8, 0.8], [0.1, 0.2, 0.9], [0.5, 0.1, 0.8], [0.9, 0.2, 0.6],
    ];
    for y in 0..height {
        for x in 0..width {
            let u = x as f32 / (width.max(2) - 1) as f32;
            let v = y as f32 / (height.max(2) - 1) as f32;
            let px = if v < 0.25 {
                patches[((u * 8.0) as usize).min(7)]
            } else if v < 0.4 {
                [u, u, u]
            } else {
                [0.15 + 0.7 * u, 0.25 + 0.5 * (1.0 - v), 0.35 + 0.4 * v]
            };
            img.set_pixel(x, y, px);
        }
    }
    img
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Fixture { out_dir, width, height } => {
            fs::create_dir_all(&out_dir)?;
            let img = make_fixture(width, height);
            write_png(&out_dir.join("fixture.png"), &img)?;
            let rgba = img.to_rgba8();
            let mut jpg = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 90);
            image::ImageEncoder::write_image(enc, &rgba, width, height, image::ExtendedColorType::Rgba8)?;
            fs::write(out_dir.join("fixture.jpg"), jpg)?;
            println!("wrote fixture.png fixture.jpg ({width}x{height})");
        }
        Cmd::Render { input, recipe, output } => {
            let src = load_image(&input)?;
            let r = load_recipe(&recipe)?;
            let out = render(&src, &r)?;
            write_png(&output, &out)?;
            println!("{}", sha256_hex(&out.to_rgba8()));
        }
        Cmd::Downscale { input, long_edge, output } => {
            let src = load_image(&input)?;
            let out = downscale_encoded_to_long_edge(&src, long_edge)?;
            write_png(&output, &out)?;
            println!("{}x{} {}", out.width(), out.height(), sha256_hex(&out.to_rgba8()));
        }
        Cmd::GenAnswer { input, recipe, out_dir, long_edge } => {
            let t0 = Instant::now();
            let src = load_image(&input)?;
            let r = load_recipe(&recipe)?;
            let full = render(&src, &r)?;
            let small = downscale_encoded_to_long_edge(&full, long_edge)?;
            fs::create_dir_all(&out_dir)?;
            write_png(&out_dir.join("answer_full.png"), &full)?;
            write_png(&out_dir.join(format!("answer_{long_edge}.png")), &small)?;
            let meta = serde_json::json!({
                "engine_version": ENGINE_VERSION,
                "scoring_version": SCORING_VERSION,
                "schema_version": SCHEMA_VERSION,
                "input_sha256_rgba8": sha256_hex(&src.to_rgba8()),
                "answer_sha256_rgba8_full": sha256_hex(&full.to_rgba8()),
                "answer_sha256_rgba8_small": sha256_hex(&small.to_rgba8()),
                "width": full.width(), "height": full.height(),
                "small_width": small.width(), "small_height": small.height(),
                "render_ms": t0.elapsed().as_millis(),
            });
            fs::write(out_dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)?;
            println!("{}", meta["answer_sha256_rgba8_small"].as_str().unwrap_or_default());
        }
        Cmd::Score { original, answer, player, output, long_edge } => {
            let t0 = Instant::now();
            let o = downscale_encoded_to_long_edge(&load_image(&original)?, long_edge)?;
            let a = downscale_encoded_to_long_edge(&load_image(&answer)?, long_edge)?;
            let p = downscale_encoded_to_long_edge(&load_image(&player)?, long_edge)?;
            let s = score(&o, &a, &p, &Region::Full)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, serde_json::to_string_pretty(&s)?)?;
            eprintln!("scored in {} ms", t0.elapsed().as_millis());
            println!("{}", s.correction_score);
        }
        Cmd::Hash { input } => {
            let img = load_image(&input)?;
            println!("{}", sha256_hex(&img.to_rgba8()));
        }
    }
    Ok(())
}

// `anyhow` needs `std::error::Error`; the core errors implement it via thiserror, but
// `image::ImageError` inside `DecodeError` must also be Send + Sync — it is.
#[allow(dead_code)]
fn _assert_error_bounds() -> Result<()> {
    Err(anyhow!("never called"))
}
```

`unwrap_or_default()`는 `Option<&str>`에 대해 안전하지만 lint(`unwrap_used`)는 `unwrap()`만 잡는다. `_assert_error_bounds`는 삭제해도 된다 — 컴파일 확인 후 제거한다.

Run: `cd engine && cargo build -p engine-cli`
Expected: `Finished`. `image::ImageEncoder::write_image(enc, ...)` 시그니처가 맞지 않으면 `enc.write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)?` 형태로 바꾼다(0.25는 `self`를 소비하는 트레이트 메서드).

- [ ] **Step 2: 통합 테스트 작성**

`engine/cli/tests/cli.rs`:
```rust
//! End-to-end: fixture → gen-answer → score(original)=0, score(answer)=100, hash stable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_engine-cli"))
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retouch-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn stdout(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn fixture_then_gen_answer_then_score_original_is_zero_and_answer_is_hundred() {
    let dir = tmp("e2e");
    stdout(cli().args(["fixture", "--out-dir"]).arg(&dir));
    let recipe = dir.join("r.json");
    fs::write(&recipe, r#"{"schema_version":1,"basic":{"exposure":0.8,"contrast":25,"temperature":30}}"#).unwrap();

    let ans_dir = dir.join("ans");
    let small_hash = stdout(cli().args(["gen-answer", "--input"]).arg(dir.join("fixture.jpg")).arg("--recipe").arg(&recipe).arg("--out-dir").arg(&ans_dir).args(["--long-edge", "256"]));
    assert_eq!(small_hash.len(), 64);

    let orig_score = stdout(
        cli().args(["score", "--original"]).arg(dir.join("fixture.jpg")).arg("--answer").arg(ans_dir.join("answer_full.png")).arg("--player").arg(dir.join("fixture.jpg")).arg("--output").arg(dir.join("s0.json")).args(["--long-edge", "256"]),
    );
    assert_eq!(orig_score, "0");

    let ans_score = stdout(
        cli().args(["score", "--original"]).arg(dir.join("fixture.jpg")).arg("--answer").arg(ans_dir.join("answer_full.png")).arg("--player").arg(ans_dir.join("answer_full.png")).arg("--output").arg(dir.join("s1.json")).args(["--long-edge", "256"]),
    );
    assert_eq!(ans_score, "100");
}

#[test]
fn render_hash_is_stable_across_two_runs() {
    let dir = tmp("stable");
    stdout(cli().args(["fixture", "--out-dir"]).arg(&dir));
    let recipe = dir.join("r.json");
    fs::write(&recipe, r#"{"schema_version":1,"basic":{"exposure":-0.5,"saturation":40}}"#).unwrap();
    let h1 = stdout(cli().args(["render", "--input"]).arg(dir.join("fixture.png")).arg("--recipe").arg(&recipe).arg("--output").arg(dir.join("a.png")));
    let h2 = stdout(cli().args(["render", "--input"]).arg(dir.join("fixture.png")).arg("--recipe").arg(&recipe).arg("--output").arg(dir.join("b.png")));
    assert_eq!(h1, h2);
}

#[test]
fn render_rejects_recipe_with_masks_in_plan_1a() {
    let dir = tmp("masks");
    stdout(cli().args(["fixture", "--out-dir"]).arg(&dir));
    let recipe = dir.join("r.json");
    fs::write(&recipe, r#"{"schema_version":1,"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.3,"ry":0.3,"feather":0.5}]}"#).unwrap();
    let out = cli().args(["render", "--input"]).arg(dir.join("fixture.png")).arg("--recipe").arg(&recipe).arg("--output").arg(dir.join("a.png")).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("masks"));
}
```

- [ ] **Step 3: 테스트 실행**

Run: `cd engine && cargo test -p engine-cli`
Expected: `test result: ok. 3 passed`

- [ ] **Step 4: 커밋**

```bash
git add engine/cli
git commit -m "feat(engine): CLI (fixture, render, downscale, gen-answer, score, hash)"
```

---

## Task 13: 골든 픽스처와 네이티브 골든 테스트

**Files:**
- Create: `engine/golden/recipes/identity.json`, `exposure_plus1.json`, `warm_contrast.json`
- Create: `engine/golden/regen.sh`
- Create: `engine/core/tests/golden.rs`
- Generated & committed: `engine/golden/fixture.jpg`, `engine/golden/fixture.png`, `engine/golden/expected.json`

- [ ] **Step 1: 레시피 3개**

`engine/golden/recipes/identity.json`:
```json
{ "schema_version": 1 }
```
`engine/golden/recipes/exposure_plus1.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 1.0 } }
```
`engine/golden/recipes/warm_contrast.json`:
```json
{ "schema_version": 1, "basic": { "exposure": 0.3, "contrast": 35, "highlights": -40, "shadows": 25, "whites": 10, "blacks": -15, "temperature": 35, "tint": 8, "vibrance": 20, "saturation": -10 } }
```

- [ ] **Step 2: 재생성 스크립트**

`engine/golden/regen.sh`:
```bash
#!/usr/bin/env bash
# Regenerate expected.json with the native CLI. Run after ANY render/score math change
# (and bump ENGINE_VERSION / SCORING_VERSION first).
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release -p engine-cli --manifest-path ../Cargo.toml
CLI=../target/release/engine-cli
[ -f fixture.jpg ] || $CLI fixture --out-dir . --width 512 --height 384

ORIG_HASH=$($CLI hash --input fixture.jpg)
echo '{' > expected.json
echo "  \"engine_version\": \"$(grep -o 'engine-[0-9.]*' ../core/src/lib.rs | head -1)\"," >> expected.json
echo "  \"scoring_version\": \"$(grep -o 'scoring-[0-9.]*' ../core/src/lib.rs | head -1)\"," >> expected.json
echo "  \"fixture_sha256_rgba8\": \"$ORIG_HASH\"," >> expected.json
echo '  "long_edge": 256,' >> expected.json
echo '  "recipes": {' >> expected.json
first=1
for f in recipes/*.json; do
  name=$(basename "$f" .json)
  out="out/$name"
  $CLI gen-answer --input fixture.jpg --recipe "$f" --out-dir "$out" --long-edge 256 >/dev/null
  small=$(python3 -c "import json;print(json.load(open('$out/meta.json'))['answer_sha256_rgba8_small'])")
  full=$(python3 -c "import json;print(json.load(open('$out/meta.json'))['answer_sha256_rgba8_full'])")
  $CLI score --original fixture.jpg --answer "$out/answer_full.png" --player fixture.jpg --output "$out/score_original.json" --long-edge 256 >/dev/null 2>&1
  s0=$(python3 -c "import json;print(json.load(open('$out/score_original.json'))['correction_score'])")
  [ $first = 1 ] || echo ',' >> expected.json; first=0
  printf '    "%s": { "answer_sha256_full": "%s", "answer_sha256_256": "%s", "score_original": %s }' "$name" "$full" "$small" "$s0" >> expected.json
done
echo '' >> expected.json
echo '  }' >> expected.json
echo '}' >> expected.json
rm -rf out
cat expected.json
```

Run: `bash engine/golden/regen.sh`
Expected: `expected.json` 출력. `identity`의 `score_original`은 정의상 부적격(D≤1)이며 `relative_score`가 0 또는 100을 낼 수 있다 — 값이 무엇이든 커밋되고 다른 OS/wasm에서 **같아야** 한다. (python3가 없으면 `python`으로 바꾼다.)

- [ ] **Step 3: 네이티브 골든 테스트**

`engine/core/tests/golden.rs`:
```rust
//! Renders every golden recipe onto the committed fixture and compares hashes/scores with
//! expected.json. Runs on ubuntu/windows/macos in CI (AC-E1a native half).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use engine_core::buffer::ImageF32;
use engine_core::decode::decode_rgba8;
use engine_core::hash::sha256_hex;
use engine_core::recipe::Recipe;
use engine_core::render::render;
use engine_core::resample::downscale_encoded_to_long_edge;
use engine_core::score::{score, Region};
use serde::Deserialize;

#[derive(Deserialize)]
struct Expected {
    engine_version: String,
    scoring_version: String,
    fixture_sha256_rgba8: String,
    long_edge: u32,
    recipes: BTreeMap<String, ExpectedRecipe>,
}

#[derive(Deserialize)]
struct ExpectedRecipe {
    answer_sha256_full: String,
    answer_sha256_256: String,
    score_original: u8,
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../golden")
}

fn load_fixture() -> ImageF32 {
    let bytes = fs::read(golden_dir().join("fixture.jpg")).unwrap();
    let rgba = decode_rgba8(&bytes).unwrap();
    ImageF32::from_rgba8(rgba.width, rgba.height, &rgba.data).unwrap()
}

fn expected() -> Expected {
    serde_json::from_str(&fs::read_to_string(golden_dir().join("expected.json")).unwrap()).unwrap()
}

#[test]
fn versions_match_expected_json() {
    let e = expected();
    assert_eq!((e.engine_version.as_str(), e.scoring_version.as_str()), (engine_core::ENGINE_VERSION, engine_core::SCORING_VERSION));
}

#[test]
fn fixture_decodes_to_the_committed_hash() {
    let e = expected();
    assert_eq!(sha256_hex(&load_fixture().to_rgba8()), e.fixture_sha256_rgba8);
}

#[test]
fn every_golden_recipe_reproduces_its_hashes_and_original_score() {
    let e = expected();
    let src = load_fixture();
    for (name, exp) in &e.recipes {
        let json = fs::read_to_string(golden_dir().join(format!("recipes/{name}.json"))).unwrap();
        let recipe = Recipe::from_json(&json).unwrap().recipe;
        let full = render(&src, &recipe).unwrap();
        assert_eq!(sha256_hex(&full.to_rgba8()), exp.answer_sha256_full, "{name}: full hash");
        let small = downscale_encoded_to_long_edge(&full, e.long_edge).unwrap();
        assert_eq!(sha256_hex(&small.to_rgba8()), exp.answer_sha256_256, "{name}: small hash");
        let o = downscale_encoded_to_long_edge(&src, e.long_edge).unwrap();
        let s = score(&o, &small, &o, &Region::Full).unwrap();
        assert_eq!(s.correction_score, exp.score_original, "{name}: original score");
        let s_ans = score(&o, &small, &small, &Region::Full).unwrap();
        assert_eq!(s_ans.correction_score, 100, "{name}: answer must score 100");
    }
}
```

Run: `cd engine && cargo test -p engine-core --test golden`
Expected: `test result: ok. 3 passed`

- [ ] **Step 4: 커밋** (픽스처·expected.json 포함)

```bash
git add engine/golden engine/core/tests
git commit -m "test(engine): 골든 픽스처와 해시·점수 회귀 테스트"
```

---

## Task 14: WASM 바인딩과 wasm==native 결정성 테스트

**Files:**
- Modify: `engine/wasm/src/lib.rs`
- Create: `engine/wasm/test/determinism.test.mjs`

- [ ] **Step 1: 바인딩 구현**

`engine/wasm/src/lib.rs`:
```rust
//! wasm-bindgen surface (master plan §3.4). Thin: parse args, call engine-core, return bytes/JSON.

use engine_core::buffer::ImageF32;
use engine_core::decode::decode_rgba8;
use engine_core::recipe::Recipe;
use engine_core::render::render;
use engine_core::resample::downscale_encoded_to_long_edge;
use engine_core::score::{score, Region};
use wasm_bindgen::prelude::*;

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Decoded image handed to JS.
#[wasm_bindgen(getter_with_clone)]
pub struct DecodedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 bytes.
    pub rgba: Vec<u8>,
}

/// Decode JPEG/PNG bytes with the reference decoder (never the browser's).
#[wasm_bindgen]
pub fn decode_image(bytes: &[u8]) -> Result<DecodedImage, JsValue> {
    let d = decode_rgba8(bytes).map_err(js_err)?;
    Ok(DecodedImage { width: d.width, height: d.height, rgba: d.data })
}

/// Render `recipe_json` onto RGBA8 pixels; returns RGBA8 of the same size.
#[wasm_bindgen]
pub fn render_rgba8(width: u32, height: u32, rgba: &[u8], recipe_json: &str) -> Result<Vec<u8>, JsValue> {
    let src = ImageF32::from_rgba8(width, height, rgba).map_err(js_err)?;
    let recipe = Recipe::from_json(recipe_json).map_err(js_err)?.recipe;
    let out = render(&src, &recipe).map_err(js_err)?;
    Ok(out.to_rgba8())
}

/// Lanczos3 linear-light downscale so the long edge equals `long_edge`.
#[wasm_bindgen]
pub fn downscale_rgba8(width: u32, height: u32, rgba: &[u8], long_edge: u32) -> Result<DecodedImage, JsValue> {
    let src = ImageF32::from_rgba8(width, height, rgba).map_err(js_err)?;
    let out = downscale_encoded_to_long_edge(&src, long_edge).map_err(js_err)?;
    Ok(DecodedImage { width: out.width(), height: out.height(), rgba: out.to_rgba8() })
}

/// Score JSON for `player` vs `answer` normalized by `original` (all same size, RGBA8).
/// `region_json` is `{"kind":"full"}` in Plan 1a.
#[wasm_bindgen]
pub fn score_rgba8(
    width: u32,
    height: u32,
    original: &[u8],
    answer: &[u8],
    player: &[u8],
    region_json: &str,
) -> Result<String, JsValue> {
    let o = ImageF32::from_rgba8(width, height, original).map_err(js_err)?;
    let a = ImageF32::from_rgba8(width, height, answer).map_err(js_err)?;
    let p = ImageF32::from_rgba8(width, height, player).map_err(js_err)?;
    let region: Region = serde_json::from_str(region_json).map_err(js_err)?;
    let s = score(&o, &a, &p, &region).map_err(js_err)?;
    serde_json::to_string(&s).map_err(js_err)
}

/// Lower-case hex SHA-256.
#[wasm_bindgen]
pub fn sha256_hex(bytes: &[u8]) -> String {
    engine_core::hash::sha256_hex(bytes)
}

/// `{"engine":"...","scoring":"...","schema":1}`.
#[wasm_bindgen]
pub fn versions() -> String {
    format!(
        r#"{{"engine":"{}","scoring":"{}","schema":{}}}"#,
        engine_core::ENGINE_VERSION,
        engine_core::SCORING_VERSION,
        engine_core::SCHEMA_VERSION
    )
}
```

Run: `cd engine && cargo build -p engine-wasm --target wasm32-unknown-unknown`
Expected: `Finished`. (`image` 크레이트의 jpeg/png가 wasm32에서 컴파일되는지 여기서 확인된다. 실패하면 `image` 대신 `zune-jpeg` + `png` 크레이트를 직접 쓰는 방향으로 전환하고 master plan A2를 갱신한다.)

- [ ] **Step 2: wasm-pack 빌드**

Run: `cd engine && wasm-pack build wasm --target nodejs --release`
Expected: `engine/wasm/pkg/engine_wasm.js`, `engine_wasm_bg.wasm` 생성. (`pkg/`는 `.gitignore`에 이미 있음.)

- [ ] **Step 3: 결정성 테스트**

`engine/wasm/test/determinism.test.mjs`:
```js
// wasm output must equal the native golden (AC-E1a, wasm half). Run: node --test engine/wasm/test
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const golden = resolve(here, '../../golden')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)

const expected = JSON.parse(readFileSync(resolve(golden, 'expected.json'), 'utf8'))
const fixtureBytes = new Uint8Array(readFileSync(resolve(golden, 'fixture.jpg')))
const fixture = wasm.decode_image(fixtureBytes)

test('versions match expected.json', () => {
  const v = JSON.parse(wasm.versions())
  assert.equal(v.engine, expected.engine_version)
  assert.equal(v.scoring, expected.scoring_version)
})

test('wasm decoder reproduces the fixture hash', () => {
  assert.equal(wasm.sha256_hex(fixture.rgba), expected.fixture_sha256_rgba8)
})

for (const [name, exp] of Object.entries(expected.recipes)) {
  test(`recipe ${name}: wasm render + downscale hashes equal native golden`, () => {
    const recipe = readFileSync(resolve(golden, `recipes/${name}.json`), 'utf8')
    const full = wasm.render_rgba8(fixture.width, fixture.height, fixture.rgba, recipe)
    assert.equal(wasm.sha256_hex(full), exp.answer_sha256_full, 'full')
    const small = wasm.downscale_rgba8(fixture.width, fixture.height, full, expected.long_edge)
    assert.equal(wasm.sha256_hex(small.rgba), exp.answer_sha256_256, 'small')
  })

  test(`recipe ${name}: wasm scores original=${exp.score_original} and answer=100`, () => {
    const recipe = readFileSync(resolve(golden, `recipes/${name}.json`), 'utf8')
    const full = wasm.render_rgba8(fixture.width, fixture.height, fixture.rgba, recipe)
    const small = wasm.downscale_rgba8(fixture.width, fixture.height, full, expected.long_edge)
    const orig = wasm.downscale_rgba8(fixture.width, fixture.height, fixture.rgba, expected.long_edge)
    const region = JSON.stringify({ kind: 'full' })
    const s0 = JSON.parse(wasm.score_rgba8(small.width, small.height, orig.rgba, small.rgba, orig.rgba, region))
    assert.equal(s0.correction_score, exp.score_original)
    const s1 = JSON.parse(wasm.score_rgba8(small.width, small.height, orig.rgba, small.rgba, small.rgba, region))
    assert.equal(s1.correction_score, 100)
    assert.equal(s1.perfect, true)
  })
}
```

Run: `node --test engine/wasm/test`
Expected: 모든 테스트 `ok`. 해시 하나라도 다르면 **결정성 위반**이다 — 가장 먼저 `scripts/lint-determinism.sh`, 그다음 `image` 디코더의 타깃별 SIMD 경로(`zune-jpeg` 기능 플래그)를 의심한다. 절대 expected.json을 wasm 값으로 덮어써서 "고치지" 말 것.

- [ ] **Step 4: 커밋**

```bash
git add engine/wasm
git commit -m "feat(engine): wasm 바인딩과 wasm==native 골든 결정성 테스트"
```

---

## Task 15: CI 통과 확인과 성능 기준선

**Files:**
- Modify (필요 시): `.github/workflows/ci.yml` — 이미 `engine/Cargo.toml` 존재 시 3-OS 엔진 잡을 돌리도록 작성되어 있다.

- [ ] **Step 1: 로컬 전체 검증**

```bash
cd engine
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
bash ../scripts/lint-determinism.sh
wasm-pack build wasm --target nodejs --release && node --test wasm/test
```
Expected: 전부 성공.

- [ ] **Step 2: 성능 기준선 측정 (AC-S5a 준비)**

2048px 입력으로 네이티브 릴리스 시간을 잰다(모바일 wasm은 Plan 2에서 실측; 여기서는 연산량 감각과 회귀 기준만 남긴다):
```bash
cd engine
cargo build --release -p engine-cli
./target/release/engine-cli fixture --out-dir /tmp/perf --width 2048 --height 1365
echo '{"schema_version":1,"basic":{"exposure":0.5,"contrast":20}}' > /tmp/perf/r.json
./target/release/engine-cli gen-answer --input /tmp/perf/fixture.jpg --recipe /tmp/perf/r.json --out-dir /tmp/perf/ans
./target/release/engine-cli score --original /tmp/perf/fixture.jpg --answer /tmp/perf/ans/answer_full.png --player /tmp/perf/fixture.jpg --output /tmp/perf/s.json
```
Expected: `scored in N ms` 출력. N을 `engine/golden/PERF.md`에 기기명과 함께 기록한다(예: "Ryzen 5 5600X, native release: render 2048 = 120 ms, score(3×decode+3×downscale+ΔE) = 480 ms"). wasm은 보통 네이티브의 2~4배이므로 N × 3이 2000ms를 넘으면 Plan 1b에서 최적화 항목(픽셀 루프 SIMD, Lab 캐시)을 우선순위로 올린다.

- [ ] **Step 3: 푸시 후 CI 확인**

```bash
git push
gh run watch --exit-status
```
Expected: `CI` 워크플로에서 `Engine · ubuntu-latest / windows-latest / macos-latest` 3개 잡 모두 성공. 세 OS의 `cargo test`(골든 해시)와 `node --test`가 통과하면 **AC-E1a의 CI 절반이 성립**한다(실기기 브라우저 검증은 Plan 2).

- [ ] **Step 4: 커밋**

```bash
git add engine/golden/PERF.md
git commit -m "docs(engine): 성능 기준선 기록"
git push
```

---

## 자체 검토 결과 (writing-plans Self-Review)

**1. 스펙 커버리지**
| AC | 태스크 | 비고 |
|----|--------|------|
| E1a 결정성(해시 일치) | 13, 14, 15 | 3-OS 네이티브 + wasm(node). 실기기 브라우저는 Plan 2 |
| E1c 입력 정규화·해시 | 6, 12(gen-answer meta) | EXIF 적용은 콘텐츠 빌드(Plan 3) |
| E1e f32 중간값·단일 양자화 | 4, 9 | `buffer::quantize` 유일 |
| E3c 경계 입력에서 유한 결과 | 7(NaN 거부) | 픽셀 경계값(무채색 등)은 Task 8 테스트 일부; 1b에서 확장 |
| E3d 잘못된 값 거부/정규화 보고 | 7 | `Normalization` 목록 |
| S1a 3중 만점 | 11 | `T_TILE=2.0` 초기값 |
| S1b T_TILE 버전 기록 | 11 | `SCORING_VERSION` |
| S1c 미충족 ≤99 | 11 | |
| S1d 정답=무손실 PNG/재생성 | 6, 12 | |
| S1e "원본 대비 개선" 명칭 | 11(`correction_score`) | UI 문구는 Plan 3 |
| S1f 색 변환 경로 | 2, 3 | |
| S1g Sharma 벡터 1e-4 | 3 | |
| S1h 2048→1024 Lanczos3 규칙 고정 | 5 | |
| S1j 상대 척도 | 11 | E=전체(Full); 마스크 영역은 1b |
| S2 톤·색 진단 | 11 | detail/local/composition = None |
| S3 원본=0, 정답=100 | 11, 12, 13, 14 | |
| S5a 2초 | 15 | 기준선만; 실측은 Plan 2 |
| 마스터 A1, A2, A3, A4, A6, A8, §5 | 0, 6, 9, 11, 13 | |

빠진 것(의도적, 다음 계획): 마스크·크롭 렌더, 평가 영역·누출 페널티(S1i/k), 진단 3종·상위 오차 영역(S2c/S6), XMP(E4), 콘텐츠 적격성(C2), 성능 실측(S5a 본체) → Plan 1b/2.

**2. 플레이스홀더 스캔** — `todo!`/`TODO`/"나중에" 없음. 1a가 렌더하지 않는 기능은 데이터 타입 + 명시적 `NotYetSupported` 에러 + 이를 검증하는 테스트로 처리(Task 9, 12).

**3. 타입 일관성** — `ImageF32::{width,height,pixel,set_pixel,pixels,samples,map_pixels,to_rgba8,from_rgba8,zeroed}`가 Task 4에 정의되고 5·9·11·12·13·14에서 같은 이름으로 사용됨. `Recipe::from_json → Normalized{recipe,notes}`, `render(&ImageF32,&Recipe) -> Result<ImageF32,RenderError>`, `score(&o,&a,&p,&Region) -> Result<Score,ScoreError>`, `downscale_encoded_to_long_edge(&ImageF32,u32)`, `sha256_hex(&[u8]) -> String` 모두 정의와 호출이 일치. `DecodedImage`는 wasm에서만 사용.

**알려진 검증 필요 지점(구현자가 첫 실행에서 확인할 것)**
- Task 3 Sharma 표 전사 정확성(테스트가 잡아준다).
- Task 9 `identity_recipe_quantizes_back_to_identical_bytes`: f32 인코드/디코드 라운드트립이 모든 바이트에서 양자화 경계를 넘지 않는지(Task 1 테스트가 2e-6 허용치로 먼저 검증).
- Task 12 `image` 0.25의 `ImageEncoder::write_image` 호출 형태.
- Task 14 `image`(zune-jpeg) wasm32 컴파일과 SIMD 경로 결정성 — 실패 시 A2 갱신.

---

## 실행 인수인계

이 계획은 **승인 대기** 상태다. 승인 후 실행 방식:

1. **Subagent-Driven (권장)** — 태스크마다 새 서브에이전트, 사이사이 리뷰 (`superpowers:subagent-driven-development`).
2. **Inline** — 이 세션에서 체크포인트마다 확인하며 순차 실행 (`superpowers:executing-plans`).

승인 전에 이 문서는 Architect/Critic 서브에이전트 + Codex gpt-6-astra 합의 루프를 거친다(마스터 플랜 §6).
