// wasm half of AC-E1a: the wasm build must reproduce engine/golden/expected.json exactly
// (hashes of every render and scoring image, and full Score JSON strings). On any failure the
// wasm result (mismatch) or `{"failure":{case,player,stage,error}}` (exception, including an
// unreadable expected.json) is written to engine/golden/actual-wasm.json for the CI artifact.
// Never copy it into expected.json.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const golden = resolve(here, '../../golden')
const actualPath = resolve(golden, 'actual-wasm.json')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (p) => readFileSync(resolve(golden, p), 'utf8')
const FULL = '{"kind":"full"}'

class GoldenFailure extends Error {
  constructor(failure) {
    super(`case ${failure.case ?? '-'} / player ${failure.player ?? '-'} / ${failure.stage}: ${failure.error}`)
    this.failure = failure
  }
}

/** Run `f`; on an exception, rethrow it with its {case, player, stage} context. */
const at = (ctx, stage, f) => {
  try {
    return f()
  } catch (err) {
    if (err instanceof GoldenFailure) throw err
    throw new GoldenFailure({ case: ctx.case ?? null, player: ctx.player ?? null, stage, error: String(err?.message ?? err) })
  }
}

/** Copy an engine-owned ImageData out once and free it (each `.rgba` read copies). */
const take = (img) => {
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

const sha = (bytes) => wasm.sha256_hex(bytes)
const scoringSha = (w, h, rgba) => sha(take(wasm.scoring_image(w, h, rgba, 1024)).rgba)

/** Same integer formula as `probe_rgba8` in engine/cli/src/golden.rs. */
function probeRgba(p, role) {
  const { width: w, height: h } = p
  const out = new Uint8Array(w * h * 4)
  let i = 0
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const answer = [
        Math.floor((x * 255) / Math.max(w - 1, 1)),
        Math.floor((y * 255) / Math.max(h - 1, 1)),
        ((Math.floor(x / 16) + Math.floor(y / 16)) % 2) * 160 + 48,
      ]
      for (let c = 0; c < 3; c++) {
        const a = answer[c]
        let v = a
        if (role === 'original') v = c === 1 ? a - p.shift : a + p.shift
        if (role === 'player') v = a + ((x * 31 + y * 17 + c * 7) % (2 * p.noise + 1)) - p.noise
        out[i++] = Math.min(255, Math.max(0, v))
      }
      out[i++] = 255
    }
  }
  return out
}

function computeCase(c, src) {
  const ctx = { case: c.name }
  const { width: w, height: h, rgba } = src
  const render = (pctx, recipe) =>
    at(pctx, 'render', () => wasm.render_rgba8(w, h, rgba, read(recipe), c.seed))
  const answer = render({ ...ctx, player: 'answer' }, c.answer)
  const reference = at(ctx, 'scoring reference', () => new wasm.ScoringReference(w, h, rgba, answer, FULL))
  try {
    const players = {}
    const submit = (player, pixels) => {
      const pctx = { ...ctx, player }
      players[player] = {
        render_sha256: sha(pixels),
        scoring_sha256: at(pctx, 'scoring image', () => scoringSha(w, h, pixels)),
        score_json: at(pctx, 'score', () => reference.score(pixels, null)),
      }
    }
    submit('answer', answer)
    submit('original', rgba)
    for (const [player, p] of Object.entries(c.players)) submit(player, render({ ...ctx, player }, p.recipe))
    const a = take(reference.answer_scoring())
    return {
      answer_sha256: sha(answer),
      answer_scoring_sha256: sha(a.rgba),
      scoring_width: a.width,
      scoring_height: a.height,
      players: Object.fromEntries(Object.entries(players).sort(([x], [y]) => (x < y ? -1 : 1))),
    }
  } finally {
    reference.free()
  }
}

function computeProbe(p) {
  const ctx = { player: p.name }
  const [original, answer, player] = ['original', 'answer', 'player'].map((role) => probeRgba(p, role))
  const reference = at(ctx, 'scoring reference', () => new wasm.ScoringReference(p.width, p.height, original, answer, FULL))
  try {
    const a = take(reference.answer_scoring())
    return {
      scoring_width: a.width,
      scoring_height: a.height,
      answer_scoring_sha256: sha(a.rgba),
      player_scoring_sha256: at(ctx, 'scoring image', () => scoringSha(p.width, p.height, player)),
      score_json: at(ctx, 'score', () => reference.score(player, null)),
    }
  } finally {
    reference.free()
  }
}

function compute(spec) {
  const v = JSON.parse(wasm.versions())
  const out = {
    engine_version: v.engine,
    scoring_version: v.scoring,
    schema_version: v.schema,
    scoring_params: v.scoring_params,
    long_edge: v.scoring_params.long_edge,
    images: {},
    cases: {},
    probes: {},
  }
  const decoded = {}
  for (const [name, file] of Object.entries(spec.images)) {
    const img = at({}, 'decode image', () => take(wasm.decode_image(new Uint8Array(readFileSync(resolve(golden, file))))))
    decoded[name] = img
    out.images[name] = { width: img.width, height: img.height, sha256_rgba8: sha(img.rgba) }
  }
  for (const c of spec.cases) out.cases[c.name] = computeCase(c, decoded[c.image])
  for (const p of spec.probes) out.probes[p.name] = computeProbe(p)
  // Key order of expected.json (serde BTreeMap): sorted.
  for (const k of ['images', 'cases', 'probes']) out[k] = Object.fromEntries(Object.entries(out[k]).sort(([x], [y]) => (x < y ? -1 : 1)))
  return out
}

test('wasm reproduces engine/golden/expected.json exactly', () => {
  let actual
  try {
    const spec = at({}, 'read cases.json', () => JSON.parse(read('cases.json')))
    actual = compute(spec)
    const expected = at({}, 'read expected.json', () => JSON.parse(read('expected.json')))
    assert.deepStrictEqual(actual, expected)
  } catch (err) {
    const report = err instanceof GoldenFailure ? { failure: err.failure } : actual ?? { failure: { case: null, player: null, stage: 'unknown', error: String(err) } }
    writeFileSync(actualPath, JSON.stringify(report, null, 2) + '\n')
    throw err
  }
})
