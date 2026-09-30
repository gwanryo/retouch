## 무엇을 바꾸나요

<!-- 한두 문장. 관련 이슈: #  -->

## 어느 인수 기준(AC)에 해당하나요

<!-- 예: AC-S1a, AC-E5g — 스펙 .omc/specs/deep-interview-photo-edit-match-game.md -->

## 체크리스트

- [ ] 테스트를 먼저 썼고, 실패 → 통과를 확인했다
- [ ] `web`: `npm run check` 통과
- [ ] `engine`: `cargo test --workspace` 통과, `scripts/lint-determinism.sh` 통과
- [ ] 렌더/채점 수식이 바뀌었다면 `ENGINE_VERSION` / `SCORING_VERSION`을 올리고 골든을 재생성했다
- [ ] UI가 바뀌었다면 모바일(390px) 스크린샷을 첨부했다
