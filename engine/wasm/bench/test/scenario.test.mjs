// Shared bench scenario (plan 02-1 Task 3, table 3.4 "scenario"). Fake wasm for call counts,
// buffers and timing; the real nodejs wasm (S8) when BENCH_FIXTURE_DIR points at a fixture made
// by `engine-cli fixture` + `gen-answer --allow-any-size`.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { COUNTS, pct, runGate, switchOnce, validateResult } from '../scenario.mjs'

const here = dirname(fileURLToPath(import.meta.url))

/** Fake wasm module. Every object throws on a second free(); `live()` counts unfreed objects. */
const fakeWasm = ({ decodeBytes = 1000, renderBytes = 400, onRender = () => {}, memory } = {}) => {
  const calls = []
  const args = { ScoringReference: [], render_rgba8: [], score: [] }
  let created = 0
  let refsCreated = 0
  let renders = 0
  const liveSet = new Set()
  const track = (obj) => {
    created++
    liveSet.add(obj)
    const free = () => {
      if (!liveSet.has(obj)) throw new Error('double free')
      liveSet.delete(obj)
    }
    obj.free = free
    return obj
  }
  const fake = {
    calls,
    args,
    live: () => liveSet.size,
    decode_image(bytes) {
      calls.push('decode_image')
      return track({ width: 2, height: 2, get rgba() { return new Uint8Array(decodeBytes) } })
    },
    render_rgba8(...a) {
      calls.push('render_rgba8')
      args.render_rgba8.push(a)
      onRender(renders++)
      return new Uint8Array(renderBytes)
    },
    ScoringReference: function ScoringReference(...a) {
      calls.push('ScoringReference')
      args.ScoringReference.push(a)
      refsCreated++
      return track({
        score(...s) {
          calls.push('score')
          args.score.push(s)
          return '{}'
        },
      })
    },
    memory_bytes: () => (memory ? memory({ created, refsCreated }) : created * 1000 + 10000),
    versions: () => '{"engine":"e","scoring":"s"}',
    sha256_hex: (b) => b.length.toString(16).padStart(64, '0'),
  }
  return fake
}

/** A clock that advances by 1 on every read; `advance(n)` adds more. */
const fakeClock = () => {
  let t = 0
  return { now: () => t++, advance: (n) => { t += n } }
}

const fakeInputs = (patch = {}) => ({
  jpeg: new Uint8Array(100),
  answerPng: new Uint8Array(200),
  playerRecipe: '{}',
  regionJson: '{"kind":"full"}',
  seed: 0,
  compositionJson: null,
  ...patch,
})

const opts = (clock = fakeClock()) => ({
  now: clock.now,
  wasmSha256: 'a'.repeat(64),
  harness: { commit: 'c0ffee', sha256: 'b'.repeat(64) },
  kind: 'desktop',
  runId: 'run-1',
})

const count = (calls, name) => calls.filter((c) => c === name).length

test('S1 pct is nearest-rank and rejects empty input', () => {
  const oneTo20 = Array.from({ length: 20 }, (_, i) => i + 1)
  assert.equal(pct([5, 1, 4, 2, 3], 0.5), 3)
  assert.equal(pct(oneTo20, 0.95), 19)
  assert.equal(pct(oneTo20, 0.5), 10)
  assert.throws(() => pct([], 0.5))
})

test('S2 runGate takes the planned samples and frees everything', async () => {
  const fake = fakeWasm()
  const r = await runGate(fake, fakeInputs(), opts())
  assert.equal(fake.live(), 0)
  for (const k of ['total', 'decodeOriginal', 'decodeAnswer', 'reference']) assert.equal(r.load.warm[k].length, 5)
  for (const k of ['render', 'score', 'total']) assert.equal(r.submit[k].length, 20)
  assert.equal(r.switch.ms.length, 10)
  assert.equal(r.switch.wasmBytesAfter.length, 10)
  assert.equal(count(fake.calls, 'ScoringReference'), 17)
  assert.equal(count(fake.calls, 'decode_image'), 34)
  assert.equal(count(fake.calls, 'render_rgba8'), 51)
  assert.equal(count(fake.calls, 'score'), 51)
  assert.deepEqual(validateResult(r), [])
})

test('S2b switchOnce loads, submits three times and frees', () => {
  const fake = fakeWasm()
  switchOnce(fake, fakeInputs())
  assert.equal(fake.live(), 0)
  assert.equal(count(fake.calls, 'ScoringReference'), 1)
  assert.equal(count(fake.calls, 'render_rgba8'), COUNTS.submitsPerSwitch)
  assert.equal(count(fake.calls, 'score'), COUNTS.submitsPerSwitch)
})

test('S3 warm-ups are excluded from recorded samples', async () => {
  const clock = fakeClock()
  // The first render is the submit warm-up: it takes 5 extra ticks.
  const fake = fakeWasm({ onRender: (i) => { if (i === 0) clock.advance(5) } })
  const r = await runGate(fake, fakeInputs(), opts(clock))
  const plain = r.submit.total[1]
  assert.equal(r.submit.total[0], plain)
  assert.notEqual(r.submit.total[0], plain + 5)
  assert.ok(r.submit.total.every((t) => t === plain))
  assert.ok(r.submit.render.every((t) => t === r.submit.render[0]))
})

test('S4 jsAllocTwoRep counts compressed inputs and two consecutive repetitions', async () => {
  const r = await runGate(fakeWasm({ decodeBytes: 1000, renderBytes: 400 }), fakeInputs(), opts())
  assert.equal(r.memory.jsAllocTwoRepBytes, 300 + (2000 + 1200) * 2)
  assert.equal(r.memory.jsAllocTwoRepBytes, 6700)
})

test('S5 wasmPeakBytes is the maximum over stages', async () => {
  // 100000 only while exactly the cold + 5 warm references exist (end of the warm load stage).
  const memory = ({ refsCreated }) => (refsCreated === 1 + COUNTS.loadWarm ? 100000 : 10000)
  const r = await runGate(fakeWasm({ memory }), fakeInputs(), opts())
  assert.equal(r.memory.wasmPeakBytes, 100000)
  assert.ok(r.switch.wasmBytesAfter.every((b) => b === 10000))
})

test('S6 runGate passes region, seed and composition through', async () => {
  const fake = fakeWasm()
  const regionJson = '{"kind":"masks","masks":[]}'
  const compositionJson = '{"answer":null,"player":null}'
  await runGate(fake, fakeInputs({ regionJson, seed: 123456789, compositionJson }), opts())
  assert.ok(fake.args.ScoringReference.every((a) => a[4] === regionJson))
  assert.ok(fake.args.render_rgba8.every((a) => a[4] === 123456789))
  assert.ok(fake.args.score.every((a) => a[1] === compositionJson))
})

test('S7 runGate records the run identity', async () => {
  const o = opts()
  const r = await runGate(fakeWasm(), fakeInputs(), o)
  assert.equal(r.run.inputs.jpeg, '64'.padStart(64, '0'))
  assert.equal(r.run.inputs.answerPng, 'c8'.padStart(64, '0'))
  assert.equal(r.run.inputs.playerRecipe, '2'.padStart(64, '0'))
  assert.equal(r.run.wasmSha256, o.wasmSha256)
  assert.deepEqual(r.run.harness, o.harness)
  assert.equal(r.runId, o.runId)
  assert.equal(r.kind, o.kind)
  assert.deepEqual(r.run.counts, COUNTS)
  assert.equal(r.run.engine, 'e')
  assert.equal(r.run.scoring, 's')
})

const fixtureDir = process.env.BENCH_FIXTURE_DIR
test('S8 runGate on the real nodejs wasm with a tiny image', { skip: fixtureDir ? false : 'BENCH_FIXTURE_DIR not set' }, async () => {
  const pkg = resolve(here, '../../pkg/engine_wasm.js')
  assert.ok(existsSync(pkg), `${pkg} missing: wasm-pack build engine/wasm --target nodejs --release -- --locked`)
  const wasm = await import(pathToFileURL(pkg).href)
  const read = (name) => new Uint8Array(readFileSync(resolve(fixtureDir, name)))
  const inputs = {
    jpeg: read('fixture.jpg'),
    answerPng: read('answer_2048.png'),
    playerRecipe: readFileSync(resolve(here, '../../../golden/recipes/identity.json'), 'utf8'),
    regionJson: '{"kind":"full"}',
    seed: 0,
    compositionJson: null,
  }
  const r = await runGate(wasm, inputs, { ...opts(), now: () => performance.now() })
  assert.deepEqual(validateResult(r), [])
})
