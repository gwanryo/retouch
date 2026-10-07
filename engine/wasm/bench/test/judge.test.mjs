// Budget judgement (plan 02-1 Task 3, table 3.4 "judge"): judge() in scenario.mjs and the
// judge.mjs CLI.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { COUNTS, judge } from '../scenario.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const budget = JSON.parse(readFileSync(resolve(here, '../budget.json'), 'utf8'))
const MiB = 2 ** 20
const hex = (c) => c.repeat(64)
const fill = (n, v) => Array.from({ length: n }, () => v)

/** A valid result, every item inside the budget of `kind`, then `patch(r)`. */
const resultWith = (patch = () => {}, kind = 'mobile', runId = 'r1') => {
  const t = kind === 'mobile' ? 1000 : 600
  const r = {
    schema: 1,
    kind,
    runId,
    run: {
      engine: 'engine-0.1.0',
      scoring: 'scoring-0.1.0',
      wasmSha256: hex('a'),
      harness: { commit: 'c0ffee', sha256: hex('b') },
      inputs: { jpeg: hex('c'), answerPng: hex('d'), playerRecipe: hex('e') },
      seed: 0,
      regionJson: '{"kind":"full"}',
      compositionJson: null,
      counts: { ...COUNTS },
    },
    load: {
      cold: { total: t, decodeOriginal: 10, decodeAnswer: 10, reference: 10 },
      warm: { total: fill(5, t), decodeOriginal: fill(5, 10), decodeAnswer: fill(5, 10), reference: fill(5, 10) },
    },
    submit: { render: fill(20, 10), score: fill(20, 10), total: fill(20, t) },
    switch: { ms: fill(10, 3000), wasmBytesAfter: fill(10, 100 * MiB) },
    memory: {
      wasmPeakBytes: 100 * MiB,
      jsAllocTwoRepBytes: 50 * MiB,
      residual: { source: 'ua', bytes: fill(10, 100 * MiB) },
    },
    env: {
      userAgent: 'UA',
      hardwareConcurrency: 8,
      deviceMemory: 8,
      isSecureContext: true,
      crossOriginIsolated: true,
      uaMemoryApi: true,
      charging: false,
      device: 'test device',
    },
    incidents: [],
  }
  patch(r)
  return r
}
const three = (patch, kind = 'mobile') => ['r1', 'r2', 'r3'].map((id) => resultWith(patch, kind, id))
const verdict = (results, kind = 'mobile') => judge(results, budget, kind).verdict

const J10 = Array.from({ length: 10 }, (_, i) => (100 + 0.5 * i) * MiB)

test('J0 the baseline result passes', () => {
  assert.equal(verdict(three()), 'PASS')
  assert.equal(verdict(three(() => {}, 'desktop'), 'desktop'), 'PASS')
})

for (const [name, v, want] of [['J1', 1999, 'PASS'], ['J2', 2000, 'PASS'], ['J3', 2001, 'FAIL']]) {
  test(`${name} submit p95 ${v}`, () => {
    assert.equal(verdict(three((r) => { r.submit.total = fill(20, v) })), want)
  })
}

for (const [name, v, want] of [['J4', 3000, 'PASS'], ['J5', 3001, 'FAIL']]) {
  test(`${name} load p95 ${v}`, () => {
    assert.equal(verdict(three((r) => { r.load.warm.total = fill(5, v) })), want)
  })
}

for (const [name, extra, want] of [['J6', 0, 'PASS'], ['J7', 1, 'FAIL']]) {
  test(`${name} peak 300MiB + ${extra}B`, () => {
    const results = three((r) => {
      r.memory.wasmPeakBytes = 250 * MiB
      r.memory.jsAllocTwoRepBytes = 50 * MiB + extra
    })
    assert.equal(verdict(results), want)
  })
}

for (const [name, extra, want] of [['J8', 0, 'PASS'], ['J9', 1, 'FAIL']]) {
  test(`${name} growth exactly 10% + ${extra}B`, () => {
    const bytes = [...fill(9, 100 * MiB), 110 * MiB + extra]
    assert.equal(verdict(three((r) => { r.memory.residual.bytes = bytes })), want)
  })
}

test('J10 strictly increasing residual', () => {
  const { verdict: v, lines } = judge(three((r) => { r.memory.residual.bytes = J10 }), budget, 'mobile')
  assert.equal(v, 'FAIL')
  assert.ok(lines.some((l) => /^FAIL residual_monotonic/.test(l)), lines.join('\n'))
})

test('J11 increasing with one equal step', () => {
  const bytes = [...J10]
  bytes[4] = bytes[3]
  assert.equal(verdict(three((r) => { r.memory.residual.bytes = bytes })), 'PASS')
})

test('J12 wasm growth also judged', () => {
  assert.equal(verdict(three((r) => { r.switch.wasmBytesAfter = J10 })), 'FAIL')
})

test('J13 desktop peak is INFO only', () => {
  const results = three((r) => {
    r.memory.wasmPeakBytes = 900 * MiB
    r.memory.jsAllocTwoRepBytes = 100 * MiB
  }, 'desktop')
  const { verdict: v, lines } = judge(results, budget, 'desktop')
  assert.equal(v, 'PASS')
  assert.ok(lines.some((l) => /^INFO peak/.test(l)), lines.join('\n'))
})

test('J13b desktop submit budget', () => {
  assert.equal(verdict(three((r) => { r.submit.total = fill(20, 701) }, 'desktop'), 'desktop'), 'FAIL')
})

test('J14 residual missing -> INCOMPLETE', () => {
  const results = three()
  results[1].memory.residual = { source: null, reason: 'timeout' }
  assert.equal(verdict(results), 'INCOMPLETE')
})

test('J15 manual residual is complete', () => {
  const results = three((r) => {
    r.memory.residual = { source: 'manual', bytes: fill(10, 40 * MiB), note: 'DevTools heap after GC' }
  })
  assert.equal(verdict(results), 'PASS')
})

test('J16 fewer than 3 runs -> INCOMPLETE', () => {
  assert.equal(verdict(three().slice(0, 2)), 'INCOMPLETE')
})

test('J17 failure wins over incomplete', () => {
  const results = three().slice(0, 2)
  results[1].submit.total = fill(20, 2001)
  assert.equal(verdict(results), 'FAIL')
})

test('J18 one failing run among passing -> FAIL', () => {
  const results = three()
  results[1].submit.total = fill(20, 2001)
  assert.equal(verdict(results), 'FAIL')
})

test('J19 incident or crash record -> FAIL', () => {
  const withIncident = three()
  withIncident[0].incidents = ['reload']
  assert.equal(verdict(withIncident), 'FAIL')
  const withCrash = [...three().slice(0, 2), { schema: 1, kind: 'mobile', crashed: true, note: 'tab reloaded' }]
  assert.equal(verdict(withCrash), 'FAIL')
})

test('J20 mixed runs -> FAIL', () => {
  const cases = {
    'inputs.jpeg': (r) => { r.run.inputs.jpeg = hex('f') },
    'harness.sha256': (r) => { r.run.harness.sha256 = hex('f') },
    'harness.commit': (r) => { r.run.harness.commit = 'other' },
    'env.device': (r) => { r.env.device = 'other device' },
    'env.charging': (r) => { r.env.charging = true },
  }
  for (const [name, patch] of Object.entries(cases)) {
    const results = three()
    patch(results[2])
    assert.equal(verdict(results), 'FAIL', name)
  }
  assert.equal(verdict(three(), 'desktop'), 'FAIL', 'cli kind desktop, mobile results')
  const dup = three()
  dup[2].runId = dup[0].runId
  assert.equal(verdict(dup), 'FAIL', 'duplicate runId')
})

test('J21 invalid results -> FAIL', () => {
  const cases = {
    'submit.total 19': (r) => { r.submit.total = fill(19, 1000) },
    'counts.submit 1 with 1 sample': (r) => {
      r.run.counts.submit = 1
      r.submit.total = [1000]
      r.submit.render = [10]
      r.submit.score = [10]
    },
    'NaN time': (r) => { r.submit.total[3] = NaN },
    'negative time': (r) => { r.load.warm.total[0] = -1 },
    'wasmPeakBytes 0': (r) => { r.memory.wasmPeakBytes = 0 },
    'bytes 1.5': (r) => { r.memory.jsAllocTwoRepBytes = 1.5 },
    'residual ua 9': (r) => { r.memory.residual.bytes = fill(9, 100 * MiB) },
    'residual ua null': (r) => { r.memory.residual.bytes = null },
    'inputs.jpeg h100': (r) => { r.run.inputs.jpeg = 'h100' },
    'no runId': (r) => { delete r.runId },
    'no run': (r) => { delete r.run },
  }
  for (const [name, patch] of Object.entries(cases)) {
    const results = three()
    patch(results[0])
    const { verdict: v, lines } = judge(results, budget, 'mobile')
    assert.equal(v, 'FAIL', name)
    assert.ok(lines.some((l) => /^FAIL format/.test(l)), `${name}: ${lines.join('\n')}`)
  }
})

test('J22 judge.mjs exit codes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'judge-'))
  const write = (name, value) => {
    const p = join(dir, name)
    writeFileSync(p, typeof value === 'string' ? value : JSON.stringify(value))
    return p
  }
  const pass = three().map((r, i) => write(`pass-${i}.json`, r))
  const fail = three((r) => { r.submit.total = fill(20, 2001) }).map((r, i) => write(`fail-${i}.json`, r))
  const broken = write('broken.json', '{"schema": 1,')
  const cli = resolve(here, '../judge.mjs')
  const run = (...files) => spawnSync(process.execPath, [cli, 'mobile', ...files], { encoding: 'utf8' })
  const cases = [
    [pass, 0, 'GATE PASS'],
    [fail, 1, 'GATE FAIL'],
    [pass.slice(0, 2), 2, 'GATE INCOMPLETE'],
    [[...pass.slice(0, 2), broken], 1, 'GATE FAIL'],
  ]
  for (const [files, code, last] of cases) {
    const p = run(...files)
    assert.equal(p.status, code, p.stdout + p.stderr)
    assert.equal(p.stdout.trim().split('\n').at(-1), last)
  }
})
