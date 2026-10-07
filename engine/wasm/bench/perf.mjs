// Performance baseline for AC-S5a (Plan 1a measures only; the budget gate is Plan 1b's first
// task). Three measurements, each with one warm-up run and RUNS timed runs, p50/p95 by nearest
// rank:
//   load    challenge load as in A8: decode the 2048 original JPEG, decode the answer PNG that
//           engine-cli wrote, build the ScoringReference
//   submit  one ScoringReference reused: render a 2048 player, score it
//   switch  load + one submit + free, repeated; wasm linear memory must not keep growing
// Every ImageData and ScoringReference is freed in `finally`, so memory numbers are working
// memory, not garbage waiting for finalizers.
// Usage (repository root):
//   wasm-pack build engine/wasm --target nodejs --release -- --locked
//   cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer \
//     --input engine/golden/images/lake_2048x1365_baseline420.jpg \
//     --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf
//   node engine/wasm/bench/perf.mjs
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const RUNS = 7
const here = dirname(fileURLToPath(import.meta.url))
const engine = resolve(here, '../..')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (p) => new Uint8Array(readFileSync(resolve(engine, p)))
const answerPng = 'target/perf/answer_2048.png'
if (!existsSync(resolve(engine, answerPng))) throw new Error(`engine/${answerPng} missing: run engine-cli gen-answer first (see header)`)
const jpeg = read('golden/images/lake_2048x1365_baseline420.jpg')
const png = read(answerPng)
const playerRecipe = readFileSync(resolve(engine, 'golden/recipes/player_warm_near.json'), 'utf8')
const MiB = (bytes) => (bytes / 2 ** 20).toFixed(1)

const times = {}
const time = (label, f) => {
  const t0 = performance.now()
  const r = f()
  ;(times[label] ??= []).push(performance.now() - t0)
  return r
}

/** Decode with the engine, copy the pixels out once, free the wasm object. */
const decode = (bytes) => {
  const img = wasm.decode_image(bytes)
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

/** Challenge load (A8). Returns the reference and the original pixels the submit step needs. */
const load = () =>
  time('load: total', () => {
    const src = time('load: decode original jpeg', () => decode(jpeg))
    const answer = time('load: decode answer png', () => decode(png))
    const reference = time('load: ScoringReference::new', () =>
      new wasm.ScoringReference(src.width, src.height, src.rgba, answer.rgba, '{"kind":"full"}'))
    return { src, reference }
  })

const submit = ({ src, reference }) =>
  time('submit: total', () => {
    const player = time('submit: render player 2048', () =>
      wasm.render_rgba8(src.width, src.height, src.rgba, playerRecipe, 0))
    return time('submit: score', () => reference.score(player, null))
  })

const repeat = (f) => {
  f() // warm-up (JIT, memory growth); not recorded
  for (const k of Object.keys(times)) delete times[k]
  for (let i = 0; i < RUNS; i++) f()
  return { ...times }
}

const freeAfter = (f) => () => {
  const challenge = load()
  try {
    f(challenge)
  } finally {
    challenge.reference.free()
  }
}

const results = {}
Object.assign(results, repeat(freeAfter(() => {})))
const memAfterLoad = wasm.memory_bytes()
const challenge = load()
try {
  Object.assign(results, repeat(() => submit(challenge)))
} finally {
  challenge.reference.free()
}
const memBeforeSwitch = wasm.memory_bytes()
const switchTimes = repeat(() => time('switch: load+submit+free', freeAfter(submit)))
results['switch: load+submit+free'] = switchTimes['switch: load+submit+free']
const memAfterSwitch = wasm.memory_bytes()

const pct = (xs, p) => {
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.max(1, Math.ceil(p * s.length)) - 1]
}
console.log(`runs=${RUNS} node=${process.version}`)
for (const [label, xs] of Object.entries(results)) {
  console.log(`${label.padEnd(30)} p50 ${pct(xs, 0.5).toFixed(0).padStart(5)} ms   p95 ${pct(xs, 0.95).toFixed(0).padStart(5)} ms`)
}
console.log(`wasm memory after load runs    ${MiB(memAfterLoad)} MiB`)
console.log(`wasm memory after submit runs  ${MiB(memBeforeSwitch)} MiB`)
console.log(`wasm memory after switch runs  ${MiB(memAfterSwitch)} MiB (growth during switch ${MiB(memAfterSwitch - memBeforeSwitch)} MiB)`)
console.log(`process rss                    ${MiB(process.memoryUsage().rss)} MiB`)
