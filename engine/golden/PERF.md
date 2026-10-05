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
