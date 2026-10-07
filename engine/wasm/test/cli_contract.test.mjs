// CLI -> files -> wasm (plan 01a-4 Task 13): the files `engine-cli` wrote in
// engine/target/cli-contract (by `cargo test -p engine-cli --test cli`,
// gen_answer_pngs_and_meta_reproduce_the_scoring_input) must be reproduced by the wasm build:
// decode, answer render, 2048 -> 1024 re-derivation, player render and the Score JSON string.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const dir = resolve(here, '../../target/cli-contract')
const wasm = await import(pathToFileURL(resolve(here, '../pkg/engine_wasm.js')).href)
const bytes = (name) => new Uint8Array(readFileSync(resolve(dir, name)))
const text = (name) => readFileSync(resolve(dir, name), 'utf8')

/** Copy an engine-owned ImageData out once and free it (each `.rgba` read copies). */
const take = (img) => {
  try {
    return { width: img.width, height: img.height, rgba: img.rgba }
  } finally {
    img.free()
  }
}

test('wasm reproduces the CLI gen-answer, render and score files', () => {
  assert.ok(existsSync(resolve(dir, 'score.json')), `${dir} incomplete: run cargo test -p engine-cli --test cli first`)
  const meta = JSON.parse(text('ans/meta.json'))
  const original = take(wasm.decode_image(bytes('fixture.jpg')))
  const { width: w, height: h } = original
  assert.equal(wasm.sha256_hex(original.rgba), meta.original_sha256_rgba8, 'original decode')

  const answer = take(wasm.decode_image(bytes('ans/answer_2048.png')))
  assert.equal(wasm.sha256_hex(answer.rgba), meta.answer_sha256_rgba8_2048, 'answer_2048.png')
  const rendered = wasm.render_rgba8(w, h, original.rgba, text('r.json'), meta.seed)
  assert.equal(wasm.sha256_hex(rendered), meta.answer_sha256_rgba8_2048, 'wasm answer render')

  const small = take(wasm.decode_image(bytes('ans/answer_1024.png')))
  assert.equal(wasm.sha256_hex(small.rgba), meta.answer_sha256_rgba8_1024, 'answer_1024.png')
  const again = take(wasm.scoring_image(w, h, answer.rgba, 1024))
  assert.deepEqual([again.width, again.height], [meta.scoring_width, meta.scoring_height])
  assert.equal(wasm.sha256_hex(again.rgba), meta.answer_sha256_rgba8_1024, '2048 -> 1024 re-derivation')

  const player = take(wasm.decode_image(bytes('player.png')))
  const playerRender = wasm.render_rgba8(w, h, original.rgba, text('p.json'), meta.seed)
  assert.equal(wasm.sha256_hex(playerRender), wasm.sha256_hex(player.rgba), 'player render')

  const reference = new wasm.ScoringReference(w, h, original.rgba, answer.rgba, '{"kind":"full"}')
  try {
    assert.equal(wasm.sha256_hex(take(reference.answer_scoring()).rgba), meta.answer_sha256_rgba8_1024, 'reference scoring image')
    assert.equal(reference.score(player.rgba, null), text('score.json'), 'Score JSON string')
  } finally {
    reference.free()
  }
})
