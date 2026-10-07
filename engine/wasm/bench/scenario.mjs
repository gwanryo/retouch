// Performance gate scenario shared by node (perf.mjs) and the browser Worker (web/worker.mjs),
// plus result validation and the budget judgement (plan 02-1 Task 3.1, master plan §4.1).
// No node or browser globals: runs unchanged in both.
//
// Memory policy (index "user decision 2", option A): wasmPeakBytes is the maximum of
// wasm.memory_bytes() read at the end of every stage (exact, linear memory only grows).
// jsAllocTwoRepBytes = compressed input bytes + the maximum, over stages, of the JS buffers
// allocated by two consecutive repetitions (k-1, k) of the same stage. It is a policy figure:
// it assumes the GC may not have reclaimed the previous repetition's buffers but has reclaimed
// anything older, and does not bound the real peak (if the GC skips three or more repetitions
// the real peak is larger). The peak used by the gate is wasmPeakBytes + jsAllocTwoRepBytes.

export const COUNTS = Object.freeze({ loadWarm: 5, submit: 20, switch: 10, submitsPerSwitch: 3 })

const MiB = 2 ** 20

/** Nearest rank: sorted[max(1, ceil(p * n)) - 1]. */
export const pct = (xs, p) => {
  if (!xs.length) throw new Error('pct of an empty sample')
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.max(1, Math.ceil(p * s.length)) - 1]
}

/** Per-repetition recorder: labelled durations and the JS buffer bytes this repetition made. */
const recorder = (now) => ({
  times: {},
  allocBytes: 0,
  time(label, f) {
    const t0 = now()
    const r = f()
    ;(this.times[label] ??= []).push(now() - t0)
    return r
  },
})

/** Decode with the engine, copy the pixels out once, free the wasm object. */
const decode = (wasm, bytes, rec) => {
  const img = wasm.decode_image(bytes)
  try {
    const out = { width: img.width, height: img.height, rgba: img.rgba }
    rec.allocBytes += out.rgba.byteLength
    return out
  } finally {
    img.free()
  }
}

/** Challenge load (A8). The caller frees `reference`. */
const load = (wasm, inputs, rec) =>
  rec.time('load.total', () => {
    const src = rec.time('load.decodeOriginal', () => decode(wasm, inputs.jpeg, rec))
    const answer = rec.time('load.decodeAnswer', () => decode(wasm, inputs.answerPng, rec))
    const reference = rec.time('load.reference', () =>
      new wasm.ScoringReference(src.width, src.height, src.rgba, answer.rgba, inputs.regionJson))
    return { src, reference }
  })

const submit = (wasm, inputs, { src, reference }, rec) =>
  rec.time('submit.total', () => {
    const player = rec.time('submit.render', () =>
      wasm.render_rgba8(src.width, src.height, src.rgba, inputs.playerRecipe, inputs.seed))
    rec.allocBytes += player.byteLength
    return rec.time('submit.score', () => reference.score(player, inputs.compositionJson))
  })

const withChallenge = (wasm, inputs, rec, f) => {
  const challenge = load(wasm, inputs, rec)
  try {
    return f(challenge)
  } finally {
    challenge.reference.free()
  }
}

/** One challenge switch: load, submit three times, free everything. */
export const switchOnce = (wasm, inputs, rec = recorder(() => 0)) =>
  withChallenge(wasm, inputs, rec, (challenge) => {
    for (let i = 0; i < COUNTS.submitsPerSwitch; i++) submit(wasm, inputs, challenge, rec)
  })

/** Largest sum of two consecutive entries (the single entry when there is one). */
const twoRep = (xs) => {
  let best = xs[0] ?? 0
  for (let k = 1; k < xs.length; k++) best = Math.max(best, xs[k - 1] + xs[k])
  return best
}

const sha256Text = (wasm, text) => wasm.sha256_hex(new TextEncoder().encode(text))

/**
 * Time pass of the gate. `inputs = { jpeg, answerPng, playerRecipe, regionJson, seed,
 * compositionJson }`, `opts = { now, wasmSha256, harness, kind, runId }`. The caller fills
 * `env` and replaces the placeholder `memory.residual` after its residual pass.
 */
export const runGate = async (wasm, inputs, opts) => {
  const { now } = opts
  const versions = JSON.parse(wasm.versions())
  const run = {
    engine: versions.engine,
    scoring: versions.scoring,
    wasmSha256: opts.wasmSha256,
    harness: opts.harness,
    inputs: {
      jpeg: wasm.sha256_hex(inputs.jpeg),
      answerPng: wasm.sha256_hex(inputs.answerPng),
      playerRecipe: sha256Text(wasm, inputs.playerRecipe),
    },
    seed: inputs.seed,
    regionJson: inputs.regionJson,
    compositionJson: inputs.compositionJson,
    counts: { ...COUNTS },
  }

  let wasmPeakBytes = 0
  const readMemory = () => {
    const b = wasm.memory_bytes()
    wasmPeakBytes = Math.max(wasmPeakBytes, b)
    return b
  }
  let stageTwoRepMax = 0
  /** Run `body` n times with a fresh recorder each; end-of-stage memory and alloc bookkeeping. */
  const stage = (n, body) => {
    const recs = []
    for (let i = 0; i < n; i++) {
      const rec = recorder(now)
      body(rec)
      recs.push(rec)
    }
    stageTwoRepMax = Math.max(stageTwoRepMax, twoRep(recs.map((r) => r.allocBytes)))
    readMemory()
    return recs
  }
  const series = (recs, label) => recs.map((r) => r.times[label][0])
  const loadOnly = (rec) => withChallenge(wasm, inputs, rec, () => {})

  const [cold] = stage(1, loadOnly)
  const warm = stage(COUNTS.loadWarm, loadOnly)

  // Submit warm-up: a fresh load and one unrecorded submit; the 20 samples reuse its reference.
  let challenge
  stage(1, (rec) => {
    challenge = load(wasm, inputs, rec)
    try {
      submit(wasm, inputs, challenge, rec)
    } catch (e) {
      challenge.reference.free()
      throw e
    }
  })
  let submits
  try {
    submits = stage(COUNTS.submit, (rec) => submit(wasm, inputs, challenge, rec))
  } finally {
    challenge.reference.free()
  }

  const switchMs = []
  const wasmBytesAfter = []
  stage(COUNTS.switch, (rec) => {
    const t0 = now()
    switchOnce(wasm, inputs, rec)
    switchMs.push(now() - t0)
    wasmBytesAfter.push(readMemory())
  })

  const loadKeys = { total: 'load.total', decodeOriginal: 'load.decodeOriginal', decodeAnswer: 'load.decodeAnswer', reference: 'load.reference' }
  const pick = (recs) => Object.fromEntries(Object.entries(loadKeys).map(([k, label]) => [k, series(recs, label)]))
  return {
    schema: 1,
    kind: opts.kind,
    runId: opts.runId,
    run,
    load: {
      cold: Object.fromEntries(Object.entries(pick([cold])).map(([k, v]) => [k, v[0]])),
      warm: pick(warm),
    },
    submit: {
      render: series(submits, 'submit.render'),
      score: series(submits, 'submit.score'),
      total: series(submits, 'submit.total'),
    },
    switch: { ms: switchMs, wasmBytesAfter },
    memory: {
      wasmPeakBytes,
      jsAllocTwoRepBytes: inputs.jpeg.byteLength + inputs.answerPng.byteLength + stageTwoRepMax,
      residual: { source: null, reason: 'not measured' },
    },
    env: {},
    incidents: [],
  }
}

// ---------------------------------------------------------------------------------------------
// Validation

const HEX64 = /^[0-9a-f]{64}$/
const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v)
const isTime = (v) => typeof v === 'number' && Number.isFinite(v) && v >= 0
const isBytes = (v) => Number.isInteger(v) && v > 0

const deepEqual = (a, b) => {
  if (a === b) return true
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return Number.isNaN(a) && Number.isNaN(b)
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const ka = Object.keys(a)
  const kb = Object.keys(b)
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && deepEqual(a[k], b[k]))
}

/** Format errors of a GateResult or CrashRecord; [] when valid. */
export const validateResult = (r) => {
  const errors = []
  const need = (ok, msg) => { if (!ok) errors.push(msg) }
  if (!isObject(r)) return ['not an object']
  need(r.schema === 1, 'schema must be 1')
  need(r.kind === 'mobile' || r.kind === 'desktop', 'kind must be mobile or desktop')
  if (r.crashed === true) {
    need(typeof r.note === 'string', 'crash record needs a note')
    return errors
  }
  need(typeof r.runId === 'string' && r.runId.length > 0, 'runId missing')

  const run = r.run
  if (!isObject(run)) {
    errors.push('run missing')
  } else {
    for (const k of ['engine', 'scoring', 'wasmSha256', 'harness', 'inputs', 'seed', 'regionJson', 'compositionJson', 'counts']) {
      need(Object.hasOwn(run, k), `run.${k} missing`)
    }
    need(run.engine != null && run.scoring != null, 'run.engine/scoring missing')
    need(HEX64.test(run.wasmSha256), 'run.wasmSha256 not a sha256')
    need(isObject(run.harness) && typeof run.harness.commit === 'string' && HEX64.test(run.harness.sha256), 'run.harness invalid')
    for (const k of ['jpeg', 'answerPng', 'playerRecipe']) need(isObject(run.inputs) && HEX64.test(run.inputs[k]), `run.inputs.${k} not a sha256`)
    need(Number.isInteger(run.seed) && run.seed >= 0, 'run.seed invalid')
    need(typeof run.regionJson === 'string', 'run.regionJson invalid')
    need(run.compositionJson === null || typeof run.compositionJson === 'string', 'run.compositionJson invalid')
    need(deepEqual(run.counts, { ...COUNTS }), 'run.counts differ from COUNTS')
  }

  const times = (xs, n, name) => need(Array.isArray(xs) && xs.length === n && xs.every(isTime), `${name} must be ${n} finite times >= 0`)
  const bytes = (xs, n, name) => need(Array.isArray(xs) && xs.length === n && xs.every(isBytes), `${name} must be ${n} positive integers`)
  const loadKeys = ['total', 'decodeOriginal', 'decodeAnswer', 'reference']
  if (!isObject(r.load) || !isObject(r.load.cold) || !isObject(r.load.warm)) {
    errors.push('load missing')
  } else {
    for (const k of loadKeys) {
      need(isTime(r.load.cold[k]), `load.cold.${k} must be a finite time >= 0`)
      times(r.load.warm[k], COUNTS.loadWarm, `load.warm.${k}`)
    }
  }
  if (!isObject(r.submit)) errors.push('submit missing')
  else for (const k of ['render', 'score', 'total']) times(r.submit[k], COUNTS.submit, `submit.${k}`)
  if (!isObject(r.switch)) {
    errors.push('switch missing')
  } else {
    times(r.switch.ms, COUNTS.switch, 'switch.ms')
    bytes(r.switch.wasmBytesAfter, COUNTS.switch, 'switch.wasmBytesAfter')
  }

  const m = r.memory
  if (!isObject(m)) {
    errors.push('memory missing')
  } else {
    need(isBytes(m.wasmPeakBytes), 'memory.wasmPeakBytes must be a positive integer')
    need(isBytes(m.jsAllocTwoRepBytes), 'memory.jsAllocTwoRepBytes must be a positive integer')
    const res = m.residual
    if (!isObject(res)) errors.push('memory.residual missing')
    else if (res.source === 'ua') bytes(res.bytes, COUNTS.switch, 'memory.residual.bytes')
    else if (res.source === 'manual') {
      bytes(res.bytes, COUNTS.switch, 'memory.residual.bytes')
      need(typeof res.note === 'string', 'memory.residual.note missing')
    } else if (res.source === null) need(typeof res.reason === 'string', 'memory.residual.reason missing')
    else errors.push('memory.residual.source must be ua, manual or null')
  }
  need(isObject(r.env), 'env missing')
  need(Array.isArray(r.incidents) && r.incidents.every((s) => typeof s === 'string'), 'incidents must be strings')
  return errors
}

// ---------------------------------------------------------------------------------------------
// Judgement

const fmt = (v) => (Number.isInteger(v) ? String(v) : v.toFixed(4))

/**
 * Judge results of one kind against budget.json. Order is fixed; the first stage that fails
 * decides: format, crash/incident, mixed runs, budgets, completeness. Lines are
 * `PASS|FAIL|INFO <item> <value> <limit> run<k>`.
 */
export const judge = (results, budget, kind) => {
  const lines = []
  const tag = (i) => `run${i + 1}`

  results.forEach((r, i) => {
    for (const e of validateResult(r)) lines.push(`FAIL format ${e.replaceAll(' ', '_')} - ${tag(i)}`)
  })
  if (lines.length) return { verdict: 'FAIL', lines }

  results.forEach((r, i) => {
    if (r.crashed === true) lines.push(`FAIL crash ${JSON.stringify(r.note)} - ${tag(i)}`)
    else for (const s of r.incidents) lines.push(`FAIL incident ${JSON.stringify(s)} - ${tag(i)}`)
  })
  if (lines.length) return { verdict: 'FAIL', lines }

  const b = budget?.[kind]
  if (!b) return { verdict: 'FAIL', lines: [`FAIL kind ${kind} no_budget -`] }
  const first = results[0]
  const seen = new Set()
  results.forEach((r, i) => {
    if (r.kind !== kind) lines.push(`FAIL kind ${r.kind} ${kind} ${tag(i)}`)
    if (seen.has(r.runId)) lines.push(`FAIL duplicate-run ${r.runId} - ${tag(i)}`)
    seen.add(r.runId)
    if (i === 0) return
    if (!deepEqual(r.run, first.run)) lines.push(`FAIL mixed-runs run - ${tag(i)}`)
    for (const k of ['device', 'userAgent', 'charging']) {
      if (!deepEqual(r.env[k], first.env[k])) lines.push(`FAIL mixed-runs env.${k} - ${tag(i)}`)
    }
  })
  if (lines.length) return { verdict: 'FAIL', lines }

  let failed = false
  const check = (item, value, limit, i) => {
    if (limit === null || limit === undefined) {
      lines.push(`INFO ${item} ${fmt(value)} - ${tag(i)}`)
      return
    }
    const ok = value <= limit
    if (!ok) failed = true
    lines.push(`${ok ? 'PASS' : 'FAIL'} ${item} ${fmt(value)} ${fmt(limit)} ${tag(i)}`)
  }
  const growth = (name, xs, i) => {
    check(`${name}_growth`, (xs[xs.length - 1] - xs[0]) / xs[0], b.growth_ratio, i)
    const increasing = xs.every((x, k) => k === 0 || x > xs[k - 1])
    if (increasing) failed = true
    lines.push(`${increasing ? 'FAIL' : 'PASS'} ${name}_monotonic ${increasing ? 'strictly-increasing' : 'not-strictly-increasing'} not-strictly-increasing ${tag(i)}`)
  }
  let incomplete = results.length < 3
  results.forEach((r, i) => {
    lines.push(`INFO load_cold_ms ${fmt(r.load.cold.total)} - ${tag(i)}`)
    check('submit_p95_ms', pct(r.submit.total, 0.95), b.submit_p95_ms, i)
    check('load_p95_ms', pct(r.load.warm.total, 0.95), b.load_p95_ms, i)
    check('peak_mib', (r.memory.wasmPeakBytes + r.memory.jsAllocTwoRepBytes) / MiB, b.peak_mib, i)
    growth('wasm', r.switch.wasmBytesAfter, i)
    const res = r.memory.residual
    if (res.source === null) {
      incomplete = true
      lines.push(`INFO residual missing ${JSON.stringify(res.reason)} ${tag(i)}`)
    } else {
      lines.push(`INFO residual_source ${res.source} - ${tag(i)}`)
      growth('residual', res.bytes, i)
    }
  })
  if (results.length < 3) lines.push(`INFO runs ${results.length} 3 -`)
  return { verdict: failed ? 'FAIL' : incomplete ? 'INCOMPLETE' : 'PASS', lines }
}
