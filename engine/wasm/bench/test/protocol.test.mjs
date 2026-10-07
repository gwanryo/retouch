// Page/Worker protocol helpers (plan 02-1 Task 3, table 3.4 "protocol"). web/protocol.mjs has
// no browser globals and no wasm import, so node can test it directly.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { answerProbe, awaitProbe, loadInputs, probeMemory } from '../web/protocol.mjs'

const okEnv = (measure) => ({ isSecureContext: true, crossOriginIsolated: true, measure })

/** fetch stub: 200 with small bodies unless the URL is in `missing`. */
const fetchStub = (missing = []) => async (url) => {
  if (missing.includes(url)) return new Response('not found', { status: 404 })
  if (url.endsWith('.json')) return new Response('{"schema_version":1}', { status: 200 })
  return new Response(new Uint8Array([1, 2, 3]), { status: 200 })
}

test('P1 loadInputs reports a missing input with the recovery command', async () => {
  await assert.rejects(loadInputs(fetchStub(['/inputs/answer.png'])), (e) => {
    assert.match(e.message, /\/inputs\/answer\.png 404/)
    assert.match(e.message, /engine-cli -- gen-answer/)
    return true
  })
})

test('P2 loadInputs returns bytes and text', async () => {
  const r = await loadInputs(fetchStub())
  assert.ok(r.jpeg instanceof Uint8Array)
  assert.ok(r.answerPng instanceof Uint8Array)
  assert.deepEqual([...r.jpeg], [1, 2, 3])
  assert.equal(typeof r.playerRecipe, 'string')
  assert.equal(r.playerRecipe, '{"schema_version":1}')
})

test('P3 probeMemory without API / isolation / secure context', async () => {
  const measure = async () => ({ bytes: 1 })
  assert.deepEqual(await probeMemory(okEnv(undefined)), { bytes: null, reason: 'measure' })
  assert.deepEqual(await probeMemory({ ...okEnv(measure), crossOriginIsolated: false }), { bytes: null, reason: 'crossOriginIsolated' })
  assert.deepEqual(await probeMemory({ ...okEnv(measure), isSecureContext: false }), { bytes: null, reason: 'isSecureContext' })
  assert.deepEqual(await probeMemory(okEnv(async () => ({ bytes: 12345 }))), { bytes: 12345, reason: null })
})

test('P4 probeMemory on SecurityError', async () => {
  const measure = () => Promise.reject(new DOMException('x', 'SecurityError'))
  assert.deepEqual(await probeMemory(okEnv(measure)), { bytes: null, reason: 'SecurityError' })
  const throwing = () => { throw new DOMException('x', 'SecurityError') }
  assert.deepEqual(await probeMemory(okEnv(throwing)), { bytes: null, reason: 'SecurityError' })
})

test('P5 probeMemory times out', async () => {
  const measure = () => new Promise(() => {})
  assert.deepEqual(await probeMemory(okEnv(measure), 10), { bytes: null, reason: 'timeout' })
})

test('P6 answerProbe posts exactly once per id', async () => {
  const cases = {
    success: async () => ({ bytes: 777 }),
    failure: () => Promise.reject(new DOMException('x', 'SecurityError')),
    timeout: () => new Promise(() => {}),
  }
  for (const [name, measure] of Object.entries(cases)) {
    const posted = []
    await answerProbe({ type: 'probe', id: name }, okEnv(measure), (m) => posted.push(m), 10)
    await new Promise((r) => setTimeout(r, 30))
    assert.equal(posted.length, 1, name)
    assert.equal(posted[0].type, 'probe-result', name)
    assert.equal(posted[0].id, name, name)
    assert.ok('bytes' in posted[0] && 'reason' in posted[0], name)
  }
})

test('P7 awaitProbe ignores other ids', async () => {
  const listeners = new Set()
  const onMessage = (l) => {
    listeners.add(l)
    return () => listeners.delete(l)
  }
  const deliver = (m) => { for (const l of [...listeners]) l(m) }
  const posted = []
  const p = awaitProbe((m) => posted.push(m), onMessage, 7)
  assert.deepEqual(posted, [{ type: 'probe', id: 7 }])
  deliver({ type: 'probe-result', id: 6, bytes: 1, reason: null })
  deliver({ type: 'progress', text: 'x' })
  deliver({ type: 'probe-result', id: 7, bytes: 2, reason: null })
  assert.deepEqual(await p, { bytes: 2, reason: null })
  assert.equal(listeners.size, 0)
})

test('P8 awaitProbe gives up when the page never answers', async () => {
  const listeners = new Set()
  const onMessage = (l) => {
    listeners.add(l)
    return () => listeners.delete(l)
  }
  const r = await awaitProbe(() => {}, onMessage, 1, 10)
  assert.deepEqual(r, { bytes: null, reason: 'no-answer' })
  assert.equal(listeners.size, 0)
})
