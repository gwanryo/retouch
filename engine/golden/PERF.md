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

## Plan 1b-1 게이트 (2026-10-07~08)

**판정: 조건부 PASS**(2026-10-08 사용자 결정). 데스크톱 GATE PASS 3/3, Galaxy S26 Ultra PASS(1회), Galaxy A12 FAIL(시간만, 메모리 PASS). 스펙 판정 기기(Galaxy A54, 중급)는 실측하지 못했고, 칩 세대로 보아 S26U보다 2~3배 느린 수준이라 제출 약 0.6~0.9초, 로드 약 0.7~1.1초로 추정한다(실측 아님). 보급형(A12급)은 v1 성능 보장 대상에서 빠진다(동작은 하지만 느림). 중급 실기기 최종 판정은 Plan 2와 분할 6 최대 복잡도 재검증에서 한다. 근거 기록: 스펙 변경 이력 v2.1.

측정: 브라우저 Dedicated Worker(`engine/wasm/bench/web/`, `serve.mjs`, COOP/COEP), 로드 콜드 1 + 워밍 5, 제출 워밍업 1 + 20, 전환 10회(로드 → 제출 3 → 해제), 잔존 메모리는 시간 측정 뒤 별도 패스에서 `measureUserAgentSpecificMemory`. 결과 원본: `engine/wasm/bench/results/2026-10-07/`.

피크 메모리는 wasm 선형 최댓값 + JS 할당 정책 지표(같은 단계 연속 두 반복의 할당 합)다. **정책 지표다**: "GC가 직전 반복의 버퍼까지는 회수하지 못해도 그 이전 것은 회수한다"는 가정 아래의 값이며 실제 피크의 상한을 보장하지 않는다(GC가 세 반복 이상 돌지 않으면 실제가 더 크다).

### 최적화 전후 (node wasm, i7-10700K, p50)

| 줄 | 1b-1 착수 시점(main af5df41, 같은 날 측정) | 1b-1 최종 |
|----|---:|---:|
| load: decode original jpeg | 38 | 40 |
| load: decode answer png | 52 | 53 |
| load: ScoringReference::new | 993 | 581 |
| load: total | 1089 | 679 |
| submit: render player 2048 | 1140 | 100 |
| submit: score | 711 | 399 |
| submit: total | 1855 | 502 |
| wasm 메모리 최댓값 | 140.8MiB | 109.0MiB |

(Plan 1a의 switch 줄은 제출 1회, 1b-1은 제출 3회라 비교하지 않는다.)

### 기기별 (브라우저 Worker)

| 기기 | 회차 | 로드 콜드 | 로드 워밍 p50 / p95 | 렌더 p50 | 채점 p50 | 제출 p50 / p95 | 피크 | 잔존 증가 |
|------|------|---:|---:|---:|---:|---:|---:|---:|
| i7-10700K Chrome 154 | C4 전 1 | 1196 | 1053 / 1074 | 97 | 691 | 786 / 795 | 219MiB | -1.8% |
| i7-10700K Chrome 154 | 1 | 828 | 664 / 685 | 97 | 376 | 475 / 517 | 219MiB | -0.03% |
| i7-10700K Chrome 154 | 2 | 854 | 663 / 684 | 98 | 392 | 493 / 520 | 219MiB | -0.05% |
| i7-10700K Chrome 154 | 3 | 858 | 706 / 767 | 98 | 383 | 480 / 505 | 219MiB | +0.04% |
| Galaxy S26 Ultra Chrome 154 | 1 | 489 | 349 / 352 | 52 | 239 | 291 / 298 | 219MiB | +0.02% |
| Galaxy A12 Chrome 152 (RAM 2GB) | 1* | 7151 | 5107 / 5112 | 857 | 3041 | 3901 / 3973 | 219MiB | +0.02% |
| Galaxy A12 Chrome 152 (RAM 2GB) | 2 | 7160 | 5141 / 5148 | 856 | 3019 | 3877 / 3910 | 219MiB | +0.03% |

단위 ms, p50/p95는 nearest-rank(judge와 같은 식). *A12 1회차는 kind를 desktop으로 잘못 골랐다(측정값은 유효, 원본 보관). 모든 회차에서 탭 재로드·크래시 없음, wasm 메모리 증가 0.

### judge.mjs 출력

```
$ node engine/wasm/bench/judge.mjs desktop desktop-chrome-run*.json
INFO load_cold_ms 828.285000000149 - run1
PASS submit_p95_ms 516.7800000011921 700 run1
PASS load_p95_ms 684.6649999991059 1000 run1
INFO peak_mib 219.08316230773926 - run1
PASS wasm_growth 0 0.1 run1
PASS wasm_monotonic not-strictly-increasing not-strictly-increasing run1
PASS residual_growth -0.00028944515683848273 0.1 run1
PASS residual_monotonic not-strictly-increasing not-strictly-increasing run1
INFO load_cold_ms 853.7299999985844 - run2
PASS submit_p95_ms 519.5499999988824 700 run2
PASS load_p95_ms 684.2200000006706 1000 run2
INFO peak_mib 219.08316230773926 - run2
PASS wasm_growth 0 0.1 run2
PASS wasm_monotonic not-strictly-increasing not-strictly-increasing run2
PASS residual_growth -0.00047998297926417653 0.1 run2
PASS residual_monotonic not-strictly-increasing not-strictly-increasing run2
INFO load_cold_ms 858.3600000012666 - run3
PASS submit_p95_ms 504.56000000052154 700 run3
PASS load_p95_ms 767.2050000000745 1000 run3
INFO peak_mib 219.08316230773926 - run3
PASS wasm_growth 0 0.1 run3
PASS wasm_monotonic not-strictly-increasing not-strictly-increasing run3
PASS residual_growth 0.0003573577253739253 0.1 run3
PASS residual_monotonic not-strictly-increasing not-strictly-increasing run3
GATE PASS

$ node engine/wasm/bench/judge.mjs mobile a12-chrome-run2.json
INFO load_cold_ms 7160.135000000009 - run1
FAIL submit_p95_ms 3909.5149999998976 2000 run1
FAIL load_p95_ms 5147.945000000065 3000 run1
PASS peak_mib 219.08316230773926 300 run1
PASS wasm_growth 0 0.1 run1
PASS wasm_monotonic not-strictly-increasing not-strictly-increasing run1
PASS residual_growth 0.0003369572551498627 0.1 run1
PASS residual_monotonic not-strictly-increasing not-strictly-increasing run1
INFO runs 1 3 -
GATE FAIL

$ node engine/wasm/bench/judge.mjs mobile s26u-chrome-run1.json
INFO load_cold_ms 489.21499997377396 - run1
PASS submit_p95_ms 298.08999997377396 2000 run1
PASS load_p95_ms 351.8299999833107 3000 run1
PASS peak_mib 219.08316230773926 300 run1
PASS wasm_growth 0 0.1 run1
PASS wasm_monotonic not-strictly-increasing not-strictly-increasing run1
PASS residual_growth 0.0001671071666497402 0.1 run1
PASS residual_monotonic not-strictly-increasing not-strictly-increasing run1
INFO runs 1 3 -
GATE INCOMPLETE
```
