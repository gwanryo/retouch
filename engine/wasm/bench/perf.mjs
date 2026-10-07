// Node performance baseline (Plan 1a) on the shared gate scenario (scenario.mjs runGate, master
// plan §4.1 counts: load cold 1 + warm 5, submit warm-up 1 + 20, switch 10 with 3 submits each).
// The browser Worker gate uses the same scenario (web/worker.mjs); this script keeps the Plan 1a
// line names, values are p50/p95 by nearest rank over the warm load, submit and switch samples.
// Usage (repository root):
//   wasm-pack build engine/wasm --target nodejs --release -- --locked
//   cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer \
//     --input engine/golden/images/lake_2048x1365_baseline420.jpg \
//     --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf
//   node engine/wasm/bench/perf.mjs
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { COUNTS, pct, runGate } from './scenario.mjs'
import { gitHead, harnessSha256 } from './serve.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const engine = resolve(here, '../..')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (p) => new Uint8Array(readFileSync(resolve(engine, p)))
const answerPng = 'target/perf/answer_2048.png'
if (!existsSync(resolve(engine, answerPng))) throw new Error(`engine/${answerPng} missing: run engine-cli gen-answer first (see header)`)
const MiB = (bytes) => (bytes / 2 ** 20).toFixed(1)

const inputs = {
  jpeg: read('golden/images/lake_2048x1365_baseline420.jpg'),
  answerPng: read(answerPng),
  playerRecipe: readFileSync(resolve(engine, 'golden/recipes/player_warm_near.json'), 'utf8'),
  regionJson: '{"kind":"full"}',
  seed: 0,
  compositionJson: null,
}
const r = await runGate(wasm, inputs, {
  now: () => performance.now(),
  wasmSha256: wasm.sha256_hex(read('wasm/pkg/engine_wasm_bg.wasm')),
  harness: { commit: gitHead(), sha256: harnessSha256() },
  kind: 'desktop',
  runId: crypto.randomUUID(),
})

const lines = {
  'load: decode original jpeg': r.load.warm.decodeOriginal,
  'load: decode answer png': r.load.warm.decodeAnswer,
  'load: ScoringReference::new': r.load.warm.reference,
  'load: total': r.load.warm.total,
  'submit: render player 2048': r.submit.render,
  'submit: score': r.submit.score,
  'submit: total': r.submit.total,
  'switch: load+submit+free': r.switch.ms,
}
const c = COUNTS
console.log(`runs: load warm ${c.loadWarm}, submit ${c.submit}, switch ${c.switch} (x${c.submitsPerSwitch} submits) node=${process.version}`)
for (const [label, xs] of Object.entries(lines)) {
  console.log(`${label.padEnd(30)} p50 ${pct(xs, 0.5).toFixed(0).padStart(5)} ms   p95 ${pct(xs, 0.95).toFixed(0).padStart(5)} ms`)
}
const after = r.switch.wasmBytesAfter
console.log(`load cold                      ${r.load.cold.total.toFixed(0)} ms`)
console.log(`wasm memory peak               ${MiB(r.memory.wasmPeakBytes)} MiB`)
console.log(`wasm memory after switch 1/10  ${MiB(after[0])} / ${MiB(after.at(-1))} MiB`)
console.log(`js alloc two-rep (policy)      ${MiB(r.memory.jsAllocTwoRepBytes)} MiB`)
console.log(`process rss                    ${MiB(process.memoryUsage().rss)} MiB`)
