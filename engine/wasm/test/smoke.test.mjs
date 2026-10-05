// Early cross-target smoke (plan 01a-3 Task 10): the wasm build must reproduce the native
// results written by `cargo test -p engine-core --test smoke`. Run that first, then:
//   node --test "engine/wasm/test/*.test.mjs"
// Every `ImageData` and `ScoringReference` owns wasm memory: read `.rgba` once (each read
// copies) and `free()` in `finally`.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const smokeDir = resolve(here, '../../target/smoke')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const read = (name) => new Uint8Array(readFileSync(resolve(smokeDir, name)))

/** Decode with the engine, copy the pixels out once and release the wasm object. */
const decode = (bytes) => {
  const img = wasm.decode_image(bytes)
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

test('wasm reference path equals native smoke output', () => {
  const nativePath = resolve(smokeDir, 'native.json')
  assert.ok(existsSync(nativePath), `${nativePath} missing: run cargo test -p engine-core --test smoke first`)
  const native = JSON.parse(readFileSync(nativePath, 'utf8'))
  const original = decode(read('input.jpg'))
  assert.equal(wasm.sha256_hex(original.rgba), native.original_sha256, 'jpeg decode')

  const { width: w, height: h } = original
  const rendered = wasm.render_rgba8(w, h, original.rgba, native.answer_recipe, native.seed)
  assert.equal(wasm.sha256_hex(rendered), native.answer_sha256, 'render')
  const answer = decode(read('answer.png'))
  assert.equal(wasm.sha256_hex(answer.rgba), native.answer_sha256, 'answer PNG decode')
  const player = wasm.render_rgba8(w, h, original.rgba, native.player_recipe, native.seed)
  assert.equal(wasm.sha256_hex(player), native.player_sha256, 'player render')

  const reference = new wasm.ScoringReference(w, h, original.rgba, answer.rgba, '{"kind":"full"}')
  try {
    const scoring = reference.answer_scoring()
    try {
      assert.equal(wasm.sha256_hex(scoring.rgba), native.answer_scoring_sha256, 'scoring image')
    } finally {
      scoring.free()
    }
    assert.equal(reference.score(player, null), native.score_json, 'score JSON string')
  } finally {
    reference.free()
  }
})

test('wasm rejects zero, mismatched and oversized dimensions with the engine error', () => {
  // The patterns are the engine's own messages, so a wasm trap (RuntimeError) cannot pass.
  assert.throws(() => wasm.scoring_image(0, 4, new Uint8Array(0), 1024), /^Error: empty image: 0x4$/)
  assert.throws(() => wasm.scoring_image(2, 2, new Uint8Array(15), 1024), /^Error: rgba buffer has 15 bytes, expected 16 for 2x2$/)
  assert.throws(() => wasm.render_rgba8(100000, 100000, new Uint8Array(4), '{"schema_version":1}', 0), /^Error: image too large: 100000x100000 /)
  assert.throws(() => new wasm.ScoringReference(0, 0, new Uint8Array(0), new Uint8Array(0), '{"kind":"full"}'), /^Error: empty image: 0x0$/)
  assert.throws(() => new wasm.ScoringReference(1, 1, new Uint8Array(4), new Uint8Array(4), '{"kind":"masks"}'), /^Error: region json: /)
  assert.throws(() => wasm.render_rgba8(1, 1, new Uint8Array(4), '{"schema_version":1,"basic":{"exposure":1e39}}', 0), /^Error: field `basic.exposure` is not a finite number$/)
})

test('ScoringReference rejects a wrong-size player and composition boxes in Plan 1a', () => {
  const px = new Uint8Array(4 * 4 * 4).fill(128)
  const reference = new wasm.ScoringReference(4, 4, px, px, '{"kind":"full"}')
  try {
    assert.throws(() => reference.score(new Uint8Array(4 * 4 * 3), null), /^Error: rgba buffer has 48 bytes, expected 64 for 4x4$/)
    assert.throws(() => reference.score(px, '{"answer":{"x":0,"y":0,"w":1,"h":1},"player":null}'), /^Error: `composition` is scored from Plan 1b$/)
    assert.equal(JSON.parse(reference.score(px, '{"answer":null,"player":null}')).eligible, false)
  } finally {
    reference.free()
  }
})

test('wasm handles 1x1, 1xN and Nx1 images', () => {
  for (const [w, h] of [[1, 1], [1, 9], [9, 1]]) {
    const rgba = new Uint8Array(w * h * 4).fill(128)
    const out = wasm.render_rgba8(w, h, rgba, '{"schema_version":1,"basic":{"exposure":0.5}}', 0)
    assert.equal(out.length, w * h * 4)
    const small = wasm.scoring_image(w, h, out, 1024)
    try {
      assert.deepEqual([small.width, small.height], [w, h])
    } finally {
      small.free()
    }
  }
})

test('ImageData owns wasm memory: rgba reads copy, free() releases it', () => {
  const img = wasm.scoring_image(2, 2, new Uint8Array(16).fill(9), 1024)
  assert.ok(img instanceof wasm.ImageData)
  assert.notStrictEqual(img.rgba, img.rgba, 'each rgba read is a fresh copy')
  img.free()
  assert.throws(() => img.width, /null pointer passed to rust/)
})

test('ImageData fields are read-only accessors (no JS setters)', () => {
  for (const name of ['width', 'height', 'rgba']) {
    const d = Object.getOwnPropertyDescriptor(wasm.ImageData.prototype, name)
    assert.ok(d && typeof d.get === 'function', `${name} has a getter`)
    assert.strictEqual(d.set, undefined, `${name} has no setter`)
  }
})

test('validate_recipe reports clamped fields with mask-indexed paths', () => {
  const json = '{"schema_version":1,"basic":{"exposure":9},"masks":[{"kind":"radial","cx":0.5,"cy":0.5,"rx":0.2,"ry":0.2,"feather":0.5,"adjust":{"tint":-300}}]}'
  const report = JSON.parse(wasm.validate_recipe(json))
  assert.deepEqual(report.normalizations, [
    { path: 'basic.exposure', from: 9, to: 5, reason: 'clamped' },
    { path: 'masks[0].adjust.tint', from: -300, to: -100, reason: 'clamped' },
  ])
  assert.equal(report.recipe.basic.exposure, 5)
})
