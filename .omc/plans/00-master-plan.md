# 사진 보정 따라잡기 게임 — 마스터 구현 계획

> **For agentic workers:** 이 문서는 아키텍처·인터페이스·로드맵을 고정하는 마스터 플랜이다. 실제 작업 단계는 하위 계획(`01-engine-foundation.md` 등)에 있으며, 각 하위 계획은 superpowers:subagent-driven-development 또는 superpowers:executing-plans 로 실행한다.

**Goal:** 스펙 v2(`.omc/specs/deep-interview-photo-edit-match-game.md`)의 인수 기준 ~70개를 4개의 하위 계획으로 나누어, 각 계획이 단독으로 테스트 가능한 소프트웨어를 산출하도록 한다.

**Architecture:** 결정성의 유일한 기준인 **참조 엔진(Rust → WASM + 네이티브 CLI)** 이 JPEG 디코드·렌더·축소·채점을 모두 담당하고, **웹 앱(Vite + React + TS)** 은 조작 중 GPU(WebGL2) 프리뷰와 UI·게임 루프·공유를 담당한다. 정답 이미지·골든 해시는 같은 엔진의 네이티브 빌드로 빌드 타임에 생성해 정적 매니페스트에 넣는다. 서버는 없다.

**Tech Stack:** Rust 1.80+ (stable), `wasm-bindgen`/`wasm-pack`, `libm`(순수 Rust 초월함수), `image`(JPEG/PNG 디코드·인코드), `sha2`, `serde_json`, `clap` / Node 24, Vite 6, React 19, TypeScript 5, Zustand, Tailwind, WebGL2, Vitest, Playwright / GitHub Actions(ubuntu·windows·macos 매트릭스) / 정적 호스팅(Cloudflare Pages 또는 Vercel).

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
│  │     ├─ image.rs                # ImageF32, RGBA8 변환·반올림 규칙
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
│  │     ├─ score.rs                # 평가 영역·상대 점수·3중 만점·진단 지표
│  │     ├─ hash.rs                 # SHA-256
│  │     └─ xmp.rs                  # crs XMP ↔ Recipe (Plan 1b)
│  ├─ cli/                          # 바이너리 `engine-cli`: fixture / render / gen-answer / score / hash / verify-challenge
│  ├─ wasm/                         # cdylib `engine-wasm`: decode / render / downscale / score / hash
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
| A2 | **JPEG 디코드도 참조 엔진 안에서**(`image` 크레이트, Cargo.lock 고정). 브라우저 `<img>`/canvas 디코드 픽셀은 참조 경로에 절대 넣지 않는다. 프리뷰는 브라우저 디코드 허용. | 브라우저별 JPEG IDCT 차이(±1)가 AC-E1a를 깨뜨림. **스펙 v2.1 추가 제안: AC-E1g** |
| A3 | 렌더는 f32, 채점의 Lab·ΔE00은 f64(libm). 최종 8bit 양자화는 `floor(clamp(v)*255+0.5)` 한 곳에서만. | AC-E1e, AC-S1g(오차 1e-4는 f32로 불안정) |
| A4 | 채점 이미지 = 2048 참조 렌더 → 선형광 Lanczos3(a=3, 픽셀 중심 i+0.5, clamp-to-edge, 가중치 정규화, 오름차순 누적) → 1024 장변. | AC-S1h |
| A5 | 평가 영역 E = 픽셀 마스크(u8 알파) 하나로 통일. 글로벌 챌린지 E=전체, 크롭 챌린지 E=정답 크롭, 로컬 챌린지 E=마스크 래스터(α≥0.5)+페더 확장. 채점 코드는 E만 본다. | AC-S1i~k, D14 |
| A6 | 버전 3종을 모든 산출물에 기록: `schema_version`(레시피), `ENGINE_VERSION`(렌더), `SCORING_VERSION`(채점, `T_TILE` 포함). | AC-S1b, AC-G3b |
| A7 | 웹 앱은 WASM을 **Web Worker**에서 실행(제출 렌더·채점 2초 예산이 UI를 막지 않도록). 프리뷰는 메인 스레드 WebGL2. | AC-S5a, AC-E2a |
| A8 | 정답·골든은 **빌드 타임 CLI**로 생성해 `content/answers/`와 `manifest.json`에 넣고, CI가 wasm 빌드로 같은 해시가 나오는지 검증한다. 런타임에 정답을 재생성하지 않는다(모바일 2초 예산 보호). | AC-E1a/E1f, D3 |
| A9 | 프리뷰 셰이더는 참조 엔진과 **같은 순서·같은 수식**을 GLSL로 옮긴 것이며, 점수 계약 밖. 입력 종료 500ms 후 워커의 참조 렌더로 교체. | AC-E1b |

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
검증 규칙(AC-E3c/d): 비유한 수 → 거부(에러). 범위 밖 → 클램프 후 `normalizations[]`에 기록. 마스크 > 3, 커브 점 > 16, 크롭 면적 < 0.25 → 거부.

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
    "recipe_url": "recipes/c001.json",           // 결과 화면 해설용 (플레이 중 로드 금지)
    "explanation": { "observation": "...", "intent": "...", "region": "...", "effect": "...", "sources": ["..."] },
    "eligibility": { "type": "global", "mean_de_full": 7.3, "passed": true }     // verify-challenge 출력
  }]
}
```

### 3.3 Score JSON (엔진 출력, 결과 화면·카드 입력)
```jsonc
{ "correction_score": 87, "raw_score": 87.42, "perfect": false,
  "d_e": 1.75, "d_e_original": 7.3, "mean_de": 1.75, "tile_p95_max": 4.1, "big_error_ratio": 0.004, "leakage_penalty": 0.0,
  "composition_score": null,
  "diagnostics": { "tone": 91, "color": 84, "detail": null, "local": null, "composition": null },
  "top_error_regions": [ { "x": 0.6, "y": 0.1, "w": 0.2, "h": 0.2, "mean_dl": -3.1, "mean_da": 0.4, "mean_db": 1.2 } ],
  "engine_version": "engine-0.1.0", "scoring_version": "scoring-0.1.0" }
```

### 3.4 WASM API (`engine-wasm`)
```ts
decode_image(bytes: Uint8Array): { width: number; height: number; rgba: Uint8Array }   // JPEG/PNG, 참조 디코더
render_rgba8(width, height, rgba: Uint8Array, recipe_json: string): Uint8Array         // 인코딩 sRGB RGBA8
downscale_rgba8(width, height, rgba, long_edge: number): { width; height; rgba }        // 선형광 Lanczos3
score_rgba8(width, height, original, answer, player, region_json: string): string       // Score JSON
sha256_hex(bytes: Uint8Array): string
versions(): string   // {"engine":"...","scoring":"...","schema":1}
```

### 3.5 URL / 저장
- 라우트: `/` 홈, `/c/{id}` 챌린지, `/c/{id}/result` 결과. 정적 호스팅 SPA fallback 필수(AC-H4a).
- localStorage 키: `best:{id}` → `{score, composition, engine, scoring, at}`, `draft:{id}` → `{recipe, engine, at}`.

## 4. 로드맵 (하위 계획)

| 계획 | 파일 | 범위 | 커버 AC | 산출물(단독 테스트 가능) |
|------|------|------|---------|--------------------------|
| **1a 엔진 기반** | `01-engine-foundation.md` | 툴체인, 색 과학, 이미지·리샘플, 레시피 스키마, basic 패널 렌더, 채점 핵심(E=전체), CLI, WASM 바인딩, 골든·CI 3-OS | E1a/c/e, E3c/d(부분), S1a~h/j, S2(톤·색), S3, S5a(측정 기반 마련) | `engine-cli`로 정답 생성·채점 가능, WASM==CLI 해시 일치 CI |
| **1b 엔진 완성** | `02-engine-full.md` | HSL·커브·그레이딩·스플릿토닝·디테일(샤프·그레인·비네팅), 마스크 3종·로컬 효과·렌더 순서, 크롭·구도 점수, 평가 영역·누출 페널티, 진단 5종·상위 오차 영역, XMP 변환, verify-challenge, 경계 레시피 세트 | E1d/f, E3a/b, E4(변환기), E5a/b/d/e, E7a/b(데이터), S1i/k, S2a~d, S6c, C2a~d, C6a | 스펙의 모든 렌더·채점 AC가 CLI+CI로 검증됨 |
| **2 에디터 웹앱** | `03-web-editor.md` | Vite/React 골격, WASM 워커, WebGL2 프리뷰(참조와 동일 수식), 패널·슬라이더·커브 UI, 마스크·크롭 도구(터치), 실행취소, 초안 저장, 작성 모드(레시피 저장/XMP), 성능 측정 | E1b, E2a/b, E3(UI), E4a~f(UI), E5c/f/g, E6a/b, E7/E7c, G3a/b, G4a/b | 챌린지 1개를 끝까지 편집·제출·채점 가능 |
| **3 콘텐츠·게임·공유** | `04-content-game-share.md` | 콘텐츠 파이프라인(build-content), 파일럿 3개 → 15~20개, 홈 그리드, 결과 화면(해설·관측 문장·비교 슬라이더), 카드 PNG·Web Share·폴백, SPA fallback·오류 처리, 벤치 세트·T_tile 확정, 배포 | C1~C6, S4, S6a/b, H1~H4b, G1, G2/G2a, G4 | 출시 가능한 v1 |

각 하위 계획은 완료 시 다음 계획의 인터페이스(§3)를 변경하지 않아야 한다. 변경이 필요하면 이 문서를 먼저 고치고 스펙 변경 이력에 남긴다.

## 5. 결정성 규율 (모든 엔진 코드에 적용)
1. 초월함수는 `libm::*`만. `f32::{powf,powi,exp,exp2,ln,log2,sin,cos,tan,atan2,cbrt,hypot,mul_add}` 및 f64 동종 함수 사용 금지 — `scripts/lint-determinism.sh`가 CI에서 grep으로 차단.
2. 부동소수 누적 순서 고정(오름차순 단일 스레드). 병렬화는 픽셀 단위 독립 연산에만, 리덕션은 단일 스레드.
3. `Cargo.lock` 커밋. `image`·`libm` 버전 변경은 `ENGINE_VERSION`/`SCORING_VERSION` 범프 + 골든 재생성.
4. `.cargo/config.toml`에 `target-cpu` 지정 금지. `RUSTFLAGS`에 fast-math 계열 없음.
5. 8bit 양자화·클램프는 `image.rs`의 한 함수만 사용.
6. 골든 테스트: 레시피 세트 → 1024 RGBA8 SHA-256 + 점수 JSON이 ubuntu/windows/macos 네이티브와 wasm(node)에서 완전 일치.

## 6. 합의 루프 절차 (각 하위 계획마다)
1. 계획 초안 작성(writing-plans 규칙: 완전한 코드·명령·예상 출력, 플레이스홀더 금지).
2. 병렬 리뷰 — Architect 서브에이전트(설계 일관성·인터페이스·결정성), Critic 서브에이전트(스펙 AC 커버리지·테스트 설계·플레이스홀더 스캔), Codex gpt-6-astra(외부 시각).
3. 리뷰 반영 → 남은 불일치는 "사용자 결정 필요"로 표시 → **승인 대기**. 승인 전 코드 작성 금지.

## 7. 리스크와 완화
| 리스크 | 완화 |
|--------|------|
| Rust 미설치·학습 곡선 | Plan 1a Task 0에 설치 절차, 모든 단계에 완전한 코드 제공 |
| 모바일 WASM 채점 2초 초과 | Plan 1a에서 CLI로 연산량 측정, 1b에서 SIMD(wasm32 simd128) 옵션 검토, 실패 시 스펙 재결정(AC-S5a 조항) |
| Lanczos3 링잉으로 만점 조건(타일 95p) 불안정 | 정답과 플레이어가 같은 커널을 통과하므로 상쇄; T_tile은 벤치(Plan 3)에서 확정 |
| WebGL2 프리뷰와 참조 렌더 시각 차이 | AC-E1b 500ms 참조 교체; 프리뷰 셰이더 단위 테스트는 참조 출력과 ΔE ≤ 0.5 비교(Plan 2) |
| JPEG 디코더 크레이트 업데이트로 픽셀 변화 | Cargo.lock 고정 + 원본 RGBA8 해시를 매니페스트에 기록(A2), 변경 시 ENGINE_VERSION 범프 |
| 스펙 v2에 A2(WASM 디코드)가 없음 | 승인 시 스펙 v2.1로 AC-E1g 추가: "참조 경로의 입력 디코드는 엔진 내부 디코더로 수행하며, 원본 RGBA8 SHA-256을 매니페스트에 기록한다" |
