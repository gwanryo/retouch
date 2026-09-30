# Retouch — 프로젝트 규약 (Claude Code)

사진 보정 따라잡기 학습 게임. 스펙과 계획이 진실의 원천이다. 코드를 바꾸기 전에 읽는다.

- 스펙(v2): `.omc/specs/deep-interview-photo-edit-match-game.md` — 결정 D1~D14, 인수 기준 AC-*, Non-Goals
- 마스터 플랜: `.omc/plans/00-master-plan.md` — 아키텍처 결정 A1~A9, 인터페이스(레시피 JSON·매니페스트·Score JSON·WASM API), 결정성 규율
- 하위 계획: `.omc/plans/01-*.md` 이후. 계획은 **승인 후** 실행한다(코드 작성 전 사용자 확인).

## 언어
- 사용자와의 대화, 커밋 메시지 본문, 스펙·계획 문서, UI 문구는 **한국어**. 코드 식별자·주석은 영어.

## 스킬 사용 규칙 (필수)

### 프론트 웹 디자인/구현 (`web/`)
UI를 새로 만들거나 고칠 때는 **작업 전에** 아래 스킬을 로드하고 그 규칙을 따른다. "간단한 수정"도 예외가 아니다.

| 시점 | 스킬 | 목적 |
|------|------|------|
| 디자인 시작 전 | `frontend-design` | 명확한 미학 방향 선택, 제네릭 AI 스타일 회피 |
| 디자인 시작 전 | `high-end-visual-design` | Double-bezel 컨테이너, 버튼-인-버튼 CTA, 커스텀 cubic-bezier 모션, 매크로 여백 |
| 디자인 시작 전 | `design-taste-frontend` | Design Read 1줄 + 다이얼(VARIANCE/MOTION/DENSITY) 선언, 하드 룰과 Pre-Flight Check |
| React 코드 작성/리뷰 | `vercel-react-best-practices` | 렌더 성능·번들·데이터 패턴 |
| 컴포넌트 API 설계 | `vercel-composition-patterns` | boolean prop 남발 방지, 합성 패턴 |
| 페이지/상태 전환 | `vercel-react-view-transitions` | View Transition API 기반 전환 |
| 완료 전 점검 | `web-design-guidelines` | Vercel Web Interface Guidelines 대조(`file:line` 출력) |
| 완료 전 점검 | `react-doctor` | `npx react-doctor@latest --verbose --diff`, 점수 하락 시 커밋 금지 |

`vercel-react-native-skills`는 React Native/Expo 전용이므로 웹(`web/`)에는 적용하지 않는다(향후 네이티브 앱을 만들 때만).

design-taste-frontend의 하드 룰 중 이 프로젝트에서 특히 지킬 것:
- 서체: Fraunces·Instrument Serif 기본 사용 금지. 현재 스택은 **Geist(라틴/숫자) + Gothic A1(한글 헤드라인) + IBM Plex Sans KR(본문) + Geist Mono(라벨)**.
- 팔레트: 크림 종이 + 클레이/브라스 + 에스프레소 조합 금지. 현재는 **모노크롬(zinc) + 코발트 단일 액센트**. 액센트는 페이지 전체에서 하나만.
- 아이콘: 손그림 SVG 금지, **`@phosphor-icons/react`** 한 가족만, `weight="light"` 또는 `"regular"` 통일.
- 사용자에게 보이는 텍스트에 em-dash(—)·en-dash(–) 금지. 하이픈(-)만.
- 3등분 동일 카드 금지. 벤토 셀 수 = 콘텐츠 수.
- 히어로: 헤드라인 ≤2줄, 부제 ≤20단어, CTA 1+1, 같은 의도의 CTA 중복 금지.
- 실제 이미지 사용(빈 div로 만든 가짜 스크린샷 금지). 사진은 CC0/Unsplash/Pexels + 출처 기록.
- 모션은 `transform`/`opacity`만, `prefers-reduced-motion` 존중, `h-screen` 대신 `min-h-[100dvh]`.
- 이모지는 기본 금지. 예외: Wordle식 공유 텍스트의 🟩🟨⬜ 바(제품 기능 자체).

### Rust 엔진 (`engine/`)
- 코드 작성·리뷰·계획 문서의 Rust 코드 포함 전에 **`rust-best-practices`** 로드.
- 마스터 플랜 §5 결정성 규율 준수: 초월함수는 `libm::*`만, std 수학 함수(`f32::powf` 등)·`mul_add`·`target-cpu` 금지, 8bit 양자화는 `image.rs`의 단일 함수, `Cargo.lock` 커밋.
- 렌더/채점 수식 변경 시 `ENGINE_VERSION`/`SCORING_VERSION` 범프 + 골든 재생성.

### 공통
- 새 기능/버그 수정은 `test-driven-development`(실패 테스트 → 구현 → 통과).
- 완료 주장 전 `verification-before-completion`: 실제 명령 출력으로 확인.
- 스펙 변경이 필요하면 스펙 파일의 "변경 이력"에 먼저 기록.

## 명령어
```bash
# web
cd web && npm ci && npm run check && npm run build   # lint + typecheck + test + build(+404.html)
cd web && npm run dev                                # http://localhost:5173/retouch/

# engine (Plan 1a 이후)
cargo test --manifest-path engine/Cargo.toml --workspace
bash scripts/lint-determinism.sh
wasm-pack build engine/wasm --target nodejs && node --test engine/wasm/test
```

## 배포
- `main` 푸시 → `.github/workflows/deploy.yml` → GitHub Pages → **https://rwe.kr/retouch/** (rwe.kr은 `gwanryo/gwanryo.github.io`의 CNAME). Vite `base: '/retouch/'`, SPA fallback은 `dist/404.html`.
- 서버 없음. 정답·골든은 빌드 타임에 `engine-cli`로 생성해 `content/`에 커밋.
