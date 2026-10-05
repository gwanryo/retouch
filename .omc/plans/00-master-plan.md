# 사진 보정 따라잡기 게임 — 마스터 구현 계획

> **For agentic workers:** 이 문서는 아키텍처·인터페이스·로드맵을 고정하는 마스터 플랜이다. 실제 작업 단계는 하위 계획(`01-engine-foundation.md` 등)에 있으며, 각 하위 계획은 superpowers:subagent-driven-development 또는 superpowers:executing-plans 로 실행한다.

**Goal:** 스펙 v2(`.omc/specs/deep-interview-photo-edit-match-game.md`)의 인수 기준 ~70개를 4개의 하위 계획으로 나누어, 각 계획이 단독으로 테스트 가능한 소프트웨어를 산출하도록 한다.

**Architecture:** 결정성의 유일한 기준인 **참조 엔진(Rust → WASM + 네이티브 CLI)** 이 JPEG 디코드·렌더·축소·채점을 모두 담당하고, **웹 앱(Vite + React + TS)** 은 조작 중 GPU(WebGL2) 프리뷰와 UI·게임 루프·공유를 담당한다. 정답 이미지·골든 해시는 같은 엔진의 네이티브 빌드로 빌드 타임에 생성해 정적 매니페스트에 넣는다. 서버는 없다.

**Tech Stack:** Rust 1.98 고정(`rust-toolchain.toml`, MSRV 1.87 - jpeg-encoder 0.7 요구), `wasm-bindgen`/`wasm-pack`, `libm`(순수 Rust 초월함수), `zune-jpeg`(JPEG 디코드, SIMD 끔)·`png`·`jpeg-encoder`(픽스처), `sha2`, `serde_json`, `clap` / Node 24, Vite 6, React 19, TypeScript 5, Zustand, Tailwind, WebGL2, Vitest, Playwright / GitHub Actions(ubuntu·windows·macos 매트릭스) / 정적 호스팅(Cloudflare Pages 또는 Vercel).

---

## 1. 저장소 구조 (모노레포)

```
afterglow/                          # 이름 변경 예정 (Tonematch/Lumatch)
├─ .omc/                            # 스펙·계획·상태 (코드 아님)
├─ engine/                          # Rust 워크스페이스 — 참조 엔진
│  ├─ Cargo.toml                    # [workspace] members = core, cli, wasm
│  ├─ Cargo.lock                    # 커밋 (디코더 버전 고정 = 결정성)
│  ├─ core/                         # 라이브러리 크레이트 `engine-core`
│  │  └─ src/
│  │     ├─ lib.rs
│  │     ├─ color.rs                # sRGB↔linear, Lab D50(Bradford), ΔE00
│  │     ├─ buffer.rs               # ImageF32, RGBA8 변환·반올림 규칙(유일한 quantize)
│  │     ├─ decode.rs               # JPEG(zune-jpeg 스칼라)/PNG 디코드, 지원 외 형식 거부
│  │     ├─ resample.rs             # 선형광 Lanczos3 축소
│  │     ├─ recipe.rs               # Recipe 스키마·검증·정규화
│  │     ├─ render/
│  │     │  ├─ mod.rs               # 파이프라인 순서 고정
│  │     │  ├─ basic.rs             # 노출·WB·톤·생동감/채도 (Plan 1a)
│  │     │  ├─ hsl.rs               # HSL 8밴드 (Plan 1b)
│  │     │  ├─ curve.rs             # 톤커브 (Plan 1b)
│  │     │  ├─ grade.rs             # 컬러그레이딩·스플릿토닝 (Plan 1b)
│  │     │  ├─ mask.rs              # 선형·방사형·범위 마스크 평가 (Plan 1b)
│  │     │  ├─ local.rs             # 마스크별 효과 합성 (Plan 1b)
│  │     │  └─ detail.rs            # 샤프닝·그레인·비네팅 (Plan 1b)
│  │     ├─ region.rs               # 평가 영역 E 래스터화 (1a: full, 1b: masks/crop)
│  │     ├─ score.rs                # 상대 점수·3중 만점·진단 지표
│  │     ├─ hash.rs                 # SHA-256
│  │     └─ xmp.rs                  # crs XMP ↔ Recipe (Plan 1b)
│  ├─ cli/                          # 바이너리 `engine-cli`: fixture / render / scoring-image / gen-answer / score / hash / golden / verify-challenge
│  ├─ wasm/                         # cdylib `engine-wasm`: decode / validate / render / scoring_image / score / hash
│  │  └─ test/                      # node --test: WASM 출력 == CLI 골든 해시
│  └─ golden/                       # 골든 레시피·해시 (CI가 검증)
├─ web/                             # Vite + React + TS 앱 (Plan 2, 3)
│  ├─ src/
│  │  ├─ engine/                    # WASM 로더·워커·타입
│  │  ├─ preview/                   # WebGL2 셰이더 프리뷰 (참조와 동일 순서·수식)
│  │  ├─ editor/                    # 패널·슬라이더·마스크 도구·크롭·실행취소
│  │  ├─ game/                      # 홈 그리드·플레이·결과·해설
│  │  ├─ share/                     # 카드 PNG·Web Share·폴백
│  │  └─ storage/                   # localStorage 베스트·초안·버전
│  └─ e2e/                          # Playwright (모바일 에뮬레이션 포함)
├─ content/                         # 챌린지 원본·레시피·매니페스트 (Plan 3)
│  ├─ originals/{id}.jpg            # 2048px sRGB JPEG q90 (EXIF 적용 후)
│  ├─ recipes/{id}.json
│  ├─ answers/{id}/                 # gen-answer 출력: answer_2048.png, answer_1024.png, meta.json
│  └─ manifest.json                 # 빌드 산출물
├─ scripts/                         # build-content.mjs, lint-determinism.sh
└─ .github/workflows/ci.yml         # cargo test + wasm-pack + node test (3 OS) + web test
```

## 2. 핵심 아키텍처 결정

| # | 결정 | 근거 (스펙/토론) |
|---|------|-----------------|
| A1 | **참조 엔진 = Rust, 동일 소스로 네이티브 CLI와 wasm32 빌드.** 초월함수는 전부 `libm` 크레이트(순수 Rust) 경유, `f32::powf` 등 std 수학 함수 사용 금지(CI grep 린트). `mul_add`·`target-cpu=native`·fast-math 금지. | AC-E1a/E1e. Rust는 fast-math가 없고 LLVM이 FMA를 자동 융합하지 않으므로 IEEE 결정성 확보 가능 |
| A2 | **JPEG 디코드도 참조 엔진 안에서**(`zune-jpeg` 직접 의존 `default-features=false`로 SIMD 경로 제거 = 모든 타깃 스칼라, PNG는 `png` 크레이트, Cargo.lock 고정). 8bit RGB/불투명 RGBA 외(16bit PNG, 반투명 PNG, CMYK)는 거부. 브라우저 `<img>`/canvas 디코드 픽셀은 참조 경로에 절대 넣지 않는다. 프리뷰는 브라우저 디코드 허용. | 브라우저별 JPEG IDCT 차이(±1)가 AC-E1a를 깨뜨림. **스펙 v2.1 추가 제안: AC-E1g** |
| A3 | 렌더는 f32이며 **렌더 중간에는 클램프하지 않는다**(노출로 1을 넘은 값도 하이라이트 조절까지 보존). 채점의 Lab·ΔE00은 f64(libm). 8bit 양자화 `floor(clamp(v)*255+0.5)`는 `buffer::quantize` 한 함수뿐이며, 정확히 두 지점에서만 호출: ① 렌더 최종 출력(2048, = 정답 PNG) ② 채점 이미지 생성(1024). | AC-E1e(렌더 내부는 f32, 최종 출력만 양자화), AC-S1g |
| A4 | 채점 이미지 = `core::scoring_image(&Rgba8, 1024)` 단일 함수: 2048 RGBA8 → dequantize(선형) → Lanczos3(a=3, 픽셀 중심 i+0.5, 커널 지지 구간 전체 순회 + **샘플 인덱스만** clamp-to-edge, 가중치 정규화, 오름차순 누적, 수평→수직 모두 행 우선) → quantize → 1024 RGBA8. CLI·골든·wasm 모두 이 함수만 쓰며, 저장된 정답 PNG만으로 채점 입력을 재현할 수 있다. | AC-S1h |
| A5 | 평가 영역 E = 픽셀 마스크(u8 알파) 하나로 통일하되, **래스터화는 core(`region.rs`)가 영역 명세 JSON으로부터 수행**한다(호출자가 마스크 픽셀을 넘기지 않음 → 네이티브·wasm 동일 코드). E는 **챌린지 로드 시 `ScoringReference::new(원본 2048, 정답 2048, region)`에서 한 번** 만든다: 2048 좌표에서 기하·범위(보정 전 원본 선형값) 판정 → 임계(α≥0.5)·페더 확장 → 채점 좌표(1024)로 변환 → 캐시. 원본·정답의 채점 이미지·Lab도 이때 사전 계산. 글로벌 E=전체, 크롭 E=정답 크롭, 로컬 E=마스크 래스터. 채점 코드는 E만 본다. 1a는 `{"kind":"full"}`만. |
| A10 | **채점 렌더는 항상 원본 전체 프레임.** `render`는 레시피의 `crop`을 적용하지 않고(출력 크기 = 입력 크기), 크롭은 표시·다운로드·공유용 별도 연산 `crop_rgba8`이다. 보정 점수는 E(크롭 챌린지는 정답 크롭 영역) 안에서만 비교하고, 구도 점수(IoU)는 manifest의 정답 크롭 박스와 플레이어 크롭 박스를 score 입력으로 받아 계산한다. 정답·플레이어 픽셀은 항상 같은 좌표로 대응한다. | AC-S1i, AC-S2b, AC-E7, D12 | AC-S1i~k, D14 |
| A6 | 버전 3종을 모든 산출물에 기록: `schema_version`(레시피), `ENGINE_VERSION`(렌더), `SCORING_VERSION`(채점, `T_TILE` 포함). | AC-S1b, AC-G3b |
| A7 | 웹 앱은 WASM을 **Web Worker**에서 실행(제출 렌더·채점 2초 예산이 UI를 막지 않도록). 프리뷰는 메인 스레드 WebGL2. | AC-S5a, AC-E2a |
| A8 | 정답·골든은 **빌드 타임 CLI**로 생성해 `content/answers/`와 `manifest.json`에 넣고, CI가 wasm 빌드로 같은 해시가 나오는지 검증한다. 런타임에 정답을 재생성하지 않는다(모바일 2초 예산 보호). | AC-E1a/E1f, D3 |
| A9 | 프리뷰 셰이더는 참조 엔진과 **같은 순서·같은 수식**을 GLSL로 옮긴 것이며, 점수 계약 밖. 중간 렌더 타깃은 RGBA16F(`EXT_color_buffer_float`) 이상으로 A3의 비클램프 중간값을 보존하고, 최종 출력에서만 클램프. 입력 종료 500ms 후 워커의 참조 렌더로 교체. | AC-E1b |

## 3. 인터페이스 (하위 계획 전부가 공유하는 계약)

### 3.1 Recipe JSON (schema_version 1)
```jsonc
{
  "schema_version": 1,
  "basic": {                       // crs 호환 이름·범위
    "exposure": 0.0,               // Exposure2012, EV, -5..5
    "contrast": 0, "highlights": 0, "shadows": 0, "whites": 0, "blacks": 0,   // -100..100
    "temperature": 0, "tint": 0,   // SDR JPEG 상대값 -100..100 (AC-E4c)
    "vibrance": 0, "saturation": 0, // -100..100
    "texture": 0, "clarity": 0, "dehaze": 0                                    // -100..100 (Plan 1b)
  },
  "hsl": { "hue": [0,0,0,0,0,0,0,0], "saturation": [..8], "luminance": [..8] }, // R,O,Y,G,A,B,P,M (Plan 1b)
  "tone_curve": { "master": [[0,0],[255,255]], "red": [...], "green": [...], "blue": [...] }, // ≤16점/채널 (Plan 1b)
  "color_grade": { "shadows": {"hue":0,"sat":0,"lum":0}, "midtones": {...}, "highlights": {...}, "global": {...}, "blending": 50, "balance": 0 }, // Plan 1b
  "split_toning": { "shadow_hue": 0, "shadow_sat": 0, "highlight_hue": 0, "highlight_sat": 0, "balance": 0 }, // Plan 1b
  "detail": { "sharpen_amount": 0, "sharpen_radius": 1.0, "sharpen_detail": 25, "sharpen_masking": 0,
              "grain_amount": 0, "grain_size": 25, "grain_frequency": 50,
              "vignette_amount": 0, "vignette_midpoint": 50, "vignette_feather": 50 },                  // Plan 1b
  "masks": [                        // ≤3 (AC-E5a). 좌표는 원본 정규화 [0,1]
    { "kind": "linear", "x0": 0.5, "y0": 0.0, "x1": 0.5, "y1": 1.0, "feather": 0.5, "invert": false,
      "adjust": { "exposure": 0, "contrast": 0, "highlights": 0, "shadows": 0, "whites": 0, "blacks": 0,
                  "temperature": 0, "tint": 0, "saturation": 0, "sharpen_amount": 0, "clarity": 0 } },   // 11종 (AC-E5b)
    { "kind": "radial", "cx": 0.5, "cy": 0.5, "rx": 0.3, "ry": 0.2, "feather": 0.5, "invert": false, "adjust": {...} },
    { "kind": "range", "channel": "luminance" | "hue", "lo": 0.0, "hi": 0.5, "smooth": 0.1, "invert": false, "adjust": {...} }
  ],
  "crop": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 } // 축 정렬, 면적 0.25..1.0 (AC-E7a/b), null 허용
}
```
검증 규칙(AC-E3c/d): 비유한 수 → 거부(에러). 범위 밖 → 클램프 후 `normalizations[]`에 `{path, from, to, reason}`로 기록(마스크는 `masks[0].adjust.exposure`처럼 인덱스 포함). 마스크 > 3, 커브 점 > 16, 크롭 면적 < 0.25, 범위 마스크 lo > hi → 거부. 알 수 없는 키는 거부(`deny_unknown_fields`), 위 필드는 **전부 1a 스키마에 선언**(기본값 = 무효과)하고 렌더 미구현 기능이 기본값이 아니면 `NotYetSupported` 에러. 렌더는 검증된 `Normalized` 타입만 받는다.

### 3.2 챌린지 매니페스트 (`content/manifest.json`, 빌드 산출물)
```jsonc
{
  "manifest_version": 1,
  "engine_version": "engine-0.1.0", "scoring_version": "scoring-0.1.0",
  "challenges": [{
    "id": "c001", "title": "...", "type": "global" | "local" | "composition",
    "difficulty": "easy" | "normal" | "hard", "technique": "...", "recommended_tools": ["basic.exposure", "masks.radial"],
    "original": { "url": "originals/c001.jpg", "width": 2048, "height": 1365, "sha256_file": "...", "sha256_rgba8": "...",
                  "license": { "source_url": "...", "author": "...", "license": "CC0" } },
    "answer": { "url_2048": "answers/c001/answer_2048.png", "sha256_rgba8_1024": "...", "d_e_original": 7.3 },
    "seed": 123456789,                            // u32, 그레인 등 결정적 노이즈 시드(AC-E1d). 렌더 API에 그대로 전달
    "region": { "kind": "full" },                 // 평가 영역 명세(A5). 1b: {"kind":"masks","masks":[기하만, adjust 없음]} | {"kind":"crop",...}. 정답 레시피와 분리 → 플레이 중 레시피 비로드 원칙 유지
    "recipe_url": "recipes/c001.json",           // 결과 화면 해설용 (플레이 중 로드 금지)
    "explanation": { "observation": "...", "intent": "...", "region": "...", "effect": "...", "sources": ["..."] },
    "eligibility": { "type": "global", "mean_de_full": 7.3, "passed": true }     // verify-challenge 출력
  }]
}
```

### 3.3 Score JSON (엔진 출력, 결과 화면·카드 입력)
```jsonc
{ "correction_score": 87, "raw_score": 87.42, "perfect": false, "eligible": true,   // eligible = D_E >= 3 (AC-S1j)
  "d_e": 1.75, "d_e_original": 7.3, "mean_de": 1.75, "tile_p95_max": 4.1, "t_tile": 3.0, "big_error_ratio": 0.004, "leakage_penalty": 0.0,
  "composition_score": null,
  "diagnostics": { "tone": 91, "color": 84, "detail": null, "local": null, "composition": null },
  // diagnostics 각 항목은 number | null(평가 없음). 1a는 tone·color를 임시 산식(평균 |ΔL|, 평균 Δab)으로 채우고 AC-S2 산식은 1b에서 교체(SCORING_VERSION 범프).
  "top_error_regions": [ { "x": 0.6, "y": 0.1, "w": 0.2, "h": 0.2, "mean_dl": -3.1, "mean_da": 0.4, "mean_db": 1.2 } ],
  "engine_version": "engine-0.1.0", "scoring_version": "scoring-0.1.0" }   // 1a는 top_error_regions: []
```
타일 p95는 nearest-rank(`idx = ceil(0.95·n) − 1`, 오름차순 정렬). f64 값은 JSON에 `serde_json` 기본 포맷(최단 왕복 표현)으로 쓰며 골든은 문자열 완전 일치로 비교.

### 3.4 WASM API (`engine-wasm`)
```ts
decode_image(bytes: Uint8Array): ImageData        // JPEG/PNG, 참조 디코더. 받은 쪽이 free()
validate_recipe(recipe_json: string): string        // {"recipe":{정규화된 레시피},"normalizations":[{path,from,to,reason}]} 또는 throw
render_rgba8(width, height, rgba: Uint8Array, recipe_json: string, seed: number /*u32*/): Uint8Array  // 인코딩 sRGB RGBA8, 렌더 최종 양자화. 항상 전체 프레임(출력 크기 = 입력 크기, crop 미적용, A10)
crop_rgba8(width, height, rgba, crop_json: string): ImageData                         // 표시·다운로드·공유용 크롭 (1b). 채점 경로에서 사용 금지
scoring_image(width, height, rgba, long_edge: number): ImageData                         // A4 단일 함수 (구 downscale_rgba8). 받은 쪽이 free()
class ScoringReference {                                                                 // 챌린지 로드 시 1회 생성·캐시 (A5)
  constructor(width, height, original_2048: Uint8Array, answer_2048: Uint8Array, region_json: string)  // region_json = manifest.region
  score(player_2048: Uint8Array, crop_json: string | null): string                      // 플레이어 2048 렌더 → scoring_image → Score JSON. crop_json = {"answer":box|null,"player":box|null} (구도, 1b). 1a는 null
  answer_scoring(): ImageData                                                            // 정답 채점 이미지(해시 = manifest answer.sha256_rgba8_1024). 호출마다 새 복사본
  free(): void
}
class ImageData {                                                                        // Rust 메모리를 소유하는 wasm-bindgen 클래스 (일반 객체 아님)
  readonly width: number
  readonly height: number
  readonly rgba: Uint8Array                                                              // 읽을 때마다 새 복사본 (getter_with_clone)
  free(): void
}
sha256_hex(bytes: Uint8Array): string
versions(): string   // {"engine":"...","scoring":"...","schema":1,"scoring_params":{"t_tile":...,...}}
memory_bytes(): number   // wasm 선형 메모리 크기(성능 진단 전용, 채점 계약 아님)
```

**소유권 규칙.** `ImageData`와 `ScoringReference`는 Rust 메모리를 소유하므로 받은 쪽이 `free()`한다. 동기 루프 안에서는 finalizer가 돌지 않으므로 GC에 기대지 않는다. `rgba`는 읽을 때마다 복사되므로 한 번 읽어 보관하고 곧바로 `free()`한다. `decode_image`·`scoring_image`·`answer_scoring()`은 호출마다 새 `ImageData`를 돌려준다. `ScoringReference`는 입력 배열을 복사해 두므로 생성 뒤 JS 배열을 버려도 되고, 챌린지를 떠날 때 `free()`한다. Worker 경계(Plan 2, A7)에서는 `{width, height, rgba}` 일반 객체로 바꿔 넘긴다.

### 3.5 URL / 저장
- 라우트: `/` 홈, `/c/{id}` 챌린지, `/c/{id}/result` 결과. 정적 호스팅 SPA fallback 필수(AC-H4a).
- localStorage 키: `best:{id}` → `{score, composition, engine, scoring, at}`, `draft:{id}` → `{recipe, engine, at}`.

## 4. 로드맵 (하위 계획)

| 계획 | 파일 | 범위 | 커버 AC | 산출물(단독 테스트 가능) |
|------|------|------|---------|--------------------------|
| **1a 엔진 기반** (4분할, 각각 승인·PR) | `01-engine-foundation.md`(인덱스·공통 계약) + `01a-1-numeric-image.md`(수치·이미지 기반) · `01a-2-recipe-render.md`(레시피·렌더) · `01a-3-region-score-wasm.md`(영역·채점·WASM) · `01a-4-cli-golden-ci.md`(CLI·골든·CI) | 툴체인, 색 과학, 이미지·리샘플, 레시피 스키마(전체 선언), basic 패널 렌더, 채점 핵심(E=전체), CLI, WASM 바인딩, 골든·CI 3-OS, 성능 기준선 | E1a(부분: 실기기 제외), E1c(부분: EXIF·ICC는 3), E1e, E3c/d(부분: UI·XMP 제외), S1a/b/c/d/f/g/h, S1j(E=전체), S2(필드·임시 산식만, AC 산식은 1b), S3(골든 케이스만, 전 챌린지는 3), S5a(기준선 측정만) | `engine-cli`로 정답 생성·채점 가능, WASM==CLI 해시·Score JSON 일치 CI |
| **1b 엔진 완성** | `02-engine-full.md` | **첫 태스크 = 성능 게이트**(기준: §4.1): 비트 동일 최적화(LUT·ScoringReference 사전 계산) + 워밍업·반복 p50/p95·메모리 측정, 예산 미달 시 기능 확장 전 중단하고 해상도·범위 재결정. 이후 HSL·커브·그레이딩·스플릿토닝·디테일(샤프·그레인·비네팅), 마스크 3종·로컬 효과·렌더 순서, 크롭·구도 점수, 평가 영역·누출 페널티, 진단 5종·상위 오차 영역, XMP 변환, verify-challenge, 경계 레시피 세트 | E1d/f, E3a/b, E4(변환기), E5a/b/d/e, E7a/b(데이터), S1i/k, **S2(톤·색 AC 산식 교체 포함)**, S2a~d, S5a(엔진 측), S6c, C2a~d, C6a | 스펙의 모든 렌더·채점 AC가 CLI+CI로 검증됨 |
| **2 에디터 웹앱** | `03-web-editor.md` | Vite/React 골격, WASM 워커, WebGL2 프리뷰(참조와 동일 수식), 패널·슬라이더·커브 UI, 마스크·크롭 도구(터치), 실행취소, 초안 저장, 작성 모드(레시피 저장/XMP), 성능 측정 | E1b, E2a/b, E3(UI), E4a~f(UI), E5c/f/g, E6a/b, E7/E7c, G3a/b, G4a/b | 챌린지 1개를 끝까지 편집·제출·채점 가능 |
| **3 콘텐츠·게임·공유** | `04-content-game-share.md` | 콘텐츠 파이프라인(build-content), 파일럿 3개 → 15~20개, 홈 그리드, 결과 화면(해설·관측 문장·비교 슬라이더), 카드 PNG·Web Share·폴백, SPA fallback·오류 처리, 벤치 세트·T_tile 확정, 배포 | C1~C6, S1e(UI 문구), S3(전 챌린지 CI), E1c(EXIF·ICC 정규화·파일 해시), S4, S6a/b, H1~H4b, G1, G2/G2a, G4 | 출시 가능한 v1 |

### 4.1 Plan 1b 첫 태스크: 성능 게이트 기준

아래 수치는 **제안값이며 1b 게이트 태스크에서 확정**한다. 확정값은 `02-engine-full.md`에 기록하고, 바꾸면 이 절을 먼저 고친다.

- **1a 기준선의 지위.** Plan 1a(분할 4 Task 14 `PERF.md`)의 데스크톱 node wasm 수치는 잠정 위험 신호다. 모바일 적합성을 입증하지 않는다. 메모리 기준선은 `ImageData`·`ScoringReference`를 모두 명시적으로 해제한 벤치로 다시 잰 값만 쓴다(1a 3회차 리뷰에서 해제 누락이 피크를 부풀린 것을 확인).
- **1a 기준선 값(2026-10-05, i7-10700K, Windows, Node 24.16, MSVC 빌드 wasm, 해제 정리 후).** 로드 p50 1031ms(원본 JPEG 디코드 37 + 정답 PNG 디코드 50 + `ScoringReference` 생성 945), 제출 p50 1767ms·p95 1776ms(렌더 1088 + 채점 678), 전환(로드+제출+해제) p50 2805ms, wasm 선형 메모리 140.8MiB(로드만 108.8MiB), 전환 7회 동안 증가 0. 해제 누락이 있던 이전 벤치의 223.5MiB에는 미해제 객체 약 83MiB가 섞여 있었다.
- **측정 기기(최소 2대).** 데스크톱 1대(1a 기준선을 잰 i7-10700K + Chrome 안정판)와 중급 모바일 1대(AC-S5a 판정 기기인 Galaxy A54 + Chrome). iPhone 12 Safari를 포함한 AC-S5a 최종 판정은 Plan 2의 실제 UI Worker에서 한다.
- **측정 경로.** 정적 벤치 페이지가 Dedicated Worker 안에서 `engine-wasm`을 로드해 실행한다(node 아님). 시간은 Worker의 `performance.now()`. 메모리는 wasm `memory.buffer.byteLength`와, 가능한 경우 `performance.measureUserAgentSpecificMemory()`(cross-origin isolated 페이지)로 JS 복사본까지 포함해 잰다. 이 API가 없는 브라우저는 개발자 도구 메모리 타임라인으로 수동 기록하고 그 사실을 결과에 적는다.
- **시나리오(분리 측정).**
  1. 로드: 원본 JPEG·정답 PNG fetch 완료 후, 엔진 디코드 → `ScoringReference` 생성까지. 콜드 1회 + 워밍 5회.
  2. 제출: 참조 재사용, 플레이어 2048 렌더 + `score`. 워밍업 1회 + 20회 p50/p95.
  3. 반복 전환: 챌린지 로드 → 제출 3회 → 전부 `free()`를 10회 연속. 회차별 메모리 기록.
- **예산(제안값).**

| 항목 | 중급 모바일 | 데스크톱 |
|------|-------------|----------|
| 제출 p95 | ≤ 2.0초(AC-S5a) | ≤ 0.7초 |
| 로드 p95 | ≤ 3.0초 | ≤ 1.0초 |
| 피크 메모리(wasm 선형 + JS 측 버퍼) | ≤ 300MiB | 기록만 |
| 전환 10회 후 메모리 증가 | 1회차 대비 ≤ 10%, 단조 증가 없음 | 같음 |

- **중단 조건.** 아래 중 하나라도 해당하면 1b 기능 확장 전에 멈추고 해상도·효과 범위·구현을 재결정한다(스펙 AC-S5a 조항, 변경 이력에 기록). ① 비트 동일 최적화 후에도 모바일 제출 p95 > 2.0초 ② 모바일 로드 p95 > 3.0초 ③ 피크 메모리 예산 초과 ④ 반복 전환에서 메모리가 회차마다 늘어남(누수) ⑤ 측정 중 탭 재로드·크래시가 1회라도 발생.
- **재검증.** 1b의 최대 복잡도 기능(마스크 3개·커브·그레이딩·그레인·누출 페널티)이 완성된 뒤, 1b 마지막 태스크에서 최대 복잡도 레시피로 같은 하네스·같은 기준을 다시 통과해야 한다. 게이트 통과는 그 시점의 기능 집합에만 유효하다.

각 하위 계획은 완료 시 다음 계획의 인터페이스(§3)를 변경하지 않아야 한다. 변경이 필요하면 이 문서를 먼저 고치고 스펙 변경 이력에 남긴다.

## 5. 결정성 규율 (모든 엔진 코드에 적용)
1. JPEG은 `image`의 `jpeg` 기능을 켜지 않고 `zune-jpeg`만 `default-features=false`로 직접 의존(전이 기능 활성 경로 차단, `cargo tree -e features`로 CI 확인). 초월함수는 `libm::*`만(`default-features=false` + `force-soft-floats`, `arch` 비활성). `f32::{powf,powi,exp,exp2,ln,log2,sin,cos,tan,atan2,cbrt,hypot,mul_add}` 및 f64 동종 함수 사용 금지(메서드 `.powf(`와 UFCS `f32::powf(` 둘 다), `libm::fma*` 금지 — `scripts/lint-determinism.sh`가 CI에서 grep으로 차단. libm `arch` 기능 비활성.
2. 부동소수 누적 순서 고정(오름차순 단일 스레드). 병렬화는 픽셀 단위 독립 연산에만, 리덕션은 단일 스레드.
3. `Cargo.lock` 커밋, 모든 cargo 명령 `--locked`, CI 끝에 `git diff --exit-code -- engine/Cargo.lock`. 픽셀·점수를 결정하는 크레이트의 버전 변경은 골든 출력이 같아도 담당 버전 범프 + 골든 재생성: `zune-jpeg`·`zune-core`·`png`·`fdeflate`·`miniz_oxide`(디코드) → `ENGINE_VERSION`, `libm`(렌더와 채점 수학) → `ENGINE_VERSION`과 `SCORING_VERSION`. CI 가드가 base/head `engine/Cargo.lock`의 버전을 비교해 강제한다(01a-4 Task 12a `crate_changes`, Task 14).
4. 어떤 `.cargo/config.toml`에도 `target-cpu`/`target-feature` 지정 금지, CI 환경의 `RUSTFLAGS`·`CARGO_ENCODED_RUSTFLAGS` 비어 있음을 확인. 툴체인(`rust-toolchain.toml`)·wasm-pack 버전 고정.
5. 8bit 양자화·클램프는 `buffer::quantize` 한 함수만 사용(A3의 두 지점).
6. 골든 테스트: 레시피 세트 → 2048 렌더 RGBA8·1024 채점 이미지 SHA-256 + **중간 점수 플레이어 레시피의 Score JSON 전체**가 ubuntu/windows/macos 네이티브(debug·release)와 wasm(node)에서 완전 일치. 실사 CC0 JPEG(baseline 4:2:0, progressive)와 비정수 배율(2048×1365→1024×683) 포함. 골든 재생성은 `engine-cli golden`만 사용하며 버전 문자열이 같은데 해시가 바뀌면 실패.

## 6. 합의 루프 절차 (각 하위 계획마다)
1. 계획 초안 작성(writing-plans 규칙: 완전한 코드·명령·예상 출력, 플레이스홀더 금지).
2. 병렬 리뷰 — **모든 리뷰어는 Codex 서브에이전트**(서브에이전트가 `codex exec -m gpt-6-astra --skip-git-repo-check -s read-only` 를 실행하고 출력만 전달, Claude가 직접 리뷰하지 않음): Architect 역할(설계 일관성·인터페이스·결정성), Critic 역할(스펙 AC 커버리지·테스트 설계·플레이스홀더 스캔), 일반 리뷰(외부 시각).
3. 리뷰 반영 → 남은 불일치는 "사용자 결정 필요"로 표시 → **승인 대기**. 승인 전 코드 작성 금지.

## 7. 리스크와 완화
| 리스크 | 완화 |
|--------|------|
| Rust 미설치·학습 곡선 | Plan 1a Task 0에 설치 절차, 모든 단계에 완전한 코드 제공 |
| 모바일 WASM 채점 2초 초과 | Plan 1a에서 데스크톱 기준선 측정(잠정 신호), 1b 첫 태스크에서 §4.1 게이트(모바일 Worker 실측·메모리·누수 조건), SIMD(wasm32 simd128) 옵션 검토, 실패 시 스펙 재결정(AC-S5a 조항) |
| Lanczos3 링잉으로 만점 조건(타일 95p) 불안정 | 정답과 플레이어가 같은 커널을 통과하므로 상쇄; T_tile은 벤치(Plan 3)에서 확정 |
| WebGL2 프리뷰와 참조 렌더 시각 차이 | AC-E1b 500ms 참조 교체; 프리뷰 셰이더 단위 테스트는 참조 출력과 ΔE ≤ 0.5 비교(Plan 2) |
| JPEG 디코더 크레이트 업데이트로 픽셀 변화 | Cargo.lock 고정 + 원본 RGBA8 해시를 매니페스트에 기록(A2), 변경 시 ENGINE_VERSION 범프 |
| 스펙 v2에 A2(WASM 디코드)가 없음 | 승인 시 스펙 v2.1로 AC-E1g 추가: "참조 경로의 입력 디코드는 엔진 내부 디코더로 수행하며, 원본 RGBA8 SHA-256을 매니페스트에 기록한다" |
