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
// Max global-color player (answer from grade_split.json via --out-dir engine/target/perf-color):
//   node engine/wasm/bench/perf.mjs --answer target/perf-color/answer_2048.png \
//     --player golden/recipes/player_global_max.json
// Challenge seed (default 0): --seed <u32>. The last line, `raw: {...}`, is unrounded.
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { COUNTS, pct, runGate } from './scenario.mjs'
import { gitHead, harnessSha256 } from './serve.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const engine = resolve(here, '../..')
const read = (p) => new Uint8Array(readFileSync(resolve(engine, p)))
// Options (paths relative to engine/): --answer <png> (default target/perf/answer_2048.png),
// --player <recipe.json> (default golden/recipes/player_warm_near.json), --seed <u32> (default 0).
const option = (name, fallback) => {
  const i = process.argv.indexOf(name)
  if (i === -1) return fallback
  const v = process.argv[i + 1]
  if (!v || v.startsWith('--')) throw new Error(`${name} needs a value`)
  return v
}
const answerPng = option('--answer', 'target/perf/answer_2048.png')
const playerRecipe = option('--player', 'golden/recipes/player_warm_near.json')
const seedText = option('--seed', '0')
const seed = Number(seedText)
if (!/^\d+$/.test(seedText) || !Number.isInteger(seed) || seed > 4294967295) throw new Error('--seed needs a u32')
// All options are validated before anything reads a file or loads the wasm package.
if (!existsSync(resolve(engine, answerPng))) throw new Error(`engine/${answerPng} missing: run engine-cli gen-answer first (see header)`)
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const MiB = (bytes) => (bytes / 2 ** 20).toFixed(1)

const inputs = {
  jpeg: read('golden/images/lake_2048x1365_baseline420.jpg'),
  answerPng: read(answerPng),
  playerRecipe: readFileSync(resolve(engine, playerRecipe), 'utf8'),
  regionJson: '{"kind":"full"}',
  seed,
  compositionJson: null,
}
console.log(`inputs: answer engine/${answerPng}, player engine/${playerRecipe}, seed ${seed}`)
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
const raw = {
  render_p50: pct(r.submit.render, 0.5),
  render_p95: pct(r.submit.render, 0.95),
  score_p50: pct(r.submit.score, 0.5),
  submit_p50: pct(r.submit.total, 0.5),
  submit_p95: pct(r.submit.total, 0.95),
  load_p50: pct(r.load.warm.total, 0.5),
  wasm_peak_bytes: r.memory.wasmPeakBytes,
}
console.log(`raw: ${JSON.stringify(raw)}`)
