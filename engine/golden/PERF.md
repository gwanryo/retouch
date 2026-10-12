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

**판정: 조건부 PASS**(2026-10-08 사용자 결정). 데스크톱 GATE PASS 3/3, Galaxy S26 Ultra 단일 회차 예산 충족(GATE INCOMPLETE, 1회), Galaxy A12 FAIL(시간만, 메모리 PASS). 스펙 판정 기기(Galaxy A54, 중급)는 실측하지 못했고, 칩 세대로 보아 S26U보다 2~3배 느린 수준이라 제출 약 0.6~0.9초, 로드 약 0.7~1.1초로 추정한다(실측 아님). 보급형(A12급)은 v1 성능 보장 대상에서 빠진다(동작은 하지만 느림). 중급 실기기 최종 판정은 Plan 2와 분할 6 최대 복잡도 재검증에서 한다. 근거 기록: 스펙 변경 이력 v2.1(측정 기기)·v2.2(판정).

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

## Plan 1b-2 글로벌 색 회귀 (2026-10-11)

i7-10700K, Node v24.16.0, rustc 1.98.1, node wasm(release). main 기준선은 별도 worktree에서 빌드해 같은 세션에 main, 1b-2 기본, 1b-2 최대 순으로 3회 교차 측정했다. 최대 플레이어는 정답 `grade_split`(`target/perf-color`), 플레이어 `player_global_max`.

| 행 | 회차 | load total p50 | render p50 | render p95 | score p50 | submit total p50 | submit total p95 | wasm 최댓값 |
|----|-----:|---:|---:|---:|---:|---:|---:|---:|
| main 기본 플레이어 | 1 | 918 | 124 | 135 | 542 | 667 | 684 | 109.0MiB |
| main 기본 플레이어 | 2 | 829 | 109 | 117 | 464 | 575 | 598 | 109.0MiB |
| main 기본 플레이어 | 3 | 875 | 111 | 125 | 474 | 593 | 669 | 109.0MiB |
| 1b-2 기본 플레이어 | 1 | 915 | 148 | 164 | 569 | 723 | 857 | 109.0MiB |
| 1b-2 기본 플레이어 | 2 | 772 | 121 | 134 | 456 | 578 | 650 | 109.0MiB |
| 1b-2 기본 플레이어 | 3 | 778 | 117 | 133 | 446 | 565 | 580 | 109.0MiB |
| 1b-2 최대 플레이어 | 1 | 821 | 553 | 569 | 481 | 1033 | 1051 | 108.9MiB |
| 1b-2 최대 플레이어 | 2 | 798 | 515 | 546 | 452 | 969 | 1075 | 108.9MiB |
| 1b-2 최대 플레이어 | 3 | 943 | 546 | 626 | 473 | 1010 | 1102 | 108.9MiB |

단위 ms. 같은 세션에서도 회차 간 편차가 컸다(main score p50 464~542).

### 판정 (D4 기준)

- 기본 플레이어 render p50이 main 대비 +5% 이내: **FAIL**. 3회 중앙값 기준 111 → 121ms(+9%), 회차별 +19%, +11%, +5%. 편차가 크지만 기준을 넘는다.
- 최대 플레이어 submit total p95 ≤ 650ms(3회 모두): **FAIL**. 1051 / 1075 / 1102ms. render p50이 515~553ms라 이것이 원인이다(기본 플레이어는 약 120ms).
- wasm 메모리 최댓값 109.0MiB 유지: **PASS**. 108.9MiB.

전체 판정 FAIL. 스테이지별 프로파일에서 커브가 증가분의 57%였고, 산술 미세 최적화는 효과가 없었다. 사용자 결정(2026-10-11)으로 출력이 한 비트도 바뀌지 않는 가속(아래 Task 6a)을 넣고 다시 쟀다. `ENGINE_VERSION`·골든은 그대로다.

### Task 6a 뒤 재측정 (2026-10-11)

Task 6a: `render_rgba8`에 입력 RGB 바이트 키 캐시(direct-mapped 2^18 슬롯, 약 2 MiB, 렌더 호출마다 할당, 처음 65536 픽셀 적중률 25% 미만이면 캐시 없는 루프로 폴백)와 스테이지 ⑤ 전부 비활성일 때의 기본 패널 전용 루프를 넣었다. 8bit 입력에서 스테이지 ①~⑤가 픽셀 (r,g,b) 바이트만의 순수 함수라서 비트 동일하다(공간 스테이지가 들어오면 캐시 위치를 옮겨야 한다, `render/memo.rs` 참고).

같은 환경과 방법(main은 별도 worktree, 3회 교차).

| 행 | 회차 | load total p50 | render p50 | render p95 | score p50 | submit total p50 | submit total p95 | wasm 최댓값 |
|----|-----:|---:|---:|---:|---:|---:|---:|---:|
| main 기본 플레이어 | 1 | 745 | 102 | 106 | 414 | 519 | 538 | 109.0MiB |
| main 기본 플레이어 | 2 | 718 | 101 | 107 | 409 | 510 | 529 | 109.0MiB |
| main 기본 플레이어 | 3 | 718 | 102 | 106 | 411 | 513 | 527 | 109.0MiB |
| 1b-2 기본 플레이어 | 1 | 713 | 29 | 34 | 411 | 438 | 468 | 109.0MiB |
| 1b-2 기본 플레이어 | 2 | 722 | 28 | 31 | 407 | 436 | 450 | 109.0MiB |
| 1b-2 기본 플레이어 | 3 | 707 | 29 | 31 | 409 | 439 | 451 | 109.0MiB |
| 1b-2 최대 플레이어 | 1 | 697 | 41 | 46 | 401 | 442 | 467 | 108.9MiB |
| 1b-2 최대 플레이어 | 2 | 690 | 42 | 44 | 399 | 440 | 459 | 108.9MiB |
| 1b-2 최대 플레이어 | 3 | 696 | 43 | 48 | 405 | 449 | 467 | 108.9MiB |

단위 ms. 이번 세션은 회차 간 편차가 작았다(main render p50 101~102).

#### 판정 (D4 기준)

- 기본 플레이어 render p50이 main 대비 +5% 이내: **PASS**. 102 → 29ms(-72%).
- 최대 플레이어 submit total p95 ≤ 650ms(3회 모두): **PASS**. 467 / 459 / 467ms.
- wasm 메모리 최댓값 109.0MiB 유지: **PASS**. 108.9 / 109.0MiB.

전체 판정 PASS. 캐시 효과는 실사진 기준이다. 고유색이 많은 잡음 같은 입력은 처음 65536 픽셀 뒤 캐시 없는 루프로 돌아가므로 Task 6a 이전 수준(최대 플레이어 render 약 0.5s)에 가까울 것으로 본다(이번에 따로 재지 않음).

브라우저·모바일 재검증은 분할 6에서 한다.

## Plan 1b-3 디테일 회귀 (2026-10-12)

i7-10700K, Node v24.16.0, rustc 1.98.1, node wasm(release, `--locked`). main(72baa5f) 기준선은 별도 worktree에서 빌드하고 벤치 하네스(`perf.mjs`)만 이 브랜치 것으로 덮어썼다. 같은 세션에 main 기본, 1b-3 기본, 1b-3 디테일 최대 순으로 3회 교차 측정했다. 기본 행은 정답 `warm_contrast`와 플레이어 `player_warm_near`(시드 0), 디테일 최대 행은 정답 `detail_mix`(시드 7)와 플레이어 `player_detail_max`(골든 레시피, 시드 7)다. 측정 전 `tasklist`로 codex, cargo, node, rustc 프로세스가 없음을 확인했다.

| 행 | 회차 | load total p50 | render p50 | render p95 | score p50 | submit total p50 | submit total p95 | wasm 최댓값 |
|----|-----:|---:|---:|---:|---:|---:|---:|---:|
| main 기본 플레이어 | 1 | 680 | 24 | 28 | 387 | 412 | 428 | 109.0MiB |
| main 기본 플레이어 | 2 | 672 | 24 | 27 | 382 | 407 | 416 | 109.0MiB |
| main 기본 플레이어 | 3 | 664 | 25 | 27 | 383 | 407 | 413 | 109.0MiB |
| 1b-3 기본 플레이어 | 1 | 360 | 23 | 25 | 198 | 221 | 225 | 117.1MiB |
| 1b-3 기본 플레이어 | 2 | 362 | 22 | 27 | 195 | 217 | 230 | 117.1MiB |
| 1b-3 기본 플레이어 | 3 | 360 | 22 | 24 | 196 | 218 | 226 | 117.1MiB |
| 1b-3 디테일 최대 | 1 | 405 | 253 | 259 | 295 | 549 | 556 | 128.6MiB |
| 1b-3 디테일 최대 | 2 | 408 | 248 | 254 | 298 | 545 | 559 | 128.6MiB |
| 1b-3 디테일 최대 | 3 | 399 | 249 | 260 | 301 | 549 | 566 | 128.6MiB |

단위 ms. 원시값은 `perf.mjs`의 `raw:` 출력에서 옮겼다(wasm 최댓값 바이트: main 114294784, 기본 122814464, 디테일 최대 134873088).

### 판정 (D4 기준, 원시값 기준 3회 각각)

- 디테일 최대 submit total p95 ≤ 650.0ms: **PASS**. 555.84 / 558.64 / 566.22ms.
- `wasm_peak_bytes` ≤ 150·2^20(157286400): **PASS**. 3회 모두 134873088(128.6MiB).
- 1b-3 기본 render p50 ≤ 같은 회차 main render p50 × 1.05: **PASS**. 22.84 ≤ 25.37, 22.22 ≤ 25.65, 21.88 ≤ 26.39.

전체 판정 PASS. 1b-3 기본 플레이어의 score는 채점 perf 최적화로 main의 약 절반이고, render는 main과 동급 이하다.

처음 한 번 잰 측정(같은 날, 리뷰어 프로세스 3개가 동시에 돌던 상태)은 다른 프로세스와 겹쳐 폐기했다. 그 회차의 디테일 최대 submit p95는 652.32 / 589.89 / 551.26ms로 1회차가 기준을 2.3ms 넘었고, 조용한 상태에서 처음부터 다시 쟀다.

wasm 힙 피크는 할당 순서에 민감하다(6b에서 쌍 memo 선할당으로 +24-32MiB 회피). 분할 4에서 단계별 memory_bytes 재측정이 필요하다.

브라우저·모바일 재검증은 분할 6에서 한다.
