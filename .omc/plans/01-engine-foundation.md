# Plan 1a: 엔진 기반 (Engine Foundation) - 인덱스와 공통 계약

> **For agentic workers:** 이 문서는 Plan 1a의 인덱스다. 실제 태스크는 아래 4개 분할 파일에 있고, 분할마다 따로 승인·실행·PR한다. 각 분할은 REQUIRED SUB-SKILL: superpowers:subagent-driven-development(권장) 또는 superpowers:executing-plans 로 실행한다. Rust 코드를 쓰기 전에 `rust-best-practices` 스킬을 로드한다. 이 문서의 "공통 계약"은 모든 분할의 요구 사항에 암묵적으로 포함된다.

**Goal:** 브라우저·OS가 달라도 같은 픽셀과 같은 점수를 내는 참조 엔진의 뼈대를 만든다. 색 과학(ΔE00), 결정적 JPEG/PNG 디코드, 선형광 Lanczos3 채점 이미지, 레시피 스키마 v1 전체, 기본 패널 렌더, `ScoringReference` 기반 채점 핵심, 네이티브 CLI, WASM 바인딩, 3-OS 네이티브(debug·release) + wasm 골든 CI, 성능 기준선.

**Architecture:** 라이브러리 `engine-core`가 모든 수학을 소유하고 `engine-cli`(네이티브)와 `engine-wasm`(wasm-bindgen)은 얇은 껍데기다. 초월함수는 `libm`(`force-soft-floats`)만, JPEG은 SIMD를 끈 `zune-jpeg`, PNG는 `png`. 렌더는 f32이며 중간에 클램프하지 않는다. 8bit 양자화는 `buffer::quantize` 하나뿐이고 두 지점(① 2048 렌더 최종 출력 = 정답 PNG, ② `scoring_image`의 1024 출력)에서만 호출된다. 렌더는 항상 전체 프레임이다(레시피 `crop` 미적용, A10). 채점은 챌린지 로드 시 원본 2048·정답 2048·영역 명세로 `ScoringReference`를 한 번 만들고, 제출마다 플레이어 2048 렌더를 넣는다(A5).

**Tech Stack:** Rust 1.98.1 고정(`rust-toolchain.toml`, MSRV 1.87), `serde`/`serde_json`, `libm 0.2`(`default-features=false`, `force-soft-floats`), `zune-jpeg 0.5`(`default-features=false`, `std`), `png 0.18`, `sha2 0.10`, `thiserror 2`, `jpeg-encoder 0.7`(CLI·테스트의 픽스처 인코드), `clap 4`(색 출력 끔), `anyhow 1`(CLI), `wasm-bindgen 0.2`, `wasm-bindgen-test 0.3`, wasm-pack 0.15.0, Node 24 `node --test`.

**Spec:** `.omc/specs/deep-interview-photo-edit-match-game.md`(v2). 계약: `.omc/plans/00-master-plan.md` §2(A1~A10)·§3·§5.

---

## 분할 구성

| 분할 | 파일 | 태스크 | 브랜치 / PR 제목 | 선행 조건 | 산출물 |
|------|------|--------|------------------|-----------|--------|
| 1 | `01a-1-numeric-image.md` | 0~4 | `feat/engine-1a-1-numeric-image` / "Plan 1a-1: 수치·이미지 기반" | 없음 | 워크스페이스·lint·CI(fmt/clippy/test 3-OS), 색 과학, 버퍼, `scoring_image`, 디코드 |
| 2 | `01a-2-recipe-render.md` | 5a~7 | `feat/engine-1a-2-recipe-render` / "Plan 1a-2: 레시피·렌더" | 분할 1 병합 | 레시피 스키마 v1·검증, 기본 패널, 렌더 파이프라인 |
| 3 | `01a-3-region-score-wasm.md` | 8~10 | `feat/engine-1a-3-region-score-wasm` / "Plan 1a-3: 영역·채점·WASM" | 분할 2 병합 | 영역·해시, 만점 게이트, `ScoringReference`, WASM API, 네이티브==wasm 스모크, CI wasm 단계 |
| 4 | `01a-4-cli-golden-ci.md` | 11a~14 | `feat/engine-1a-4-cli-golden-ci` / "Plan 1a-4: CLI·골든·CI" | 분할 3 병합 | CLI, CC0 골든·버전 가드, wasm 골든, CI 골든 단계, 성능 기준선 |

**분할 간 인수 조건.** 다음 분할은 이전 분할의 PR이 `main`에 병합되고, 그 병합 커밋에서 아래가 성공한 뒤 시작한다.

| 분할 끝 | 반드시 성공해야 하는 명령 (저장소 루트) |
|---------|----------------------------------------|
| 1 | `bash scripts/lint-determinism.sh`, `cargo fmt … --check`, `cargo clippy --locked … --workspace --all-targets -D warnings`, `cargo test --locked … --workspace`, `cargo build --locked … -p engine-core --target wasm32-unknown-unknown`, CI 3-OS 녹색 |
| 2 | 분할 1 항목 전부 |
| 3 | 분할 1 항목 + `wasm-pack test --node engine/wasm --locked`, `node --test "engine/wasm/test/*.test.mjs"`(스모크·오류·소유권 6개), CI 3-OS 녹색 |
| 4 | 분할 3 항목 + `golden check`(debug·release), wasm 골든, `node --test engine/golden/test/verify-sources.test.mjs`, `node engine/golden/verify-sources.mjs`, `git diff --exit-code -- engine/Cargo.lock`, CI 3-OS 녹색 |

분할 1~3의 완료는 마스터 §5 중 §5.1~5.5만 뜻한다. §5.6(골든·버전 가드)은 분할 4가 끝나야 성립하므로, 분할 1~3 병합 시점의 `main`은 "결정성 골든 미보호" 상태다.

분할 사이에서 공개 인터페이스(아래 "공통 계약"과 각 태스크의 Interfaces)는 바뀌지 않는다. 바꿔야 하면 이 인덱스와 마스터 플랜을 먼저 고친다.

---

## 사전 조건

- rustup 1.29+, wasm-pack 0.15.0, Node 24, Git Bash(Windows).
- 툴체인 1.98.1과 `wasm32-unknown-unknown`·clippy·rustfmt는 저장소 루트의 `rust-toolchain.toml`이 지정한다. 처음 한 번 `rustup toolchain install`(인자 없음: toolchain 파일을 읽는다).
- Windows: Visual Studio 2026의 **"C++를 사용한 데스크톱 개발" 워크로드(MSVC + Windows SDK)** 가 필요하다(`x86_64-pc-windows-msvc` 호스트의 링커). 2026-10-05 이 PC에 설치되어 `link.exe`(MSVC 14.51)를 확인했다. Git Bash의 `/usr/bin/link`는 rustc가 MSVC 링커를 직접 찾으므로 문제되지 않는다.
- 모든 명령은 **저장소 루트**에서 `--manifest-path engine/Cargo.toml`로 실행하고 cargo에는 `--locked`를 붙인다(`cd engine` 금지). JSON 처리는 `engine-cli` 또는 Node로만 한다(python 금지).

## 검증 상태

이 계획의 모든 Rust·JS 코드는 2026-10-05 임시 작업 공간에서 **MSVC 툴체인 1.98.1(x86_64-pc-windows-msvc)** 으로 다시 빌드·실행했다.

- 최종 상태: `lint-determinism` ok, `fmt --check` ok, `clippy --workspace --all-targets -D warnings`(네이티브·wasm32) ok, `cargo test --locked --workspace` 전부 통과(core 165, CLI 단위 30, CLI 통합 14, 스모크 1), `wasm-pack test --node engine/wasm --locked` ok(Sharma 1, 오류 경로 5), `node --test` wasm 8개와 `verify-sources` 7개 통과, `golden check` debug·release ok, **골든 5케이스(점수 98·99·100 경계 포함)와 채점 프로브 4개가 네이티브 debug·release와 wasm에서 플레이어별 2048 렌더·1024 채점 이미지 해시와 Score JSON 문자열까지 완전 일치**, CLI 산출물(`gen-answer`·`render`·`score`)을 wasm이 그대로 재현, `verify-sources` ok(generator 커밋 존재와 그 커밋의 `engine/Cargo.lock` SHA-256 일치 포함), `Cargo.lock` 변화 없음. 가드는 골든이 같아도 `zune-jpeg` 버전만 바꾼 `Cargo.lock`을 거부한다(확인함).
- 중간 상태(분할 1·2·3 끝, Task 0·5a·5b·6·9a·11a·11b 끝)를 각각 별도 디렉터리에 재구성해 fmt·clippy·test가 통과함을 확인했다. 분할 3 끝에서는 wasm32 clippy·`wasm-pack test`·스모크 node 테스트도 확인했다.
- 고의 불일치 확인: `expected.json`의 점수 하나를 바꾸면 `golden check`가 exit 1과 `actual-native-release.json`을, wasm 테스트가 실패와 `actual-wasm.json`을 남긴다. 계산·판독 실패는 `{case,player,stage,error}` 보고서를 남긴다. 순수 채점 수식 변경(`provisional_diag` 반올림)을 `ENGINE_VERSION`만 올려 넣으면 `golden write`가 채점 프로브로 거부한다.
- 크로스 툴체인 확인: 같은 `expected.json`이 GNU 호스트 빌드(1차 검증)와 MSVC 빌드에서 모두 통과했고, `fixture`·`prep-photo` 산출 JPEG의 SHA-256도 두 툴체인에서 같았다.
- Linux·macOS(arm64)는 CI가 처음 확인한다.

## 공통 계약 (Global Constraints)

- 렌더 중간값 비클램프. 양자화는 `buffer::quantize` 하나, 호출 지점은 ① 렌더 최종 ② `scoring_image`뿐(A3).
- 채점 이미지는 `engine_core::scoring_image(&Rgba8, 1024)` 단일 함수(A4).
- 평가 영역은 core가 `RegionSpec` JSON에서 원본 해상도로 래스터화한 뒤 채점 해상도로 매핑한다(A5). 1a는 `{"kind":"full"}`만.
- 렌더는 항상 전체 프레임, `crop` 미적용(A10). 구도 입력(`CompositionInput`)은 1a에서 상자 없음만 허용.
- 초월함수는 `libm::*`만. `mul_add`·`libm::fma*` 금지. `.cargo/config*` 금지. `RUSTFLAGS` 비어 있음(§5).
- `image` 크레이트 의존 금지(전이 기능으로 zune-jpeg SIMD가 켜진다). JPEG은 `zune-jpeg` 직접.
- 렌더·채점 수식 변경 시 `ENGINE_VERSION`/`SCORING_VERSION` 범프 + `engine-cli golden write`. 같은 버전으로 결과가 바뀌거나 사라지면 `golden write`와 CI guard가 거부한다. 픽셀·점수를 결정하는 크레이트(마스터 §5.3)의 `Cargo.lock` 버전이 담당 버전 범프 없이 바뀌어도 CI guard가 거부한다.
- 모든 cargo 명령 `--locked`, CI 끝에 `git diff --exit-code -- engine/Cargo.lock`.
- 커밋은 분할별 기능 브랜치에, 분할 끝에 PR. `main` 직접 푸시 금지. 커밋 메시지 본문·문구는 한국어, 코드 식별자·주석은 영어.

### TDD 규칙과 예외

코드 태스크는 "실패 테스트 → 실패 확인 → 구현 → 통과 → fmt·clippy → 커밋"을 따른다. 실패 확인 단계의 스텁은 `todo!()` 본문이다. 단, **`impl Trait`을 반환하는 함수는 `todo!()`가 E0277(`() is not an iterator`)로 컴파일되지 않으므로** 스텁을 `std::iter::empty()`로 둔다(확인함; 해당 함수는 `ImageF32::pixels` 하나). 예외로 RED 단계가 "테스트 작성"이 아닌 태스크:

- Task 0(스캐폴드): RED = 결정성 lint가 위반 코드를 잡는지 확인(Step 6).
- Task 12b(골든 데이터): 이 태스크의 코드 `verify-sources.mjs`는 Step 1에서 테스트 먼저 만든다(`process.exit(0)` 스텁으로 7개 FAIL → 구현 → PASS). 예외는 데이터 생성(Step 2~5)뿐이다. 판정 로직은 Task 12a가 RED→GREEN으로 검증했고, 데이터 단계의 RED는 속성 검사(선언 범위 밖 점수 거부)와 가드 변조 거부 확인이 대신한다.
- Task 13(wasm 골든·CLI 계약): 기존 골든과 Task 11b의 CLI 산출물을 대상으로 하므로 처음부터 통과한다. RED = `expected.json` 고의 변조 시 실패하고 `actual-wasm.json`이 생기는지, `engine/target/cli-contract/score.json` 변조 시 `cli_contract.test.mjs`가 실패하는지 확인. 복구 후 다시 실행해 통과를 확인한다.
- Task 14(CI): RED = 고의 변조 시 `golden check`가 실패하고 보고서를 쓰는지, guard가 버전 미변경 변경을 거부하는지 확인.

## 파일 구조

```
rust-toolchain.toml            1.98.1 + clippy/rustfmt + wasm32 (cargo가 cwd 기준으로 찾으므로 저장소 루트)
scripts/lint-determinism.sh    std 수학·FMA·RUSTFLAGS·.cargo/config·의존성 기능 검사
.github/workflows/ci.yml       engine 잡: 분할 1에서 교체, 3·4에서 단계 추가
engine/
├─ Cargo.toml, Cargo.lock, rustfmt.toml
├─ core/                       crate engine-core
│  ├─ testdata/sharma2005.csv  Sharma 34쌍 (core 단위 테스트와 wasm 테스트가 공유)
│  ├─ src/lib.rs               모듈, 버전 상수, SCORING_LONG_EDGE, scoring_image 재노출
│  ├─ src/color.rs             sRGB 확장 전달 함수(부호 대칭), Lab D50, CIEDE2000      [1]
│  ├─ src/buffer.rs            Rgba8, ImageF32, quantize/dequantize, MAX_PIXELS        [1]
│  ├─ src/resample.rs          fit_long_edge, Lanczos3, scoring_image                  [1]
│  ├─ src/decode.rs            zune-jpeg/png 디코드, PNG 인코드, 지원 외 거부           [1]
│  ├─ src/recipe/{mod,schema,validate}.rs  스키마 v1 전체, Normalized, 검증            [2]
│  ├─ src/render/{mod,basic}.rs            파이프라인, RenderContext, 기본 패널        [2]
│  ├─ src/region.rs            RegionSpec → RegionMask, 채점 해상도 매핑               [3]
│  ├─ src/hash.rs                                                                      [3]
│  ├─ src/score/{mod,gate}.rs  ScoringReference·Score JSON / 상대식·통계·게이트·매개변수 [3]
│  └─ tests/smoke.rs           교차 타깃 스모크(네이티브 쪽)                           [3]
├─ cli/src/{main,golden}.rs    engine-cli, 골든 write/check/guard                      [4]
├─ cli/tests/cli.rs                                                                     [4]
├─ wasm/src/lib.rs             §3.4 API (ScoringReference 클래스)                       [3]
├─ wasm/tests/color_vectors.rs wasm32 안에서 Sharma                                    [3]
├─ wasm/tests/core_errors.rs   wasm32 안에서 영역·통계 오류 경로                         [3]
├─ wasm/test/smoke.test.mjs [3], golden.test.mjs [4], cli_contract.test.mjs [4]
├─ wasm/bench/perf.mjs                                                                  [4]
└─ golden/  sources.json(+generator), verify-sources.mjs, test/verify-sources.test.mjs,
            cases.json(+probes), expected.json,
            PERF.md, images/(4), recipes/(11)                                           [4]
```
`[n]` = 만드는 분할.

## 렌더·채점 계약 (GLSL·JS 이식 기준, 마스터 A3·A4·A9·A10)

- 렌더 스테이지 순서(`render/mod.rs` 모듈 문서가 원문): ① `srgb_decode` ② 선형: 노출 `×2^EV` → WB ③ `srgb_encode` ④ 인코딩 영역: 대비 → 하이라이트(비례식 `x·(1+0.25·hi·w)`)·쉐도우·화이트·블랙 → 생동감·채도 ⑤ [1b 스테이지] ⑥ 유한성 검사. **어디에도 클램프가 없다.** 클램프는 `quantize`에서만 일어난다. WebGL2 프리뷰는 RGBA16F 이상 중간 타깃으로 같은 값을 유지한다(Plan 2).
- `srgb_decode`/`srgb_encode`는 확장형이다. 1 초과는 거듭제곱 곡선을 연장하고, 음수는 **W3C CSS Color 4와 같은 부호 대칭**(`f(-v) = -f(v)`)이다.
- 생동감 가중치 `w = clamp(1 - (max-min), 0, 1)`. 비클램프 입력에서 `max-min > 1`이어도 채도 계수 `k = (1+s)(1+v·w) ≥ 0`이라 색이 반전되지 않는다. GLSL도 같은 `clamp`를 쓴다.
- 렌더 출력 크기 = 입력 크기. `crop`은 표시용 `crop_rgba8`(1b)에서만 쓴다.
- 채점 이미지: RGBA8 → LUT(`srgb_decode(dequantize(i))`) → Lanczos3(커널 지지 구간 전체, **샘플 인덱스만** 가장자리 클램프, 가중치 정규화, 오름차순 누적, 수평→수직 행 순서, 크기 불변 축은 복사) → `srgb_encode` → `quantize` → RGBA8(알파 255).
- 영역 매핑(1a 규칙, `scoring_params.region_resample = "source_pixel_at_center"`): 채점 픽셀 `(x,y)`는 원본 픽셀 `floor((2x+1)·W/(2w))`의 알파를 쓴다(정수 연산).
- 타일 p95: nearest-rank `ceil(0.95·n)−1`. 큰 오차: ΔE00 **>** 5. 만점: 평균 ≤ 1, 타일 p95 ≤ `T_TILE`, 비율 ≤ 1%(모두 포함 부등호). 부적격: `D_E < 3` → `eligible=false`, 점수 0.
- 채점 매개변수는 `ScoringParams` 객체로 `versions().scoring_params`와 골든 `expected.json`에 기록된다.
- Score JSON 필드 순서는 계약이다(직렬화 테스트로 고정). 숫자는 `serde_json` 기본 포맷(최단 왕복 표현)이며 골든은 문자열로 비교한다.

### Plan 1b seam 노트

- **1b 첫 태스크 = 성능 게이트**(D3, 기준은 마스터 §4.1): Task 14 `PERF.md` 기준선(해제 누락을 고친 재측정값)을 출발점으로 비트 동일 최적화(스테이지 ① 256 LUT 등)를 한 뒤, 데스크톱 1대와 중급 모바일 1대(Galaxy A54 Chrome)에서 Worker 측정 경로로 로드·제출·반복 전환을 따로 잰다. 판정 항목은 제출 p95, 로드 p95, 피크 메모리, 반복 전환의 메모리 증가, 탭 재로드·크래시다(제안값, 1b 게이트에서 확정). 하나라도 어기면 기능 확장 전에 멈추고 해상도·범위를 재결정한다. 최대 복잡도 기능 완성 후 1b 마지막 태스크에서 같은 기준으로 재검증한다. **1a 기준선은 잠정 위험 신호이며 모바일 적합성을 입증하지 않는다.** 3회차 재측정 기준선(i7-10700K, Node 24.16, 해제 정리 후): 로드 p50 1031ms, 제출 p50 1767ms·p95 1776ms, 전환 p50 2805ms, wasm 메모리 140.8MiB, 전환 7회 증가 0. 1b 게이트는 1a `perf.mjs`의 측정 경로(정답 PNG 로드 → 참조, 참조 재사용 제출, 전환)를 그대로 브라우저 Worker로 옮긴다.
- 그레인은 `RenderContext.seed`(manifest `seed`, u32)만 쓴다. 1a는 받기만 한다(Task 7 `seed_does_not_change_a_basic_render`).
- 범위 마스크는 **원본 2048의 선형 픽셀**로 판정한다. `ScoringReference::new`가 원본 2048을 받으므로 `RegionSpec::rasterize`에 원본을 넘기도록 시그니처만 넓히면 된다.
- 크롭: 채점 렌더는 전체 프레임 그대로(A10). 크롭 챌린지의 E는 `RegionSpec::Crop`, 구도 점수는 `ScoringReference::score`의 `CompositionInput`으로 받는다.
- `unsupported_feature`의 "활성" 정의(예: 그레이딩은 sat·lum이 0이 아니면 활성)를 1b 렌더러도 그대로 무효과로 취급한다.
- 진단 tone·color 임시 산식은 AC-S2 산식으로 교체하고 `SCORING_VERSION`을 올린다.

---

## 전체 스펙 커버리지

열: **완료 조건** / **1a 범위** / **후속 담당** / **검증 테스트**(분할-태스크).

| AC | 완료 조건 | 1a 범위 | 후속 담당 | 검증 테스트 |
|----|-----------|---------|-----------|-------------|
| E1a | 골든 레시피 세트의 2048 렌더·1024 채점 이미지 SHA-256과 Score JSON 문자열이 3-OS 네이티브(debug·release)와 wasm(node)에서 일치 | 부분: CI 런너만(정답·플레이어마다 렌더·채점 이미지 해시·Score, 채점 프로브, CLI 산출물의 wasm 재현) | 실기기 브라우저 Plan 2 | 4-12b `golden check`, 4-13 `golden.test.mjs`·`cli_contract.test.mjs`, 4-14 CI |
| E1c | (AC 원문) "제작 자산은 EXIF 방향을 적용한 SDR sRGB 2048px JPEG로 정규화하고, 챌린지 매니페스트에 원본 파일 해시와 정규화 자산 해시를 기록한다." | 부분: 참조 경로 디코드를 엔진 내부 디코더(zune-jpeg 스칼라·png)로 수행, 원본 RGBA8 SHA-256을 `meta.json`에 기록, 지원 외 형식(CMYK 등) 거부. AC 자체는 1a에서 충족되지 않는다 | EXIF 방향 적용·SDR sRGB 정규화·2048 JPEG 정규화, 매니페스트의 원본 파일 해시·정규화 자산 해시 기록은 Plan 3(build-content) | 1-4 decode, 4-11b `meta.json` |
| E1e | 렌더 중간 f32 비클램프, 최종 출력에서만 양자화 | 전체 | - | 2-7 비클램프·하이라이트 복구·생동감 반전 없음, 1-2 quantize |
| E3c | 비유한 수가 렌더·채점에 도달하지 않음 | 부분: 파싱·정규화 거부, 렌더 유한성 검사, 극값 1024조합(렌더·채점) | 1b 기능별 확장 | 2-5a·5b, 2-7 `extreme_basic_values_stay_finite...`, 3-9b `extreme_basic_values_score_to_finite_numbers` |
| E3d | 잘못된 값은 거부 또는 경로 포함 정규화 보고 | 부분: JSON(엔진·wasm `validate_recipe`) | UI Plan 2, XMP 1b | 2-5b, 3-10 `validate_recipe reports...` |
| S1a | 평균·타일 p95·큰 오차 3중 만점 | 전체(`T_TILE=2.0` 임시) | `T_TILE` 확정 Plan 3 | 3-9a 경계, 3-9b `perfect_gate_through_score` |
| S1b | 채점 매개변수 기록, 변경 시 범프 강제 | 전체 | - | 3-9a 스냅숏, 4-12a guard 단위 테스트, 4-14 CI guard |
| S1c | 만점 미달 ≤ 99 | 전체 | - | 3-9a·9b(조건별 단독 실패, 마지막 부분 타일), 4-12b 골든 98·99·100 |
| S1d | 정답은 무손실 PNG, 채점 입력은 PNG만 | 전체 | - | 1-4 PNG 왕복, 4-11b `gen_answer_pngs_and_meta...`·`score_rejects_a_jpeg_answer`, 3-10 스모크(wasm PNG 디코드) |
| S1f | sRGB→선형→XYZ D65→Bradford→Lab D50 | 전체 | - | 1-1 독립 기준값 |
| S1g | Sharma 34쌍 ≤1e-4 (양방향, wasm 포함) | 전체 | - | 1-1, 3-10 `color_vectors.rs` |
| S1h | 2048→1024 Lanczos3 규칙 고정 | 전체 | - | 1-3 독립 기준값, 4-12b 골든 |
| S1j | 상대 점수 식, `D_E<3` 부적격 | 전체(E=전체) | 마스크·크롭 영역 1b | 3-9a `relative_score`·`eligibility` |
| S2 | 진단 지표 5종 | 부분: 필드·nullable 계약, tone·color 임시 산식 | AC 산식, detail·local·composition 1b | 3-9b 직렬화 계약 |
| S3 | 원본 제출 0, 정답 제출 100 | 부분: 골든 케이스만 | 전 챌린지 CI Plan 3 | 3-9b, 4-11b, 4-12b 속성 검사 |
| S5a | 제출 렌더·채점 2초 | **기준선 측정만**(로드·제출·전환 분리, 객체 해제 후 메모리) | 엔진 측 게이트 1b 첫 태스크(마스터 §4.1), 기기 실측 Plan 2 | 4-13 `perf.mjs`, 4-14 `PERF.md` |
| 마스터 A1~A6, A10, §5.1~5.5 | 결정성·계약(lint, 누적 순서, `--locked`, 플래그 금지, 단일 `quantize`) | 전체(분할 1부터). 단 §5.3의 크레이트 버전 범프 강제는 §5.6 행(분할 4) | A7(Worker)·A9(GLSL) Plan 2 | 1-0 lint, 1-2 quantize, 각 분할 CI |
| 마스터 §5.6 | 골든 해시·Score JSON의 3-OS·wasm 일치, 버전 미범프 변경 거부(§5.3 크레이트 버전 포함) | 전체(**분할 4에서만**, 분할 1~3은 미포함) | - | 4-12a guard, 4-12b `golden check`, 4-13 wasm 골든, 4-14 CI |
| 마스터 A8 | 정답·골든을 빌드 타임 CLI로 생성해 `content/answers/`·`manifest.json`에 넣고 CI가 wasm으로 같은 해시를 검증 | 부분: CLI `gen-answer`·`golden write/check`와 골든의 wasm 대조 | `content/answers/`·`manifest.json` 생성과 그 CI 검증은 Plan 3(build-content) | 4-11b, 4-12b, 4-13 |

1a에서 빼는 것: S1e(UI 문구, Plan 3), S1i·S1k(평가 영역·누출, 1b), E1b·E1d·E1f, E4·E5·E7, C·H·G 계열.

---

## 리뷰 반영 기록

출처: 4회차 `consolidated-r4.md`(U). 3회차 `consolidated-r3.md`(T). 1회차 `.omc/state/plan1a-review/consolidated.md`(B·M·MINOR), Codex Critic(XC), Codex Architect(XA). 2회차 `consolidated-r2.md`(R·MINOR·D). "확인함"은 임시 작업 공간 실행으로 검증했다는 뜻이다. 위치는 `분할-태스크`.

### 4회차 (U1~U6)
출처: `consolidated-r4.md`(Codex Architect·Critic·일반 모두 APPROVE_WITH_CHANGES, BLOCKER 0, 지적은 전부 분할 4).

| ID | 반영 위치 | 내용 |
|----|-----------|------|
| U1 | 4-13, 4-14 CI | Task 13 커밋에 `cli_contract.test.mjs` 추가. CI "Golden test files present" 단계가 wasm 테스트 3개와 `verify-sources.test.mjs`의 존재를 검사(glob 단계가 파일 누락을 조용히 넘기지 않게) |
| U2 | 4-12a | 프로브 삭제·이름 변경도 `SCORING_VERSION`만 인정. 회귀 테스트: 삭제/이름 변경 + 엔진만 범프 → 거부, 채점 범프 → 통과 |
| U3 | 4-12a, 4-12b Step 6, 4-14 CI, 1 머리·Task 0 checkout 주석, 마스터 §5.3 | `crate_changes`: base/head `engine/Cargo.lock`에서 `PIXEL_CRATES`(`zune-jpeg`·`zune-core`·`png`·`fdeflate`·`miniz_oxide` → ENGINE, `libm` → ENGINE+SCORING) 버전 비교, 골든이 같아도 무범프면 거부. 단위 테스트 5개(버전만 변경·골든 동일·무범프 FAIL 포함), CI 가드가 base lockfile 전달. 실제 lockfile 변조로 거부 확인함 |
| U4 | 4-13 Step 3 | RED의 `sed -i`를 Node 치환으로(macOS 호환) |
| U5 | 4-12b | `verify-sources`가 generator 커밋 존재(`git cat-file`)와 그 커밋의 `engine/Cargo.lock` SHA-256(`git show <commit>:engine/Cargo.lock`)을 검증. 채우는 스크립트도 커밋된 lockfile 바이트를 해시. CI는 `fetch-depth: 0`, 분할 4 PR은 merge commit 병합(squash 금지) |
| U6 | 4-12b Step 1, 이 문서 TDD 예외 | `verify-sources.mjs`를 코드 단계로 분리: `engine/golden/test/verify-sources.test.mjs` 7개(정상 PASS, 누락 파일·잘못된 해시·미기입·없는 커밋·lockfile 불일치·원본 SHA-1 FAIL), 스텁 RED → GREEN. 예외는 데이터 생성만 |

### 3회차 (T1~T20)
출처: `consolidated-r3.md`(Codex Architect REJECT, Critic REJECT, 일반 APPROVE_WITH_CHANGES, BLOCKER 0).

| ID | 반영 위치 | 내용 |
|----|-----------|------|
| T1 | 1-1, 1-2, 1-3, 1-4 | GREEN "같은 명령"을 RED와 같은 literal `cargo test --locked … -p engine-core <모듈>::` 명령으로 교체 |
| T2 | 1 머리 "커버 AC"·"이 분할이 끝내지 않는 것", 인덱스 분할 간 인수 조건·커버리지 표 | 분할 1은 §5.1~5.5만 담당, §5.6 골든·버전 가드(크레이트 버전 범프 강제 포함)는 분할 4. 분할 1~3 병합 시점의 `main`은 골든 미보호임을 명시 |
| T3 | 2-5c Step 3 (1) | import **두 줄 전체** 교체로 지시 정정(E0252 방지). 5b 스냅숏에 5c 지시를 문자 그대로 적용해 fmt·clippy(-D warnings)·`cargo test --workspace`(core 97) 통과 확인함 |
| T4 | 2-5a, 2-6 | GREEN literal 명령(`recipe::`, `render::basic`) |
| T5 | 3-10 | wasm32에서 `RegionSpec::rasterize`·`RegionMask::resample_to`·`error_stats`·`ScoringReference::new` 오류 경로를 직접 호출하는 `core_errors.rs`(5개). JS `assert.throws`는 엔진 오류 메시지 정규식을 지정해 wasm trap이 통과하지 못함(확인함) |
| T6 | 3-10, 4-13, 마스터 §3.4 | `ImageData`는 Rust 메모리를 소유하는 클래스: `free()` 책임, `rgba` 읽기마다 복사, `answer_scoring()`·`memory_bytes()` API 명시. 스모크·골든·CLI 계약·벤치 JS가 모두 `try/finally`로 해제, 해제 확인 테스트 |
| T7 | 3-10 Step 2 | Task 10 RED를 패키지 부재가 아니라 `todo!()` 스텁 바인딩의 동작 실패로 변경(node 6개가 `RuntimeError: unreachable`로 실패, 확인함) |
| T8 | 3-9b | `extreme_basic_values_score_to_finite_numbers`: 기본 패널 극값 1024조합을 정답, 보수 조합을 플레이어로 채점해 Score 실수가 모두 유한(E3c 채점 쪽) |
| T9 | 4-12a | `Expected`가 과거 스냅숏을 현재 타입에 묶이지 않고 읽음: `scoring_params`는 무타입 `Params`(숫자는 값 비교, JavaScript 왕복의 `2.0`→`2` 오탐을 검증 중 발견해 수정), 새 필드는 `serde(default)`. 매개변수 추가·삭제 × 범프 유무 테스트 |
| T10 | 4-12a, 4-12b, 4-13 | 플레이어(예약 `answer`·`original` 포함)마다 `render_sha256`·`scoring_sha256`·`score_json` 저장, native debug·release·wasm 비교. 플레이어 렌더 해시 변경은 `ENGINE_VERSION` 범프 필요 |
| T11 | 4-12a, 4-12b | 정수 공식으로 만든 채점 프로브 4개(만점·타일 게이트 99·중간·부적격). 프로브 결과 변경은 `SCORING_VERSION`만 인정(버전 문자열 제외 비교). 실제 변이 확인: `provisional_diag` 반올림 변경 + 엔진만 범프 → `golden write` 거부, 채점 범프 → 통과 |
| T12 | 4-12a | golden write/check CLI 통합 테스트 5개: 정상 생성·비교, 미범프 거부와 원본 바이트 보존, 불일치 보고, 계산 실패 보고(파일 미변경), 판독 불가 `expected.json` 보고 |
| T13 | 4-11b, 4-13 | CLI가 2048×1365로 `gen-answer`·`render`·`score`를 실행해 `engine/target/cli-contract`에 남기고, `cli_contract.test.mjs`가 디코드 해시·정답 렌더·2048→1024 재축소·플레이어 렌더·Score 문자열을 대조 |
| T14 | 4-12a, 4-13 | 실패 보고서 `{"failure":{case,player,stage,error}}`(계산·`expected.json` 판독 포함), 불일치는 재계산 결과 전체 + stderr에 불일치 JSON 경로 |
| T15 | 4-12b | 다운로드·가공 블록을 `bash -euo pipefail` heredoc으로(호출 셸이 `&&` 목록 안이면 `( set -e … )`가 무시됨을 확인). `sources.json` `generator`에 생성 커밋과 `Cargo.lock` SHA-256, 미기입이면 `verify-sources` 실패 |
| T16 | 4-13, 4-14, 마스터 §4.1 | 벤치를 로드(원본 JPEG·정답 PNG 디코드 → 참조), 참조 재사용 제출, 전환으로 분리하고 객체를 모두 해제해 재측정: 로드 p50 1031ms, 제출 p50 1767ms, 메모리 140.8MiB, 전환 증가 0. 같은 날 이전 스크립트는 1786ms·223.5MiB |
| T17 | 4-12b, 4-13, 4-14, 이 문서 TDD 예외 | 12b는 데이터 태스크 예외 근거 명시, 13은 변조 복구 후 재실행으로 PASS 확인, `TMP=$(mktemp -d)`를 태스크별로 생성 |
| T18 | 3-8, 3-9a, 4-11a, 4-11b, 4-12a | GREEN "같은 명령"을 literal 명령으로 교체 |
| T19 | 마스터 §4.1(신설)·§4 1b 행·§7, 이 문서 seam 노트·사용자 결정 1 | 1b 성능 게이트: 데스크톱 1 + 중급 모바일 1(Galaxy A54 Chrome) Worker 측정 경로, 로드·제출·반복 전환 분리, 예산(제안값)·중단 조건(누수·탭 크래시 포함), 최대 복잡도 완성 후 재검증. 1a 기준선은 잠정 신호 |
| T20 | 커버리지 표 E1c·A8·§5 행 | E1c 완료 조건을 AC 원문으로 인용하고 1a 부분(디코드·RGBA8 해시·형식 거부)을 분리. A8은 CLI/골든 부분만 1a, `content/answers`·manifest는 Plan 3. §5를 5.1~5.5와 5.6으로 분리 |

### 2회차 (R1~R9, MINOR, D1~D4)
| ID | 반영 위치 | 내용 |
|----|-----------|------|
| D1 | 2-7, 3-9b, 3-10, 인덱스 계약 | 렌더는 `crop` 미적용·출력=입력 크기(`crop_never_changes_the_scoring_render`), `CompositionInput`(1a는 상자 없음만), wasm `score(player, crop_json|null)` |
| D2 | 3-8, 3-9b, 3-10 | `ScoringReference::new(원본 2048, 정답 2048, region)`: 2048에서 래스터화 → `resample_to` → 채점 이미지·Lab 캐시. wasm `ScoringReference` 클래스(constructor/score/free), `score_rgba8` 제거 |
| D3 | 4-13, 4-14, seam 노트 | 워밍업 1회 + 7회 p50/p95, wasm 메모리 피크(`memory_bytes`), 기준선만 기록. 측정(당시, 2회차): 제출 p50 2038ms·p95 2100ms, 메모리 223.5MiB(MSVC 빌드 wasm, i7-10700K). 3회차 T16에서 해제 누락을 고쳐 재측정: 제출 p50 1767ms, 로드 p50 1031ms, 메모리 140.8MiB |
| D4 | 이 문서 + 4개 분할 | 분할별 헤더·인수 조건·브랜치/PR, CI 점진 확장, 중복 코드 단일화(validate는 5c에 추가분만, CLI는 11b에 추가분만) |
| R1 | 2-6, 2-7 | 생동감 가중치 `clamp(0,1)`, 단위 테스트(과대 범위 픽셀) + 렌더 테스트(빨강+노출5+생동감100 → 빨강 유지, R=255) |
| R2 | 4-12a | guard 재설계: `scoring_params`·`long_edge`는 `SCORING_VERSION` 단독, 이미지·케이스·플레이어 삭제·이름 변경 검출, 렌더 변경은 엔진 범프만 인정. 단위 테스트 12개 |
| R3 | 4-14 CI | PR base → push `before` → zero SHA면 `merge-base origin/main`. base 커밋 조회 실패는 오류, base에 `expected.json`이 없을 때만 "최초 도입"으로 건너뜀 |
| R4 | 전 분할, 1-0 lint, CI | 모든 cargo·`cargo tree` `--locked`. **wasm-pack 0.15.0 소스 확인**: `wasm-pack test`는 크레이트 경로 뒤 인자를 `cargo test`에 그대로 넘기고(`src/test/mod.rs` `cmd.args(extra_options)`), `--` 뒤 인자는 테스트 러너로 간다. 따라서 `wasm-pack test --node engine/wasm --locked`가 맞다(실행 확인). 1차의 "붙이지 않는다" 판단과 Codex의 "`-- --locked`" 주장은 둘 다 틀렸다. CI 끝 `git diff --exit-code -- engine/Cargo.lock` |
| R5 | 3-8, 3-9a | `rasterize`·`resample_to`가 `checked_len` + `Result`, `error_stats`가 길이 불일치를 `StatsError`로. 0 크기·`u32::MAX²` 테스트, wasm 0 크기 생성자 테스트 |
| R6 | 3-9b, 4-12b | `score()` 경로에서 세 조건 각각 단독 실패(나머지 둘 통과를 단언), 마지막 부분 타일 → 99. 골든 `lake_warm`에 98·99·100 경계 플레이어(네이티브 release·wasm 일치 확인함) |
| R7 | 4-11a, 4-11b, 3-10 | 2048×1365 `gen-answer` → `answer_2048/1024.png`·`meta.json` 해시 → 재축소 일치, `prep-photo`·`scoring-image`·`hash` 계약 테스트, 스모크가 정답을 PNG로 저장해 wasm에서 디코드·채점 일치 |
| R8 | 공통 계약 "TDD 규칙과 예외", 1-2, 1-0, 4-13, 4-14 | `pixels()` 스텁은 `std::iter::empty()`(E0277 확인함), Task 0·13·14의 RED 단계 명시 |
| R9 | 인덱스, 마스터 §4(코디네이터 갱신) | 커버리지 표 정정, A2 문구(마스터), Score 숫자 포맷 = serde_json, `versions().scoring_params` 객체, 완료된 MSRV 요청 삭제, 태스크 참조 재생성 |
| MINOR 실패 보고서 | 4-12a, 4-13 | `golden check`는 계산 예외도 `--actual-out`에 기록, wasm 테스트는 계산 전체를 try 안에 |
| MINOR curl·해시 | 4-12b | `curl --fail --location --show-error` + `verify-sources.mjs`(SHA-1·SHA-256 자동 비교), `sources.json`에 리비전 시각·SHA-256·전처리 명령 |
| MINOR PR 체크 | 각 분할 마지막 스텝 | `gh pr checks --watch --required` |
| MINOR CI 순서 | 3-10 CI | setup-node를 wasm-pack test 앞으로, wasm32 clippy 단계 |
| MINOR libm 설명 | 1-0 lint 주석 | `force-soft-floats`가 `arch`·`intrinsics` cfg를 모두 끈다(libm 0.2.16 `configure.rs` 확인) |
| MINOR sRGB 음수 | 1-1 | 부호 대칭(W3C)으로 변경·명시, 테스트 |
| MINOR JS 제목 | 3-10 | `validate_recipe` 테스트가 실제 `masks[0].adjust.tint` 경로를 검증 |
| MINOR Task 8 명령 | 3-8 | 리터럴 명령 두 줄 |
| MINOR 커밋 분리 | 4-12a/12b | 골든 도구(+guard 테스트)와 골든 데이터 커밋 분리 |

### 1회차 (B·M·MINOR, XC, XA)
| ID | 반영 위치 | 내용 |
|----|-----------|------|
| B1 | 1-3, 2-7, 3-9b, 3-10, 4-12b, 4-13 | `scoring_image` 단일 함수, 양자화 2지점(사용자 결정) |
| B2 | 4-11a `encode_jpeg` | `jpeg-encoder` `ColorType::Rgba`(알파 무시), `image` 의존 제거 |
| B3 | 4-12b | 중간 점수 플레이어 + 경계 98·99·100, Score JSON 문자열 비교, 비정수 배율 |
| B4 | 1-0 | pedantic 허용 목록(근거 주석), 태스크마다 fmt·clippy, 툴체인 고정, rust-version 1.87 |
| M1 | 1-0, 1-4, 4-12b | zune-jpeg SIMD 끔, libm `force-soft-floats`, lint가 `cargo tree --locked -e features,normal`로 단언, CC0 실사 3장 |
| M2 | 2-5a | §3.1 전 필드, 마스터 예제 계약 테스트 |
| M3 | 2-5a, 2-7 | `Normalized` 비공개 필드, `RenderContext{seed}` |
| M4 | 3-8, 3-9b | §3.3 필드, `RegionSpec` 래스터화 |
| M5 | 1-1, 2-6, 2-7 | 비클램프 파이프라인, 하이라이트 비례식, 복구 테스트 |
| M6 | 1-3 | 샘플 인덱스 클램프, 독립 기대값 |
| M7 | 2-5a~5c, 3-10 | `Normalization{path,from,to,reason}`, `masks[i]` 경로, `validate_recipe` |
| M8, M9 | 1-2 | 8×8 라운드트립, `checked_len`·`MAX_PIXELS` |
| M10 | 공통 계약, 3-10 | TDD 단계, 골든 전 스모크 |
| M11 | 3-9a, 3-9b | 조건별 단독 실패 + 경계값 |
| M12 | 커버리지 표 | 열 분리, 부분·후속 명시 |
| M13 | 1-4 | 16bit·반투명 PNG, CMYK 거부, 1×1·3000×1 |
| M14 | 4-12a, 4-14 | `engine-cli golden`, 원자 교체, 실패 보고서 업로드 |
| MINOR(1회차) | 각 위치 | UFCS·FMA·RUSTFLAGS lint, 산출물 이름, `meta.json` 비결정 값 제거, `.gitignore`, 행 우선 수직 패스, p95 규칙, glob, wasm-pack 고정, 브랜치+PR, 색 과학 테스트 보강 |
| XC-1~13 | 커버리지 표, 4-11b, 3-9a, 4-12a/b, 2-5c, 2-5a·5b, 1-4, 전 명령, 1-4 Step 5, 분할 PR 스텝, 태스크 분할 | 1회차 기록과 동일(태스크 번호는 현재 번호로 갱신) |
| Codex Critic #1(마지막 1회 양자화) | - | **사용자 결정으로 기각**(2지점 양자화) |
| XA-1~8 | 1-0, 1-0, 2-7·3-10·4-11, 4-11b, 3-9b, 4-11b, 1-1, 인덱스 계약 | 1회차 기록과 동일 |

---

## 사용자 결정·확인이 필요한 사항

1. **AC-S5a 위험(1b 게이트 입력)**: 데스크톱 node wasm 제출 p50 1767ms(해제 정리 후 재측정, 2회차 당시 기록 2038ms는 측정 환경 차이). 목표 2초는 모바일 기준이고 모바일은 아직 재지 않았다. 1a는 기준선만 남기고, 결론은 1b 첫 태스크의 게이트(마스터 §4.1)가 낸다. §4.1의 예산 수치(모바일 제출 p95 ≤ 2.0초, 로드 p95 ≤ 3.0초, 피크 ≤ 300MiB, 전환 10회 후 증가 ≤ 10%)는 제안값이므로 1b 게이트 태스크 승인 때 확정이 필요하다.
2. **`T_TILE = 2.0`은 임시값**이다. 경계 골든(98·99·100)은 이 값에 의존하므로 Plan 3 벤치에서 바뀌면 `SCORING_VERSION` 범프와 골든 재생성이 따른다.

## 실행 인수인계

각 분할은 **승인 대기**다. 분할 1부터 순서대로 승인·실행한다. 실행 방식은 분할마다 고른다.

1. **Subagent-Driven (권장)**: 태스크마다 새 서브에이전트, 사이사이 리뷰(`superpowers:subagent-driven-development`). 결정성 실수는 골든 전체를 무효화하므로 태스크별 게이트가 값싸다.
2. **Inline**: 이 세션에서 체크포인트마다 확인하며 순차 실행(`superpowers:executing-plans`).
