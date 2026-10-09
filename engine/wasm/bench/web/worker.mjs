// Bench Worker (plan 02-1 Task 3.3): time pass (runGate), then the residual pass (switchOnce x10,
// one page-side memory probe after each). Messages in: run (once), manual-residual,
// manual-continue, probe-result. Messages out: progress, probe, result, manual-pause,
// manual-done, error.
import init, * as wasm from '/pkg/engine_wasm.js'
import { COUNTS, runGate, switchOnce } from '/scenario.mjs'
import { loadInputs, awaitProbe } from '/web/protocol.mjs'

const listeners = new Set()
const onMessage = (listener) => {
  listeners.add(listener)
  return () => listeners.delete(listener)
}
const post = (m) => self.postMessage(m)
const progress = (text) => post({ type: 'progress', text })
const fail = (e) => post({ type: 'error', message: String(e?.stack ?? e?.message ?? e) })

let started = false
let inputs = null
let manualBusy = false

const nextMessage = (type) =>
  new Promise((resolve) => {
    const off = onMessage((m) => {
      if (m?.type !== type) return
      off()
      resolve(m)
    })
  })

const run = async ({ kind, harness, wasmSha256 }) => {
  const runId = crypto.randomUUID()
  progress('wasm 초기화')
  await init()
  progress('입력 파일 읽는 중')
  inputs = { ...(await loadInputs((url) => fetch(url))), regionJson: '{"kind":"full"}', seed: 0, compositionJson: null }
  progress('시간 측정 중 (로드 6회, 제출 21회, 전환 10회)')
  const result = await runGate(wasm, inputs, { now: () => performance.now(), wasmSha256, harness, kind, runId })
  result.env = {
    userAgent: navigator.userAgent,
    hardwareConcurrency: navigator.hardwareConcurrency ?? null,
    deviceMemory: navigator.deviceMemory ?? null,
    isSecureContext: self.isSecureContext,
    crossOriginIsolated: self.crossOriginIsolated,
  }
  const bytes = []
  let reason = null
  for (let k = 1; k <= COUNTS.switch; k++) {
    progress(`잔존 메모리 측정 ${k}/${COUNTS.switch}`)
    switchOnce(wasm, inputs)
    const probe = await awaitProbe(post, onMessage, `${runId}:${k}`)
    if (probe.bytes === null) {
      reason = probe.reason
      break
    }
    bytes.push(probe.bytes)
  }
  result.memory.residual = reason === null ? { source: 'ua', bytes } : { source: null, reason }
  post({ type: 'result', result })
}

const manualResidual = async () => {
  if (!inputs || manualBusy) return
  manualBusy = true
  try {
    for (let round = 1; round <= COUNTS.switch; round++) {
      switchOnce(wasm, inputs)
      const resumed = nextMessage('manual-continue')
      post({ type: 'manual-pause', round })
      await resumed
    }
    post({ type: 'manual-done' })
  } finally {
    manualBusy = false
  }
}

self.onmessage = (e) => {
  const m = e.data
  for (const l of [...listeners]) l(m)
  if (m?.type === 'run') {
    if (started) return
    started = true
    run(m).catch(fail)
  } else if (m?.type === 'manual-residual') {
    manualResidual().catch(fail)
  }
}
self.onerror = (e) => fail(e?.message ?? e)
self.onunhandledrejection = (e) => fail(e?.reason ?? e)
