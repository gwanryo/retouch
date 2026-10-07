# 성능 기준선 (AC-S5a, Plan 1a-4 Task 14)

wasm(nodejs 타깃, release) 엔진을 `engine/wasm/bench/perf.mjs`로 측정한 값이다. 단계마다 워밍업 1회 뒤 7회를 재고, p50/p95는 최근접 순위로 계산한다.

> 데스크톱 Node 기준이며 모바일·브라우저 Worker 적합성은 입증하지 않음. Plan 1b 첫 태스크 게이트 입력.

## 측정 환경

| 항목 | 값 |
|------|----|
| 기기 | Intel(R) Core(TM) i7-10700K CPU @ 3.80GHz, 63.9 GB RAM |
| OS | Microsoft Windows 10 Pro 10.0.19045 |
| Node | v24.16.0 |
| Rust 툴체인 | rustc 1.98.1 (48a229cea 2026-09-01), MSVC |
| 측정 일자 | 2026-10-05 |

## 재현

```bash
wasm-pack build engine/wasm --target nodejs --release -- --locked
cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer \
  --input engine/golden/images/lake_2048x1365_baseline420.jpg \
  --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf
node engine/wasm/bench/perf.mjs
```

## 단계별 시간 (ms)

| 단계 | p50 | p95 |
|------|----:|----:|
| 로드: 원본 JPEG 디코드 | 38 | 39 |
| 로드: 정답 PNG 디코드 | 51 | 58 |
| 로드: ScoringReference::new | 966 | 990 |
| **로드 합계** | **1055** | **1079** |
| 제출: 2048 플레이어 렌더 | 1108 | 1123 |
| 제출: 채점 | 695 | 711 |
| **제출 합계** | **1805** | **1827** |
| **전환 (로드 + 제출 + 해제)** | **2860** | **2875** |

## 메모리

```
wasm memory after load runs    108.8 MiB
wasm memory after submit runs  140.8 MiB
wasm memory after switch runs  140.8 MiB (growth during switch 0.0 MiB)
process rss                    208.7 MiB
```

전환 반복 중 wasm 선형 메모리 증가는 0.0 MiB다.

## 해석

제출 p50은 1805ms로 2000ms 미만이지만, 이는 데스크톱 Node 기준이며 모바일·브라우저 Worker 적합성은 입증하지 않는다. 예산 판정은 Plan 1b 첫 태스크 게이트가 이 값을 입력으로 받아 수행한다. 검증 실행에서는 제출 p50 1767ms, 로드 1031ms, 메모리 140.8MiB였고 이번 값과 편차 범위 안이다.

## Plan 1b-1 스테이지 프로파일 (2026-10-07)

네이티브 release, `score::tests::profile_scoring_stages`(5회 중앙값), lake 2048×1365, 정답 `warm_contrast`, 플레이어 `player_warm_near`. Task 1(렌더 LUT)과 Task 2(ΔE 생략) 적용 후.

```bash
cargo test --locked --release --manifest-path engine/Cargo.toml -p engine-core score::tests::profile_scoring_stages -- --ignored --nocapture
```

| 단계 | ms |
|------|---:|
| render 2048 | 129.3 |
| scoring_image | 152.5 |
| lab_image (플레이어 1024) | 86.7 |
| ΔE00 전 픽셀 | 302.6 |
| delta_e_image (같은 픽셀 생략) | 329.4 |
| error_stats | 12.3 |
| ScoringReference::new | 782.5 |

플레이어 채점 이미지의 서로 다른 색 97,101 / 699,392, 정답과 같은 픽셀 2,249(0.3%).

해석: 채점의 약 55%가 ΔE00이다. 같은 픽셀 생략은 이 중간 점수 플레이어에서 이득이 없고(비교 비용만큼 약간 느림, wasm 채점 p50 724 → 708~716ms로 노이즈 범위) 만점에 가까운 제출에서만 줄어든다. 후보 C1(`lab_image` 16% < 시도 조건 20%)과 C3(`error_stats` 2% < 10%)은 시도하지 않았다.

### 후보 C2 (미채택)

`scoring_image`의 LUT 선형화를 수평 패스에 결합해 2048 전체 선형 이미지(약 32MiB)를 만들지 않는 변경. 출력은 비트 동일(수평 패스·양자화 전 f32 `to_bits`와 최종 바이트, 2048×1365·2048×1·1×3000·1024×683·7×5)이었으나 wasm 벤치(2회)에서:

| 항목 | 전(Task 2) | 후(C2) |
|------|-----------:|-------:|
| 제출: 채점 p50 | 708~716ms | 700~715ms |
| 로드: ScoringReference::new p50 | 996~1023ms | 977~984ms |
| wasm 메모리(로드 후) | 108.8MiB | 92.8MiB |
| wasm 메모리(제출 후, 최댓값) | 108.8MiB | **124.8MiB** |

최댓값이 늘어 채택 기준("wasm 메모리 최댓값이 늘지 않을 것")에 걸려 되돌렸다. 할당 크기가 바뀌면서 선형 메모리 단편화가 생긴 것으로 보인다(원인은 확인하지 않음).

### 후보 C4 + C1 (채택, 2026-10-07)

데스크톱 Chrome 게이트 1회차가 FAIL(제출 p95 795ms > 700, 로드 p95 1074ms > 1000)이어서 사용자 결정으로 추가한 후보. 1024 채점 이미지 699,392픽셀 중 서로 다른 (플레이어 RGB, 정답 RGB) 쌍은 골든 플레이어 4종에서 180,839~190,871개(약 27%)였다.

- `delta_e_image`: 같은 바이트는 0.0, 나머지는 (a, b) 색쌍마다 ΔE00을 한 번만 계산(메모)
- `lab_image`: 색마다 Lab을 한 번만 계산(C1)
- 메모는 속도만 바꾼다: 값은 키의 순수 함수라 자리마다 기존 계산과 `to_bits` 동일(`delta_e_image_matches_naive_on_repeated_color_pairs`, `lab_image_cache_matches_uncached_bitwise`, 키에서 정답 색을 뺀 변이를 테스트가 잡는 것 확인)

wasm(node) p50, 같은 기기:

| 항목 | 전 | 후 |
|------|---:|---:|
| 로드: ScoringReference::new | 975ms | 598~619ms |
| 로드 합계 | 1063ms | 693~719ms |
| 제출: 채점 | 700ms | 389~410ms |
| 제출 합계 | 799ms | 492~508ms |
| wasm 메모리 최댓값 | 109.0MiB | 109.0MiB |
