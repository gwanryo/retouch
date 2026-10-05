# Plan 1a-1: 수치·이미지 기반 (Task 0~4)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development(권장) 또는 superpowers:executing-plans. Rust 코드 전에 `rust-best-practices` 로드. 공통 계약·사전 조건·TDD 예외는 `01-engine-foundation.md`에 있고 이 문서의 모든 태스크에 적용된다.

**목표:** 워크스페이스·툴체인·결정성 lint·CI(fmt/clippy/test 3-OS)를 세우고, 색 과학(확장 sRGB, Lab D50, CIEDE2000), 픽셀 버퍼와 단일 양자화, `scoring_image`(Lanczos3), 참조 디코드(zune-jpeg 스칼라·png)를 만든다.

**선행 조건:** 없음(첫 분할). `main` 최신.

**브랜치 / PR:** `feat/engine-1a-1-numeric-image` / "Plan 1a-1: 수치·이미지 기반"

**커버 AC:** S1f, S1g(네이티브), S1h, E1e(양자화 규칙), E1c(디코드 부분), S1d(PNG 왕복), 마스터 A1·A2·A3·A4, §5.1~5.5(libm·FMA lint, 누적 순서, `--locked`·`Cargo.lock`, `RUSTFLAGS`·`.cargo/config` 금지, 단일 `quantize`).

**이 분할이 끝내지 않는 것:** 마스터 §5.6 골든(렌더·채점 이미지 해시와 Score JSON의 3-OS·wasm 일치)과 버전 가드(`ENGINE_VERSION`/`SCORING_VERSION` 미범프 변경 거부)는 **분할 4(Task 12a~14)** 담당이다. 분할 1 완료는 §5 전체 완료를 뜻하지 않는다. §5.3의 "크레이트 버전 변경 시 범프" 강제도 분할 4의 가드가 맡는다(base/head `engine/Cargo.lock`의 픽셀 결정 크레이트 버전 비교, 01a-4 Task 12a `crate_changes`). 이 분할의 CI는 결정성 lint·fmt·clippy·test만 돈다.

**완료 인수 조건:** 저장소 루트에서
```bash
bash scripts/lint-determinism.sh
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path engine/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path engine/Cargo.toml --workspace
cargo build --locked --manifest-path engine/Cargo.toml -p engine-core --target wasm32-unknown-unknown
git diff --exit-code -- engine/Cargo.lock
```
모두 성공, PR의 CI `Engine · ubuntu/windows/macos`와 `Web` 녹색. 참고(검증 실행): 이 분할 끝의 `engine-core` 단위 테스트는 67개.

**Review Focus (이 분할):** wasm32 크기 오버플로(Task 2 `rejects_u32_max_square_without_overflow`), 다른 인코더의 JPEG·CMYK·16bit PNG(Task 4), 무릎점 연속성과 음수 부호 대칭(Task 1).

---

## Task 0: 브랜치, 툴체인, 워크스페이스 스캐폴드, 결정성 린트, CI 기본 잡

**Files:**
- Create: `rust-toolchain.toml`, `engine/Cargo.toml`, `engine/rustfmt.toml`, `engine/{core,cli,wasm}/Cargo.toml`, `engine/core/src/lib.rs`, 빈 모듈 파일들, `engine/cli/src/main.rs`, `engine/wasm/src/lib.rs`, `scripts/lint-determinism.sh`
- Modify: `.gitignore`, `.github/workflows/ci.yml`(engine 잡 교체)

**Interfaces:**
- Produces: 크레이트 `engine-core`(모듈 `buffer, color, decode, hash, recipe, region, render, resample, score`), 상수 `ENGINE_VERSION="engine-0.1.0"`, `SCORING_VERSION="scoring-0.1.0"`, `SCHEMA_VERSION=1`, `SCORING_LONG_EDGE=1024`.

- [ ] **Step 1: 기능 브랜치 생성**

```bash
git switch -c feat/engine-1a-1-numeric-image
```

- [ ] **Step 2: 툴체인·워크스페이스 파일 작성**

`rust-toolchain.toml`:
```toml
# Pinned so clippy/rustfmt/codegen are identical locally and in CI (master plan §5.4).
[toolchain]
channel = "1.98.1"
components = ["clippy", "rustfmt"]
targets = ["wasm32-unknown-unknown"]
profile = "minimal"
```

`engine/Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["core", "cli", "wasm"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
rust-version = "1.87"
repository = "https://github.com/gwanryo/retouch"

[workspace.dependencies]
# Crates that decide pixel values are pinned exactly (master plan §5.3); Cargo.lock pins the rest.
engine-core = { path = "core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# `arch` swaps in hardware sqrt/fma/rint. Those are IEEE-exact too, but we keep every target on
# the same pure-Rust code path (master plan §5.1).
libm = { version = "=0.2.16", default-features = false, features = ["force-soft-floats"] }
sha2 = "0.10"
thiserror = "2"
# Scalar decoder only: the default `x86`/`neon` features enable SIMD IDCT/upsampling/color
# conversion, which wasm32 does not have (master plan A2).
zune-jpeg = { version = "=0.5.15", default-features = false, features = ["std"] }
zune-core = { version = "0.5", default-features = false, features = ["std"] }
png = "=0.18.1"
jpeg-encoder = { version = "=0.7.1", default-features = false, features = ["std"] }
clap = { version = "4", default-features = false, features = ["std", "derive", "help", "usage", "error-context"] }
anyhow = "1"
wasm-bindgen = "0.2"
wasm-bindgen-test = "0.3"

[workspace.lints.rust]
missing_docs = "warn"
unsafe_code = "forbid"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
# Pixel code converts between u8/u32/usize/f32/f64 constantly; every cast site is bounded by
# BufferError/MAX_PIXELS checks, so these pedantic cast lints are noise here.
cast_possible_truncation = "allow"
cast_precision_loss = "allow"
cast_sign_loss = "allow"
cast_possible_wrap = "allow"
# Math code uses short conventional names (r, g, b, l, a, x, y).
many_single_char_names = "allow"
similar_names = "allow"
# Public fns in this engine are all pure; marking each #[must_use] adds noise, not safety.
must_use_candidate = "allow"
# Error variants are documented on the error enums themselves.
missing_errors_doc = "allow"
# Determinism tests compare exact bits on purpose, and the engine's own logic compares clamped
# values to detect changes; an epsilon would hide exactly the drift we are guarding against.
float_cmp = "allow"
# Formulas must read exactly as written so the GLSL (Plan 2) and JS ports match term by term;
# `f64::midpoint` is a different expression.
manual_midpoint = "allow"

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
zune-jpeg.workspace = true

png.workspace = true

[dev-dependencies]
jpeg-encoder.workspace = true

[lints]
workspace = true
```

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
serde.workspace = true
serde_json.workspace = true
jpeg-encoder.workspace = true

[lints]
workspace = true
```

`engine/wasm/Cargo.toml` (wasm-bindgen 0.2.129의 `#[wasm_bindgen]` 확장이 `unsafe_code = "forbid"`를 통과함을 확인했으므로 워크스페이스 lint 표를 공유한다):
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

[dev-dependencies]
wasm-bindgen-test.workspace = true

# Verified with wasm-bindgen 0.2.129: #[wasm_bindgen] expansions pass `unsafe_code = "forbid"`,
# so the wasm crate shares the workspace lint table.
[lints]
workspace = true
```

`engine/core/src/lib.rs` (`scoring_image` 재노출은 Task 3에서 추가):
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
pub mod region;
pub mod render;
pub mod resample;
pub mod score;

/// Render pipeline version. Bump whenever any render math changes (golden hashes change).
pub const ENGINE_VERSION: &str = "engine-0.1.0";
/// Scoring version. Bump whenever score math, [`score::T_TILE`] or the scoring-image path changes.
pub const SCORING_VERSION: &str = "scoring-0.1.0";
/// Recipe JSON `schema_version` accepted by this build.
pub const SCHEMA_VERSION: u32 = 1;
/// Long edge of the scoring image (AC-S1h).
pub const SCORING_LONG_EDGE: u32 = 1024;
```

빈 모듈 파일(각 한 줄 문서 주석). 저장소 루트에서:

```bash
mkdir -p engine/core/src/recipe engine/core/src/render engine/core/src/score engine/cli/src engine/wasm/src scripts
for m in buffer color decode hash region resample; do printf '//! %s (Plan 1a)\n' "$m" > "engine/core/src/$m.rs"; done
for m in recipe render score; do printf '//! %s (Plan 1a)\n' "$m" > "engine/core/src/$m/mod.rs"; done
printf '//! `engine-cli` (filled in by Tasks 11a, 11b, 12a).\n\nfn main() {}\n' > engine/cli/src/main.rs
printf '//! wasm-bindgen surface (filled in by Task 10).\n' > engine/wasm/src/lib.rs
```

- [ ] **Step 3: 결정성 린트 스크립트**

`scripts/lint-determinism.sh`:
```bash
#!/usr/bin/env bash
# Determinism lint (master plan §5). Engine code must not call std float transcendentals or FMA:
# they lower to platform libm / hardware paths that differ between targets. All such math goes
# through the pure-Rust `libm` crate with `force-soft-floats`, which (libm 0.2.16 configure.rs)
# disables both the `arch` cfg (hardware sqrt/fma/rint) and the `intrinsics` cfg even when some
# other crate turns the `arch` feature on. libm's own internals are allowed; our code may not
# call `libm::fma*` or `mul_add` explicitly. Exact IEEE ops (sqrt, abs, floor, ...) are allowed.
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0
src=()
for d in engine/core/src engine/core/tests engine/cli/src engine/cli/tests engine/wasm/src engine/wasm/tests; do
  [ -d "$d" ] && src+=("$d")
done

fns='powf|powi|exp|exp2|exp_m1|ln|ln_1p|log|log2|log10|sin|cos|tan|asin|acos|atan|atan2|sinh|cosh|tanh|asinh|acosh|atanh|cbrt|hypot|mul_add|sin_cos|to_degrees|to_radians'
# method calls, tolerating whitespace: `x.powf(`, `x . powf (`
if grep -rnE --include='*.rs' "\.[[:space:]]*($fns)[[:space:]]*\(" "${src[@]}"; then
  echo "determinism lint: std float method found; use libm::* instead" >&2; fail=1
fi
# UFCS: `f32::powf(`, `f64 :: exp (`
if grep -rnE --include='*.rs' "\bf(32|64)[[:space:]]*::[[:space:]]*($fns)[[:space:]]*\(" "${src[@]}"; then
  echo "determinism lint: std float function (UFCS) found; use libm::* instead" >&2; fail=1
fi
if grep -rnE --include='*.rs' 'libm[[:space:]]*::[[:space:]]*fmaf?\b|\bfmaf?[[:space:]]*\(' "${src[@]}"; then
  echo "determinism lint: explicit FMA is forbidden" >&2; fail=1
fi

# No target-cpu / target-feature / fast-math in any cargo config, nor in the environment.
if find . -path ./engine/target -prune -o -path ./web/node_modules -prune -o \
     \( -path '*/.cargo/config' -o -path '*/.cargo/config.toml' \) -print | grep -q .; then
  echo "determinism lint: .cargo/config(.toml) is not allowed in this repo" >&2; fail=1
fi
for v in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS; do
  if [ -n "${!v:-}" ]; then
    echo "determinism lint: $v must be empty (is '${!v}')" >&2; fail=1
  fi
done

# Dependency features of the shipped (normal-edge) graph. Dev-dependencies such as
# wasm-bindgen-test -> num-traits turn libm's `arch` feature on in test builds; that is inert
# because `force-soft-floats` overrides it, which is why force-soft-floats is required below.
tree_jpeg=$(cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e features,normal -i zune-jpeg)
if grep -qE 'zune-jpeg feature "(x86|neon|portable_simd|default)"' <<<"$tree_jpeg"; then
  echo "$tree_jpeg" >&2
  echo "determinism lint: zune-jpeg SIMD/default feature is enabled" >&2; fail=1
fi
tree_libm=$(cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e features,normal -i libm)
if grep -qE 'libm feature "(arch|default)"' <<<"$tree_libm"; then
  echo "$tree_libm" >&2
  echo "determinism lint: libm arch/default feature is enabled" >&2; fail=1
fi
if ! grep -q 'libm feature "force-soft-floats"' <<<"$tree_libm"; then
  echo "determinism lint: libm force-soft-floats is not enabled" >&2; fail=1
fi
if cargo tree --locked --manifest-path engine/Cargo.toml --workspace -e all -i image >/dev/null 2>&1; then
  echo "determinism lint: the image crate must not be a dependency (zune-jpeg + png only)" >&2; fail=1
fi

[ "$fail" -eq 0 ] && echo "determinism lint: ok"
exit "$fail"
```

- [ ] **Step 4: `.gitignore`와 CI**

`.gitignore`에서 `content/answers/**/answer_2048.png`, `content/answers/**/answer_1024.png` 두 줄과 그 위 주석을 지운다(마스터 A8: 정답은 `content/`에 커밋). `# Rust` 블록 끝에 추가:

```gitignore
engine/golden/actual-*.json
engine/golden/expected.json.tmp
```

`.github/workflows/ci.yml`의 `engine:` 잡 전체를 아래로 교체한다(`detect`·`web` 잡은 그대로). `# >>> … steps` 주석 두 줄은 분할 3·4에서 단계를 끼워 넣을 자리 표시로 남긴다.

```yaml
  engine:
    name: Engine · ${{ matrix.os }}
    needs: detect
    if: needs.detect.outputs.engine == 'true'
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    env:
      CARGO_TERM_COLOR: always
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0 # 01a-4: the golden guard reads expected.json and Cargo.lock at the base commit, verify-sources reads the generator commit
      - name: Install pinned toolchain (matches rust-toolchain.toml)
        run: rustup toolchain install 1.98.1 --profile minimal --component clippy,rustfmt --target wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: engine
      - name: Determinism lint
        shell: bash
        run: bash scripts/lint-determinism.sh
      - name: Format
        run: cargo fmt --manifest-path engine/Cargo.toml --all -- --check
      - name: Clippy (native)
        run: cargo clippy --locked --manifest-path engine/Cargo.toml --workspace --all-targets -- -D warnings
      - name: Test (native, debug)
        run: cargo test --locked --manifest-path engine/Cargo.toml --workspace
      # >>> wasm steps (Plan 1a-3 Task 10)
      # >>> golden steps (Plan 1a-4 Task 14)
      - name: Cargo.lock unchanged
        shell: bash
        run: git diff --exit-code -- engine/Cargo.lock
```

- [ ] **Step 5: 스캐폴드 검증**

```bash
rustup toolchain install
cargo generate-lockfile --manifest-path engine/Cargo.toml
cargo update --manifest-path engine/Cargo.toml -p zune-core --precise 0.5.3
cargo clippy --locked --manifest-path engine/Cargo.toml --workspace --all-targets -- -D warnings
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
bash scripts/lint-determinism.sh
```
`generate-lockfile`은 이 계획에서 `--locked` 없이 실행하는 유일한 cargo 명령이다(잠금 파일 생성). 픽셀을 결정하는 크레이트는 `engine/Cargo.toml`에서 `=` 버전으로 고정했고, 그 전이 의존성 `zune-core`는 검증에 쓴 0.5.3으로 맞춘다. Expected: clippy `Finished`(경고 0), fmt 출력 없음, `determinism lint: ok`.

- [ ] **Step 6: RED - 린트가 위반을 잡는지 확인**

```bash
printf 'fn f(x: f32) -> f32 { x . powf (2.0) + f64::exp(1.0) as f32 + libm::fmaf(1.0, 2.0, 3.0) }\n' > engine/core/src/zz_bad.rs
bash scripts/lint-determinism.sh; echo "exit=$?"
rm engine/core/src/zz_bad.rs
RUSTFLAGS="-C target-cpu=native" bash scripts/lint-determinism.sh; echo "exit=$?"
```
Expected: 첫 실행은 `std float method`, `UFCS`, `explicit FMA` 세 메시지와 `exit=1`. 둘째는 `RUSTFLAGS must be empty`와 `exit=1`(확인함).

- [ ] **Step 7: 커밋**

```bash
git add rust-toolchain.toml engine/Cargo.toml engine/Cargo.lock engine/rustfmt.toml engine/core engine/cli engine/wasm scripts/lint-determinism.sh .gitignore .github/workflows/ci.yml
git commit -m "feat(engine): 워크스페이스 스캐폴드, 툴체인 고정, 결정성 린트, CI 기본 잡"
```

---

## Task 1: 색 과학 (확장 sRGB, Lab D50, CIEDE2000)

**Files:**
- Create: `engine/core/testdata/sharma2005.csv`
- Modify: `engine/core/src/color.rs`

**Interfaces:**
- Produces: `srgb_decode(f32)->f32`, `srgb_encode(f32)->f32`(비클램프, 부호 대칭), `struct Lab{l,a,b: f64}`, `linear_srgb_to_lab_d50(r,g,b: f32)->Lab`, `delta_e00(Lab,Lab)->f64`, `SHARMA_2005_CSV`, `sharma_2005_rows()`(테스트 지원, `#[doc(hidden)]`).

- [ ] **Step 1: Sharma 데이터 파일** — 표 전사는 Codex가 독립 Python·JS f64 구현으로 대조해 양방향 최대 오차 4.95e-5를 확인했다.

`engine/core/testdata/sharma2005.csv`:
```csv
# Sharma, Wu, Dalal (2005), "The CIEDE2000 color-difference formula: Implementation notes,
# supplementary test data, and mathematical observations", Table 1. Columns: L1,a1,b1,L2,a2,b2,dE00
50.0000,2.6772,-79.7751,50.0000,0.0000,-82.7485,2.0425
50.0000,3.1571,-77.2803,50.0000,0.0000,-82.7485,2.8615
50.0000,2.8361,-74.0200,50.0000,0.0000,-82.7485,3.4412
50.0000,-1.3802,-84.2814,50.0000,0.0000,-82.7485,1.0000
50.0000,-1.1848,-84.8006,50.0000,0.0000,-82.7485,1.0000
50.0000,-0.9009,-85.5211,50.0000,0.0000,-82.7485,1.0000
50.0000,0.0000,0.0000,50.0000,-1.0000,2.0000,2.3669
50.0000,-1.0000,2.0000,50.0000,0.0000,0.0000,2.3669
50.0000,2.4900,-0.0010,50.0000,-2.4900,0.0009,7.1792
50.0000,2.4900,-0.0010,50.0000,-2.4900,0.0010,7.1792
50.0000,2.4900,-0.0010,50.0000,-2.4900,0.0011,7.2195
50.0000,2.4900,-0.0010,50.0000,-2.4900,0.0012,7.2195
50.0000,-0.0010,2.4900,50.0000,0.0009,-2.4900,4.8045
50.0000,-0.0010,2.4900,50.0000,0.0010,-2.4900,4.8045
50.0000,-0.0010,2.4900,50.0000,0.0011,-2.4900,4.7461
50.0000,2.5000,0.0000,50.0000,0.0000,-2.5000,4.3065
50.0000,2.5000,0.0000,73.0000,25.0000,-18.0000,27.1492
50.0000,2.5000,0.0000,61.0000,-5.0000,29.0000,22.8977
50.0000,2.5000,0.0000,56.0000,-27.0000,-3.0000,31.9030
50.0000,2.5000,0.0000,58.0000,24.0000,15.0000,19.4535
50.0000,2.5000,0.0000,50.0000,3.1736,0.5854,1.0000
50.0000,2.5000,0.0000,50.0000,3.2972,0.0000,1.0000
50.0000,2.5000,0.0000,50.0000,1.8634,0.5757,1.0000
50.0000,2.5000,0.0000,50.0000,3.2592,0.3350,1.0000
60.2574,-34.0099,36.2677,60.4626,-34.1751,39.4387,1.2644
63.0109,-31.0961,-5.8663,62.8187,-29.7946,-4.0864,1.2630
61.2901,3.7196,-5.3901,61.4292,2.2480,-4.9620,1.8731
35.0831,-44.1164,3.7933,35.0232,-40.0716,1.5901,1.8645
22.7233,20.0904,-46.6940,23.0331,14.9730,-42.5619,2.0373
36.4612,47.8580,18.3852,36.2715,50.5065,21.2231,1.4146
90.8027,-2.0831,1.4410,91.1528,-1.6435,0.0447,1.4441
90.9257,-0.5406,-0.9208,88.6381,-0.8985,-0.7239,1.5381
6.7747,-0.2908,-2.4247,5.8714,-0.0985,-2.2286,0.6377
2.0776,0.0795,-1.1350,0.9033,-0.0636,-0.5514,0.9082
```

- [ ] **Step 2: 실패하는 테스트 작성** — `color.rs`를 Step 4 구현으로 만들되 모든 `fn` 본문을 `todo!()`로 두고(struct·const는 그대로) 아래 테스트 모듈을 붙인다. Lab 기대값은 위 상수와 무관한 독립 유도(sRGB 원색 xy + D65 xy로 RGB→XYZ, Bradford 원뿔 행렬로 D65→D50, f64)에서 나왔다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lab8(r: u8, g: u8, b: u8) -> Lab {
        let d = |v: u8| srgb_decode(f32::from(v) / 255.0);
        linear_srgb_to_lab_d50(d(r), d(g), d(b))
    }

    fn assert_lab_close(got: Lab, l: f64, a: f64, b: f64) {
        let ok = (got.l - l).abs() < 0.05 && (got.a - a).abs() < 0.05 && (got.b - b).abs() < 0.05;
        assert!(ok, "got {got:?}, expected ({l}, {a}, {b})");
    }

    mod srgb_decode {
        use super::*;

        #[test]
        fn maps_zero_to_zero() {
            assert_eq!(srgb_decode(0.0), 0.0);
        }

        #[test]
        fn maps_mid_gray_128_to_known_linear_value() {
            let lin = srgb_decode(128.0 / 255.0);
            assert!((lin - 0.215_86).abs() < 1e-4, "got {lin}");
        }

        #[test]
        fn is_continuous_at_the_knee() {
            let below = srgb_decode(0.040_45);
            let above = srgb_decode(0.040_450_1);
            assert!((above - below).abs() < 1e-6, "{below} {above}");
        }

        #[test]
        fn extends_above_one_without_clamping() {
            assert!(srgb_decode(1.2) > 1.0);
        }

        #[test]
        fn is_sign_symmetric_below_zero() {
            for v in [0.01f32, 0.04, 0.2, 0.5, 1.3] {
                assert_eq!(srgb_decode(-v), -srgb_decode(v), "v={v}");
            }
        }
    }

    mod srgb_encode {
        use super::*;

        #[test]
        fn is_inverse_of_decode_on_every_8bit_level() {
            for i in 0..=255u8 {
                let v = f32::from(i) / 255.0;
                let back = srgb_encode(srgb_decode(v));
                assert!((back - v).abs() < 2e-6, "i={i} v={v} back={back}");
            }
        }

        #[test]
        fn is_continuous_at_the_knee() {
            let below = srgb_encode(0.003_130_8);
            let above = srgb_encode(0.003_130_81);
            assert!((above - below).abs() < 1e-6, "{below} {above}");
        }

        #[test]
        fn extends_above_one_without_clamping() {
            assert!(srgb_encode(2.0) > 1.0);
        }

        #[test]
        fn is_sign_symmetric_below_zero() {
            for v in [0.001f32, 0.003, 0.05, 0.5, 2.0] {
                assert_eq!(srgb_encode(-v), -srgb_encode(v), "v={v}");
            }
        }
    }

    // Expected values come from an independent float64 derivation (sRGB primaries + D65 xy →
    // RGB→XYZ; Bradford cone matrix → D65→D50), not from the constants above. Tolerance 0.05
    // covers the white-point rounding difference between the two.
    mod linear_srgb_to_lab_d50 {
        use super::*;

        #[test]
        fn white_is_l100_neutral() {
            assert_lab_close(lab8(255, 255, 255), 100.0, 0.0, 0.0);
        }

        #[test]
        fn black_is_l0() {
            assert_lab_close(lab8(0, 0, 0), 0.0, 0.0, 0.0);
        }

        #[test]
        fn mid_gray_128() {
            assert_lab_close(lab8(128, 128, 128), 53.585, 0.0, 0.0);
        }

        #[test]
        fn pure_red() {
            assert_lab_close(lab8(255, 0, 0), 54.289, 80.811, 69.888);
        }

        #[test]
        fn pure_green() {
            assert_lab_close(lab8(0, 255, 0), 87.819, -79.280, 80.996);
        }

        #[test]
        fn pure_blue() {
            assert_lab_close(lab8(0, 0, 255), 29.569, 68.297, -112.028);
        }

        #[test]
        fn warm_tone_200_150_100() {
            assert_lab_close(lab8(200, 150, 100), 66.126, 14.996, 33.950);
        }

        #[test]
        fn dark_blue_gray_10_20_30() {
            assert_lab_close(lab8(10, 20, 30), 5.851, -1.496, -8.255);
        }
    }

    mod delta_e00 {
        use super::*;

        #[test]
        fn data_file_has_34_rows() {
            assert_eq!(sharma_2005_rows().len(), 34);
        }

        #[test]
        fn matches_all_sharma_vectors_within_1e_4() {
            for (i, (c1, c2, want)) in sharma_2005_rows().into_iter().enumerate() {
                let got = delta_e00(c1, c2);
                assert!(
                    (got - want).abs() <= 1e-4,
                    "pair {}: want {want} got {got}",
                    i + 1
                );
            }
        }

        #[test]
        fn matches_all_sharma_vectors_with_arguments_swapped() {
            for (i, (c1, c2, want)) in sharma_2005_rows().into_iter().enumerate() {
                let got = delta_e00(c2, c1);
                assert!(
                    (got - want).abs() <= 1e-4,
                    "pair {} swapped: want {want} got {got}",
                    i + 1
                );
            }
        }

        #[test]
        fn identical_colors_have_zero_difference() {
            let c = Lab {
                l: 40.0,
                a: -12.0,
                b: 30.0,
            };
            assert_eq!(delta_e00(c, c), 0.0);
        }

        #[test]
        fn achromatic_pair_reduces_to_the_lightness_term() {
            // Both chroma 0: hue terms vanish, dE = |dL| / SL with L-bar = 55.
            let c1 = Lab {
                l: 50.0,
                a: 0.0,
                b: 0.0,
            };
            let c2 = Lab {
                l: 60.0,
                a: 0.0,
                b: 0.0,
            };
            let sl = 1.0 + 0.015 * 25.0 / libm::sqrt(45.0);
            assert!((delta_e00(c1, c2) - 10.0 / sl).abs() < 1e-12);
        }
    }
}
```

- [ ] **Step 3: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core color::` / Expected: FAIL(`not yet implemented`).

- [ ] **Step 4: 구현** — `color.rs` 테스트 모듈 위 전체:

```rust
//! Color science. All transcendental math goes through `libm` so native and wasm agree
//! bit-for-bit (master plan §5.1).

/// sRGB EOTF, encoded → linear. **Not clamped** (master plan A3). Extended sign-symmetrically
/// as in W3C CSS Color 4: `decode(-v) = -decode(v)`; values above 1 continue the power curve.
pub fn srgb_decode(v: f32) -> f32 {
    let a = v.abs();
    let lin = if a <= 0.040_45 {
        a / 12.92
    } else {
        libm::powf((a + 0.055) / 1.055, 2.4)
    };
    if v < 0.0 {
        -lin
    } else {
        lin
    }
}

/// sRGB OETF, linear → encoded. **Not clamped**, sign-symmetric like [`srgb_decode`].
pub fn srgb_encode(v: f32) -> f32 {
    let a = v.abs();
    let enc = if a <= 0.003_130_8 {
        a * 12.92
    } else {
        1.055 * libm::powf(a, 1.0 / 2.4) - 0.055
    };
    if v < 0.0 {
        -enc
    } else {
        enc
    }
}

/// CIE L*a*b* under the D50 illuminant (ICC PCS). Used for CIEDE2000.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    /// Lightness, 0..100.
    pub l: f64,
    /// Green-red axis.
    pub a: f64,
    /// Blue-yellow axis.
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
    let xyz65 = mul3(
        &M_RGB_TO_XYZ_D65,
        [f64::from(r), f64::from(g), f64::from(b)],
    );
    let xyz50 = mul3(&M_BRADFORD_D65_TO_D50, xyz65);
    let fx = lab_f(xyz50[0] / D50_WHITE[0]);
    let fy = lab_f(xyz50[1] / D50_WHITE[1]);
    let fz = lab_f(xyz50[2] / D50_WHITE[2]);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// CIEDE2000 color difference with `kL = kC = kH = 1` (AC-S1f). f64 + `libm` throughout.
pub fn delta_e00(c1: Lab, c2: Lab) -> f64 {
    use libm::{atan2, cos, exp, hypot, pow, sin, sqrt};
    use std::f64::consts::PI;
    const TWO_PI: f64 = 2.0 * PI;
    const POW25_7: f64 = 6_103_515_625.0; // 25^7

    let hue = |a: f64, b: f64| -> f64 {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            let h = atan2(b, a);
            if h < 0.0 {
                h + TWO_PI
            } else {
                h
            }
        }
    };

    let cbar = (hypot(c1.a, c1.b) + hypot(c2.a, c2.b)) / 2.0;
    let cbar7 = pow(cbar, 7.0);
    let g = 0.5 * (1.0 - sqrt(cbar7 / (cbar7 + POW25_7)));

    let a1p = (1.0 + g) * c1.a;
    let a2p = (1.0 + g) * c2.a;
    let c1p = hypot(a1p, c1.b);
    let c2p = hypot(a2p, c2.b);
    let h1p = hue(a1p, c1.b);
    let h2p = hue(a2p, c2.b);

    let dlp = c2.l - c1.l;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d.abs() <= PI {
            d
        } else if d > PI {
            d - TWO_PI
        } else {
            d + TWO_PI
        }
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

/// Sharma 2005 vectors, shared by the native unit tests and the wasm test suite.
#[doc(hidden)]
pub const SHARMA_2005_CSV: &str = include_str!("../testdata/sharma2005.csv");

/// Parses [`SHARMA_2005_CSV`] into `(c1, c2, expected_de00)` rows (test support).
#[doc(hidden)]
pub fn sharma_2005_rows() -> Vec<(Lab, Lab, f64)> {
    SHARMA_2005_CSV
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let v: Vec<f64> = l.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 7).then(|| {
                let c1 = Lab {
                    l: v[0],
                    a: v[1],
                    b: v[2],
                };
                let c2 = Lab {
                    l: v[3],
                    a: v[4],
                    b: v[5],
                };
                (c1, c2, v[6])
            })
        })
        .collect()
}
```

- [ ] **Step 5: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core color::` / Expected: 모두 PASS.

- [ ] **Step 6: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/testdata engine/core/src/color.rs
git commit -m "feat(engine): 색 과학 (부호 대칭 확장 sRGB, Lab D50 Bradford, CIEDE2000 Sharma 양방향)"
```

---

## Task 2: 픽셀 버퍼와 단일 양자화

**Files:**
- Modify: `engine/core/src/buffer.rs`

**Interfaces:**
- Produces: `MAX_PIXELS: u64 = 50_000_000`, `enum BufferError{Empty, TooLarge, LengthMismatch}`, `checked_len(w,h,channels)->Result<usize,_>`, `quantize(f32)->u8`, `dequantize(u8)->f32`, `struct Rgba8`(`new(w,h,Vec<u8>)`, `width()`, `height()`, `data()`, `into_data()`), `struct ImageF32`(`new_filled`, `from_samples`, `from_rgba8(&Rgba8)`, `to_rgba8()->Rgba8`, `width`, `height`, `samples`, `pixel`, `set_pixel`, `map_pixels`, `pixels`).

- [ ] **Step 1: 실패하는 테스트 작성** — Step 3 구현을 `fn` 본문 `todo!()`로 두되, **`pixels()`만은 본문을 `std::iter::empty()`로** 둔다(`impl Iterator` 반환에 `todo!()`를 쓰면 E0277로 컴파일되지 않는다, 공통 계약 "TDD 규칙과 예외"). 아래 테스트를 붙인다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod quantize {
        use super::*;

        #[test]
        fn rounds_half_up() {
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
        fn maps_nan_to_zero() {
            assert_eq!(quantize(f32::NAN), 0);
        }

        #[test]
        fn round_trips_every_byte() {
            for v in 0..=255u8 {
                assert_eq!(quantize(dequantize(v)), v, "byte {v}");
            }
        }
    }

    mod rgba8_new {
        use super::*;

        #[test]
        fn rejects_length_mismatch() {
            let err = Rgba8::new(2, 1, vec![0; 7]).unwrap_err();
            let want = BufferError::LengthMismatch {
                width: 2,
                height: 1,
                expected: 8,
                actual: 7,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_zero_width() {
            let err = Rgba8::new(0, 5, Vec::new()).unwrap_err();
            assert_eq!(
                err,
                BufferError::Empty {
                    width: 0,
                    height: 5
                }
            );
        }

        #[test]
        fn rejects_more_than_max_pixels_before_checking_length() {
            let err = Rgba8::new(10_000, 10_000, Vec::new()).unwrap_err();
            let want = BufferError::TooLarge {
                width: 10_000,
                height: 10_000,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_u32_max_square_without_overflow() {
            let err = Rgba8::new(u32::MAX, u32::MAX, Vec::new()).unwrap_err();
            assert!(matches!(err, BufferError::TooLarge { .. }), "{err:?}");
        }
    }

    mod image_f32 {
        use super::*;

        #[test]
        fn from_rgba8_drops_alpha_and_dequantizes() {
            let src = Rgba8::new(1, 1, vec![255, 0, 128, 7]).unwrap();
            assert_eq!(
                ImageF32::from_rgba8(&src).pixel(0, 0),
                [1.0, 0.0, 128.0 / 255.0]
            );
        }

        #[test]
        fn round_trips_rgba8_exactly() {
            let bytes: Vec<u8> = (0..64u8)
                .flat_map(|i| [i, 255 - i, i.wrapping_mul(3), 255])
                .collect();
            let src = Rgba8::new(8, 8, bytes).unwrap();
            assert_eq!(ImageF32::from_rgba8(&src).to_rgba8(), src);
        }

        #[test]
        fn new_filled_rejects_empty() {
            let err = ImageF32::new_filled(3, 0, [0.0; 3]).unwrap_err();
            assert_eq!(
                err,
                BufferError::Empty {
                    width: 3,
                    height: 0
                }
            );
        }

        #[test]
        fn new_filled_sets_every_pixel() {
            let img = ImageF32::new_filled(3, 2, [0.1, 0.2, 0.3]).unwrap();
            assert!(img.pixels().all(|p| p == [0.1, 0.2, 0.3]));
        }
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core buffer::` / Expected: 컴파일 성공, 테스트 FAIL(`not yet implemented`).

- [ ] **Step 3: 구현**

```rust
//! Pixel buffers and THE single 8-bit quantization rule (master plan A3, AC-E1e).

use thiserror::Error;

/// Largest accepted image, in pixels (≈ 8660×5773). Bounds every allocation and keeps
/// `width * height * 12` far below `usize::MAX` on wasm32.
pub const MAX_PIXELS: u64 = 50_000_000;

/// Errors from buffer construction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BufferError {
    /// Width or height is zero.
    #[error("empty image: {width}x{height}")]
    Empty {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
    },
    /// More than [`MAX_PIXELS`] pixels.
    #[error("image too large: {width}x{height} exceeds {MAX_PIXELS} pixels")]
    TooLarge {
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
    },
    /// Byte length does not match `width * height * 4`.
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

/// Validates dimensions and returns `width * height * channels` without overflow.
pub fn checked_len(width: u32, height: u32, channels: usize) -> Result<usize, BufferError> {
    if width == 0 || height == 0 {
        return Err(BufferError::Empty { width, height });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_PIXELS {
        return Err(BufferError::TooLarge { width, height });
    }
    usize::try_from(pixels)
        .ok()
        .and_then(|p| p.checked_mul(channels))
        .ok_or(BufferError::TooLarge { width, height })
}

/// Encoded [0,1] → 8-bit: clamp, scale, round half up. The ONLY quantization in the engine.
/// NaN maps to 0; render never produces NaN (it returns `RenderError::NonFinite` instead).
pub fn quantize(v: f32) -> u8 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= 1.0 {
        255
    } else {
        // `as u8` truncates toward zero; +0.5 first gives round-half-up for non-negative input.
        (v * 255.0 + 0.5) as u8
    }
}

/// 8-bit → encoded [0,1].
pub fn dequantize(v: u8) -> f32 {
    f32::from(v) / 255.0
}

/// Validated RGBA8 pixels (alpha carried but ignored by render/score).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Rgba8 {
    /// Wraps `data` after checking dimensions and length.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Self, BufferError> {
        let expected = checked_len(width, height, 4)?;
        if data.len() != expected {
            return Err(BufferError::LengthMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Interleaved RGBA bytes, row-major.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consumes the buffer and returns the bytes.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

/// RGB image, interleaved `f32`, row-major. Whether samples are encoded sRGB or linear is a
/// convention of the call site. Values are NOT clamped (master plan A3).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageF32 {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

impl ImageF32 {
    /// Image filled with one pixel value.
    pub fn new_filled(width: u32, height: u32, px: [f32; 3]) -> Result<Self, BufferError> {
        let n = checked_len(width, height, 3)?;
        let data = px.iter().copied().cycle().take(n).collect();
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Wraps interleaved RGB samples after checking dimensions and length.
    pub fn from_samples(width: u32, height: u32, data: Vec<f32>) -> Result<Self, BufferError> {
        let expected = checked_len(width, height, 3)?;
        if data.len() != expected {
            return Err(BufferError::LengthMismatch {
                width,
                height,
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// From RGBA8 (alpha dropped), samples dequantized to encoded sRGB.
    pub fn from_rgba8(src: &Rgba8) -> Self {
        let data = src
            .data()
            .chunks_exact(4)
            .flat_map(|px| [dequantize(px[0]), dequantize(px[1]), dequantize(px[2])])
            .collect();
        Self {
            width: src.width(),
            height: src.height(),
            data,
        }
    }

    /// To RGBA8 with alpha 255, via [`quantize`].
    pub fn to_rgba8(&self) -> Rgba8 {
        let data = self
            .data
            .chunks_exact(3)
            .flat_map(|px| [quantize(px[0]), quantize(px[1]), quantize(px[2]), 255])
            .collect();
        Rgba8 {
            width: self.width,
            height: self.height,
            data,
        }
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

    /// Pixel at (x, y). Panics if out of bounds (callers iterate within `width`/`height`).
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        [self.data[i], self.data[i + 1], self.data[i + 2]]
    }

    /// Sets the pixel at (x, y). Panics if out of bounds.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: [f32; 3]) {
        let i = (y as usize * self.width as usize + x as usize) * 3;
        self.data[i..i + 3].copy_from_slice(&px);
    }

    /// New image with `f` applied to every pixel, in row-major order.
    #[must_use]
    pub fn map_pixels(&self, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        let data = self
            .data
            .chunks_exact(3)
            .flat_map(|px| f([px[0], px[1], px[2]]))
            .collect();
        Self {
            width: self.width,
            height: self.height,
            data,
        }
    }

    /// Pixels in row-major order.
    pub fn pixels(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.data.chunks_exact(3).map(|px| [px[0], px[1], px[2]])
    }
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core buffer::` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/buffer.rs
git commit -m "feat(engine): Rgba8·ImageF32 버퍼, 크기 상한, 단일 양자화 규칙"
```

---

## Task 3: 채점 이미지 축소 (Lanczos3, `scoring_image`)

**Files:**
- Modify: `engine/core/src/resample.rs`, `engine/core/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 `srgb_decode/srgb_encode`, Task 2 `Rgba8`, `ImageF32::from_samples`, `quantize`, `dequantize`.
- Produces: `fit_long_edge(w,h,long)->(u32,u32)`, `downscale_lanczos3(&ImageF32,w,h)->Result<ImageF32,ResampleError>`, `scoring_image(&Rgba8, long_edge)->Result<Rgba8,ResampleError>`(crate 루트 재노출), `enum ResampleError{EmptyTarget, Upscale, Buffer}`.

- [ ] **Step 1: 실패하는 테스트 작성** — Step 3 구현을 `fn` 본문 `todo!()`로 두고 아래 테스트를 붙인다. 기대값은 모듈 문서 규칙을 float64로 독립 구현한 결과다. `engine/core/src/lib.rs`의 `pub mod score;` 아래에 `pub use resample::scoring_image;`를 추가한다.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[f32]) -> ImageF32 {
        let data = values.iter().flat_map(|&v| [v, v, v]).collect();
        ImageF32::from_samples(values.len() as u32, 1, data).unwrap()
    }

    fn assert_row_close(img: &ImageF32, want: &[f32]) {
        let got: Vec<f32> = img.pixels().map(|p| p[0]).collect();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "got {got:?} want {want:?}");
        }
    }

    mod fit_long_edge {
        use super::*;

        #[test]
        fn scales_landscape_2048x1365_to_1024x683() {
            assert_eq!(fit_long_edge(2048, 1365, 1024), (1024, 683));
        }

        #[test]
        fn scales_portrait_1530x2040_to_768x1024() {
            assert_eq!(fit_long_edge(1530, 2040, 1024), (768, 1024));
        }

        #[test]
        fn never_upscales() {
            assert_eq!(fit_long_edge(800, 600, 1024), (800, 600));
        }

        #[test]
        fn keeps_a_one_pixel_short_edge() {
            assert_eq!(fit_long_edge(3000, 1, 1024), (1024, 1));
        }
    }

    // Expected values: independent float64 reference implementation of the rules in the module
    // doc (plan Task 3). Tolerance 1e-6 absorbs f32 accumulation.
    mod downscale_lanczos3 {
        use super::*;

        #[test]
        fn edge_impulse_matches_reference() {
            let mut v = [0.0f32; 12];
            v[0] = 1.0;
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(&out, &[0.332_843_9, -0.065_327_91, 0.014_622_78, 0.0]);
        }

        #[test]
        fn center_impulse_matches_reference() {
            let mut v = [0.0f32; 12];
            v[6] = 1.0;
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(
                &out,
                &[-0.031_200_27, 0.127_278_3, 0.270_893_5, -0.048_750_42],
            );
        }

        #[test]
        fn ramp_10_to_4_matches_reference() {
            let v: Vec<f32> = (0..10).map(|i| i as f32 / 9.0).collect();
            let out = downscale_lanczos3(&row(&v), 4, 1).unwrap();
            assert_row_close(&out, &[0.079_193_74, 0.362_135_4, 0.637_864_6, 0.920_806_3]);
        }

        #[test]
        fn keeps_a_constant_image_constant() {
            let src = ImageF32::new_filled(64, 48, [0.25, 0.5, 0.75]).unwrap();
            let out = downscale_lanczos3(&src, 30, 17).unwrap();
            for p in out.pixels() {
                let ok = (p[0] - 0.25).abs() < 1e-5
                    && (p[1] - 0.5).abs() < 1e-5
                    && (p[2] - 0.75).abs() < 1e-5;
                assert!(ok, "{p:?}");
            }
        }

        #[test]
        fn three_by_one_to_one_by_one_uses_clamped_taps() {
            let out = downscale_lanczos3(&row(&[0.4, 0.4, 0.4]), 1, 1).unwrap();
            assert_row_close(&out, &[0.4]);
        }

        #[test]
        fn unchanged_axes_are_copied_bit_exact() {
            let src = row(&[0.1, 0.7, 0.3, 0.9, 0.2]);
            assert_eq!(downscale_lanczos3(&src, 5, 1).unwrap(), src);
        }

        #[test]
        fn returns_requested_dimensions() {
            let src = ImageF32::new_filled(100, 70, [0.0; 3]).unwrap();
            let out = downscale_lanczos3(&src, 50, 35).unwrap();
            assert_eq!((out.width(), out.height()), (50, 35));
        }

        #[test]
        fn rejects_upscale() {
            let src = ImageF32::new_filled(10, 10, [0.0; 3]).unwrap();
            let err = downscale_lanczos3(&src, 20, 10).unwrap_err();
            let want = ResampleError::Upscale {
                src_w: 10,
                src_h: 10,
                dst_w: 20,
                dst_h: 10,
            };
            assert_eq!(err, want);
        }

        #[test]
        fn rejects_empty_target() {
            let src = ImageF32::new_filled(10, 10, [0.0; 3]).unwrap();
            let err = downscale_lanczos3(&src, 0, 5).unwrap_err();
            assert_eq!(
                err,
                ResampleError::EmptyTarget {
                    width: 0,
                    height: 5
                }
            );
        }
    }

    mod scoring_image {
        use super::*;

        fn gradient_rgba(w: u32, h: u32) -> Rgba8 {
            let mut data = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    data.extend_from_slice(&[
                        (x % 256) as u8,
                        (y % 256) as u8,
                        ((x + y) % 256) as u8,
                        255,
                    ]);
                }
            }
            Rgba8::new(w, h, data).unwrap()
        }

        #[test]
        fn produces_1024x683_from_2048x1365() {
            let out = scoring_image(&gradient_rgba(2048, 1365), 1024).unwrap();
            assert_eq!((out.width(), out.height()), (1024, 683));
        }

        #[test]
        fn returns_the_same_pixels_when_already_small() {
            let src = gradient_rgba(40, 30);
            assert_eq!(scoring_image(&src, 1024).unwrap(), src);
        }

        #[test]
        fn forces_alpha_to_255() {
            let src = Rgba8::new(1, 1, vec![10, 20, 30, 0]).unwrap();
            assert_eq!(
                scoring_image(&src, 1024).unwrap().data(),
                &[10, 20, 30, 255]
            );
        }

        #[test]
        fn keeps_a_flat_gray_flat() {
            let src = Rgba8::new(300, 200, [77u8, 77, 77, 255].repeat(300 * 200)).unwrap();
            let out = scoring_image(&src, 100).unwrap();
            assert!(out.data().chunks_exact(4).all(|p| p == [77, 77, 77, 255]));
        }

        #[test]
        fn rejects_zero_long_edge() {
            let err = scoring_image(&gradient_rgba(4, 4), 0).unwrap_err();
            assert_eq!(
                err,
                ResampleError::EmptyTarget {
                    width: 0,
                    height: 0
                }
            );
        }
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core resample::` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
//! Downscaling for the scoring path (master plan A4, AC-S1h). Every rule below is part of the
//! score contract; changing any of them requires a `SCORING_VERSION` bump.
//!
//! - kernel Lanczos3 (`a = 3`), evaluated at `(j - center) / scale`, `scale = src / dst`
//! - pixel centers at `i + 0.5`: `center = (i + 0.5) * scale - 0.5`
//! - taps `j` span `floor(center - 3*scale) ..= ceil(center + 3*scale)` in full; only the
//!   **sample index** is clamped to `[0, len - 1]` (clamp-to-edge), weights are never dropped
//! - weights normalized to sum 1, accumulated in ascending `j`, f32
//! - separable: horizontal pass, then vertical pass, both iterating rows in order
//! - an axis whose size does not change is copied unchanged (no filtering)

use crate::buffer::{dequantize, quantize, BufferError, ImageF32, Rgba8};
use crate::color::{srgb_decode, srgb_encode};
use thiserror::Error;

/// Errors from resampling.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResampleError {
    /// Requested target has a zero dimension.
    #[error("empty target size {width}x{height}")]
    EmptyTarget {
        /// Target width.
        width: u32,
        /// Target height.
        height: u32,
    },
    /// Target larger than source on some axis (this module only downscales).
    #[error("upscaling is not supported: {src_w}x{src_h} -> {dst_w}x{dst_h}")]
    Upscale {
        /// Source width.
        src_w: u32,
        /// Source height.
        src_h: u32,
        /// Target width.
        dst_w: u32,
        /// Target height.
        dst_h: u32,
    },
    /// Buffer construction failed.
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

/// Target size whose longer edge equals `long_edge` (never upscales; short edge ≥ 1).
/// The short edge is `floor(short * long_edge / long + 0.5)`, computed in f64.
pub fn fit_long_edge(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= long_edge {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(long);
    let short = (f64::from(width.min(height)) * scale + 0.5)
        .floor()
        .max(1.0) as u32;
    if width >= height {
        (long_edge, short)
    } else {
        (short, long_edge)
    }
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
    if x.abs() >= 3.0 {
        0.0
    } else {
        sinc(x) * sinc(x / 3.0)
    }
}

/// Per target index: `(clamped source index, normalized weight)` taps in ascending `j`.
fn axis_taps(src_len: u32, dst_len: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = src_len as f32 / dst_len as f32;
    let radius = 3.0 * scale;
    let last = i64::from(src_len) - 1;
    (0..dst_len)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale - 0.5;
            let lo = (center - radius).floor() as i64;
            let hi = (center + radius).ceil() as i64;
            let raw: Vec<(usize, f32)> = (lo..=hi)
                .map(|j| {
                    (
                        j.clamp(0, last) as usize,
                        lanczos3((j as f32 - center) / scale),
                    )
                })
                .collect();
            let sum: f32 = raw.iter().map(|&(_, w)| w).sum();
            raw.into_iter().map(|(idx, w)| (idx, w / sum)).collect()
        })
        .collect()
}

fn horizontal(src: &ImageF32, dst_w: u32) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    let taps = axis_taps(sw, dst_w);
    let row_len = sw as usize * 3;
    let mut out = Vec::with_capacity(dst_w as usize * sh as usize * 3);
    for row in src.samples().chunks_exact(row_len) {
        for t in &taps {
            let mut acc = [0.0f32; 3];
            for &(idx, w) in t {
                let p = &row[idx * 3..idx * 3 + 3];
                acc[0] += p[0] * w;
                acc[1] += p[1] * w;
                acc[2] += p[2] * w;
            }
            out.extend_from_slice(&acc);
        }
    }
    Ok(ImageF32::from_samples(dst_w, sh, out)?)
}

fn vertical(src: &ImageF32, dst_h: u32) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    let taps = axis_taps(sh, dst_h);
    let row_len = sw as usize * 3;
    let samples = src.samples();
    let mut out = Vec::with_capacity(row_len * dst_h as usize);
    for t in &taps {
        let mut acc = vec![0.0f32; row_len];
        for &(idx, w) in t {
            let row = &samples[idx * row_len..(idx + 1) * row_len];
            for (a, s) in acc.iter_mut().zip(row) {
                *a += s * w;
            }
        }
        out.extend_from_slice(&acc);
    }
    Ok(ImageF32::from_samples(sw, dst_h, out)?)
}

/// Lanczos3 downscale of a **linear-light** image to exactly `dst_w`×`dst_h`.
pub fn downscale_lanczos3(
    src: &ImageF32,
    dst_w: u32,
    dst_h: u32,
) -> Result<ImageF32, ResampleError> {
    let (sw, sh) = (src.width(), src.height());
    if dst_w == 0 || dst_h == 0 {
        return Err(ResampleError::EmptyTarget {
            width: dst_w,
            height: dst_h,
        });
    }
    if dst_w > sw || dst_h > sh {
        return Err(ResampleError::Upscale {
            src_w: sw,
            src_h: sh,
            dst_w,
            dst_h,
        });
    }
    let tmp = if dst_w == sw {
        src.clone()
    } else {
        horizontal(src, dst_w)?
    };
    if dst_h == sh {
        Ok(tmp)
    } else {
        vertical(&tmp, dst_h)
    }
}

/// THE scoring-image function (master plan A4): RGBA8 → dequantize → sRGB decode (linear) →
/// Lanczos3 to `long_edge` → sRGB encode → quantize → RGBA8 (alpha 255). CLI, golden and wasm
/// all call this; a saved answer PNG reproduces the scoring input exactly.
pub fn scoring_image(src: &Rgba8, long_edge: u32) -> Result<Rgba8, ResampleError> {
    if long_edge == 0 {
        return Err(ResampleError::EmptyTarget {
            width: 0,
            height: 0,
        });
    }
    let (dw, dh) = fit_long_edge(src.width(), src.height(), long_edge);
    if (dw, dh) == (src.width(), src.height()) {
        let opaque = src
            .data()
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        return Ok(Rgba8::new(dw, dh, opaque)?);
    }
    let lut: Vec<f32> = (0..=255u8).map(|v| srgb_decode(dequantize(v))).collect();
    let linear: Vec<f32> = src
        .data()
        .chunks_exact(4)
        .flat_map(|p| {
            [
                lut[usize::from(p[0])],
                lut[usize::from(p[1])],
                lut[usize::from(p[2])],
            ]
        })
        .collect();
    let linear = ImageF32::from_samples(src.width(), src.height(), linear)?;
    let small = downscale_lanczos3(&linear, dw, dh)?;
    let bytes = small
        .pixels()
        .flat_map(|p| {
            [
                quantize(srgb_encode(p[0])),
                quantize(srgb_encode(p[1])),
                quantize(srgb_encode(p[2])),
                255,
            ]
        })
        .collect();
    Ok(Rgba8::new(dw, dh, bytes)?)
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core resample::` / Expected: 모두 PASS.

- [ ] **Step 5: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/resample.rs engine/core/src/lib.rs
git commit -m "feat(engine): Lanczos3 채점 이미지 (샘플 인덱스 clamp-to-edge, 단일 scoring_image)"
```

---

## Task 4: 참조 디코드/인코드와 wasm32 빌드 확인

**Files:**
- Modify: `engine/core/src/decode.rs`

**Interfaces:**
- Consumes: Task 2 `Rgba8`, `checked_len`, `BufferError`.
- Produces: `decode_rgba8(&[u8])->Result<Rgba8,DecodeError>`, `encode_png(&Rgba8)->Result<Vec<u8>,DecodeError>`, `enum DecodeError{UnknownFormat, Jpeg(String), Png(String), Unsupported(String), Buffer}`.

API 근거(확인한 소스): `zune-jpeg 0.5.15` `JpegDecoder::new_with_options(ZCursor, DecoderOptions)`, `decode_headers()`, `input_colorspace()`, `dimensions()->Option<(usize,usize)>`, `decode()->Vec<u8>`; 재노출된 `zune_jpeg::zune_core`(0.5.3) `DecoderOptions::{jpeg_set_out_colorspace, set_use_unsafe, set_strict_mode}`, `ColorSpace::{YCbCr,RGB,Luma,CMYK,YCCK}`. SIMD는 `x86`/`neon` 기능 뒤에만 있고 둘 다 끄면 크레이트가 `forbid(unsafe_code)`가 된다. 디코더 스레드는 쓰지 않는다. `png 0.18.1` `Decoder::new(BufRead+Seek)`, `set_transformations(EXPAND)`, `Reader::{info, output_color_type, output_buffer_size, next_frame}`, `Encoder::{set_color, set_depth, write_header}`, `Writer::{write_image_data, finish}`. 테스트용 `jpeg-encoder 0.7.1` `Encoder::new(&mut Vec, q)`, `set_sampling_factor(SamplingFactor::R_4_2_0)`, `set_progressive`, `encode(data, u16, u16, ColorType::{Rgb,Rgba,Luma,Cmyk})`.

- [ ] **Step 1: 실패하는 테스트 작성** — Step 3 구현을 `fn` 본문 `todo!()`로 두고 아래 테스트를 붙인다.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use jpeg_encoder::{ColorType, Encoder, SamplingFactor};

    fn checker_rgb(w: u16, h: u16) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let on = (x / 4 + y / 4) % 2 == 0;
                v.extend_from_slice(if on { &[200, 60, 30] } else { &[20, 90, 180] });
            }
        }
        v
    }

    fn jpeg(data: &[u8], w: u16, h: u16, color: ColorType, progressive: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = Encoder::new(&mut out, 90);
        enc.set_sampling_factor(SamplingFactor::R_4_2_0);
        enc.set_progressive(progressive);
        enc.encode(data, w, h, color).unwrap();
        out
    }

    fn png_bytes(
        w: u32,
        h: u32,
        color: png::ColorType,
        depth: png::BitDepth,
        data: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(depth);
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(data).unwrap();
        wr.finish().unwrap();
        out
    }

    #[test]
    fn png_round_trips_pixels_exactly() {
        let data = (0..12u8)
            .flat_map(|i| [i * 20, 255 - i, i * 7, 255])
            .collect();
        let img = Rgba8::new(4, 3, data).unwrap();
        assert_eq!(decode_rgba8(&encode_png(&img).unwrap()).unwrap(), img);
    }

    #[test]
    fn decodes_rgb_png_with_opaque_alpha() {
        let bytes = png_bytes(
            2,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[1, 2, 3, 4, 5, 6],
        );
        assert_eq!(
            decode_rgba8(&bytes).unwrap().data(),
            &[1, 2, 3, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn decodes_grayscale_png_to_gray_rgb() {
        let bytes = png_bytes(1, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[77]);
        assert_eq!(decode_rgba8(&bytes).unwrap().data(), &[77, 77, 77, 255]);
    }

    #[test]
    fn rejects_16_bit_png() {
        let bytes = png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Sixteen, &[0; 6]);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn rejects_translucent_png() {
        let bytes = png_bytes(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[9, 9, 9, 128],
        );
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn decodes_baseline_420_jpeg_close_to_source() {
        let src: Vec<u8> = (0..48u8)
            .flat_map(|y| (0..64u8).flat_map(move |x| [x * 4, y * 5, 128]))
            .collect();
        let out = decode_rgba8(&jpeg(&src, 64, 48, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (64, 48));
        let mean_err: f64 = out
            .data()
            .chunks_exact(4)
            .zip(src.chunks_exact(3))
            .map(|(d, s)| (0..3).map(|i| f64::from(d[i].abs_diff(s[i]))).sum::<f64>())
            .sum::<f64>()
            / (64.0 * 48.0 * 3.0);
        assert!(mean_err < 3.0, "mean abs error {mean_err}");
    }

    #[test]
    fn progressive_and_baseline_jpeg_decode_to_the_same_size() {
        let src = checker_rgb(33, 17);
        let a = decode_rgba8(&jpeg(&src, 33, 17, ColorType::Rgb, false)).unwrap();
        let b = decode_rgba8(&jpeg(&src, 33, 17, ColorType::Rgb, true)).unwrap();
        assert_eq!(
            (a.width(), a.height(), b.width(), b.height()),
            (33, 17, 33, 17)
        );
    }

    #[test]
    fn decodes_grayscale_jpeg_to_gray_rgb() {
        let out = decode_rgba8(&jpeg(&[128; 64], 8, 8, ColorType::Luma, false)).unwrap();
        assert!(out
            .data()
            .chunks_exact(4)
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255));
    }

    #[test]
    fn rejects_cmyk_jpeg() {
        let bytes = jpeg(&[0, 0, 0, 0].repeat(64), 8, 8, ColorType::Cmyk, false);
        assert!(matches!(
            decode_rgba8(&bytes),
            Err(DecodeError::Unsupported(_))
        ));
    }

    #[test]
    fn decodes_a_1x1_jpeg() {
        let out = decode_rgba8(&jpeg(&[10, 200, 30], 1, 1, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (1, 1));
    }

    #[test]
    fn decodes_a_3000x1_jpeg() {
        let out = decode_rgba8(&jpeg(&[90; 9000], 3000, 1, ColorType::Rgb, false)).unwrap();
        assert_eq!((out.width(), out.height()), (3000, 1));
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert_eq!(
            decode_rgba8(b"not an image"),
            Err(DecodeError::UnknownFormat)
        );
    }

    #[test]
    fn rejects_truncated_jpeg() {
        let bytes = jpeg(&checker_rgb(64, 48), 64, 48, ColorType::Rgb, false);
        assert!(decode_rgba8(&bytes[..bytes.len() / 2]).is_err());
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core decode::` / Expected: FAIL.

- [ ] **Step 3: 구현**

```rust
//! Reference-path decode/encode (master plan A2). JPEG goes through `zune-jpeg` built without
//! its `x86`/`neon` features, so every target runs the same scalar IDCT, upsampler and color
//! converter. PNG goes through `png`. Browser `<img>` decoding never feeds the reference path.
//!
//! Accepted input: 8-bit baseline/progressive JPEG in YCbCr, RGB or grayscale; 8-bit PNG
//! (gray/RGB/palette, alpha only if every pixel is opaque). Everything else is rejected
//! instead of being silently converted (16-bit PNG, translucent PNG, CMYK/YCCK JPEG).
//! EXIF orientation and ICC profiles are NOT applied here; content originals are normalized
//! to sRGB with orientation baked in at build time (AC-E1c, Plan 3).

use crate::buffer::{checked_len, BufferError, Rgba8};
use std::io::Cursor;
use thiserror::Error;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// Decode/encode errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    /// Bytes are neither JPEG nor PNG.
    #[error("unrecognized image format (expected JPEG or PNG)")]
    UnknownFormat,
    /// The JPEG decoder rejected the bytes.
    #[error("jpeg: {0}")]
    Jpeg(String),
    /// The PNG decoder/encoder rejected the bytes.
    #[error("png: {0}")]
    Png(String),
    /// A well-formed image the reference path does not accept.
    #[error("unsupported image: {0}")]
    Unsupported(String),
    /// Dimensions out of bounds.
    #[error(transparent)]
    Buffer(#[from] BufferError),
}

const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Decode JPEG or PNG bytes to RGBA8 (alpha 255).
pub fn decode_rgba8(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    if bytes.starts_with(JPEG_MAGIC) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(PNG_MAGIC) {
        decode_png(bytes)
    } else {
        Err(DecodeError::UnknownFormat)
    }
}

fn to_u32(v: usize) -> Result<u32, DecodeError> {
    u32::try_from(v).map_err(|_| DecodeError::Unsupported(format!("dimension {v} too large")))
}

fn decode_jpeg(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_use_unsafe(false)
        .set_strict_mode(true);
    let mut dec = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    dec.decode_headers()
        .map_err(|e| DecodeError::Jpeg(e.to_string()))?;
    match dec.input_colorspace() {
        Some(ColorSpace::YCbCr | ColorSpace::RGB | ColorSpace::Luma) => {}
        other => {
            return Err(DecodeError::Unsupported(format!(
                "jpeg colorspace {other:?}"
            )))
        }
    }
    let (w, h) = dec
        .dimensions()
        .ok_or_else(|| DecodeError::Jpeg("missing dimensions".into()))?;
    let (w, h) = (to_u32(w)?, to_u32(h)?);
    checked_len(w, h, 4)?;
    let rgb = dec.decode().map_err(|e| DecodeError::Jpeg(e.to_string()))?;
    let rgba = rgb
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    Ok(Rgba8::new(w, h, rgba)?)
}

fn decode_png(bytes: &[u8]) -> Result<Rgba8, DecodeError> {
    let png_err = |e: png::DecodingError| DecodeError::Png(e.to_string());
    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().map_err(png_err)?;
    let (w, h) = reader.info().size();
    checked_len(w, h, 4)?;
    let (color, depth) = reader.output_color_type();
    if depth != png::BitDepth::Eight {
        return Err(DecodeError::Unsupported(format!("png bit depth {depth:?}")));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| DecodeError::Png("output buffer size overflow".into()))?;
    let mut buf = vec![0u8; size];
    let frame = reader.next_frame(&mut buf).map_err(png_err)?;
    let px = &buf[..frame.buffer_size()];
    let opaque = |a: u8| {
        if a == 255 {
            Ok(())
        } else {
            Err(DecodeError::Unsupported("png with transparency".into()))
        }
    };
    let rgba: Vec<u8> = match color {
        png::ColorType::Rgb => px
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Rgba => {
            px.chunks_exact(4).try_for_each(|p| opaque(p[3]))?;
            px.to_vec()
        }
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            px.chunks_exact(2).try_for_each(|p| opaque(p[1]))?;
            px.chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], 255])
                .collect()
        }
        png::ColorType::Indexed => {
            return Err(DecodeError::Unsupported("png palette not expanded".into()))
        }
    };
    Ok(Rgba8::new(w, h, rgba)?)
}

/// Encode RGBA8 as PNG (lossless; answer references, AC-S1d). Byte output may change with the
/// `png` crate version; only decoded pixels are ever hashed.
pub fn encode_png(img: &Rgba8) -> Result<Vec<u8>, DecodeError> {
    let enc_err = |e: png::EncodingError| DecodeError::Png(e.to_string());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(enc_err)?;
    writer.write_image_data(img.data()).map_err(enc_err)?;
    writer.finish().map_err(enc_err)?;
    Ok(out)
}
```

- [ ] **Step 4: 통과 확인** — Run: `cargo test --locked --manifest-path engine/Cargo.toml -p engine-core decode::` / Expected: 모두 PASS.

- [ ] **Step 5: 디코더 wasm32 빌드 확인**

```bash
cargo build --locked --manifest-path engine/Cargo.toml -p engine-core --target wasm32-unknown-unknown
bash scripts/lint-determinism.sh
```
Expected: `Finished`, `determinism lint: ok`(zune-jpeg에 `x86`/`neon`/`default` 기능이 없음을 함께 확인).

- [ ] **Step 6: fmt·clippy·커밋**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all
cargo clippy --locked --manifest-path engine/Cargo.toml -p engine-core --all-targets -- -D warnings
git add engine/core/src/decode.rs
git commit -m "feat(engine): 스칼라 zune-jpeg·png 참조 디코드, 지원 외 형식 거부"
```

---

## 분할 마무리: 인수 확인과 PR

- [ ] **Step 1: 완료 인수 조건 실행** — 이 문서 머리의 명령 6개를 순서대로 실행한다. Expected: 모두 성공.

- [ ] **Step 2: 푸시·PR·필수 체크 대기**

```bash
git push -u origin feat/engine-1a-1-numeric-image
gh pr create --base main --head feat/engine-1a-1-numeric-image --title "Plan 1a-1: 수치·이미지 기반" --body "Plan 1a-1(.omc/plans/01a-1-numeric-image.md) Task 0~4. 색 과학, 버퍼, scoring_image, 참조 디코드, CI 기본 잡."
gh pr checks --watch --required
```
Expected: 필수 체크 전부 pass. 병합은 사용자 확인 후.
