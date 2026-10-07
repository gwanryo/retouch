// Bench page (plan 02-1 Task 3.3): starts the Worker, answers its memory probes, shows the
// result JSON, and runs the manual residual pass when the UA memory API is unavailable.
import { answerProbe } from '/web/protocol.mjs'

const $ = (id) => document.getElementById(id)
const status = $('status')
const output = $('output')
const startButton = $('start')
const copyButton = $('copy')
const manualButton = $('manual')
const manualForm = $('manual-form')
const manualLabel = $('manual-label')
const manualInput = $('manual-mb')

const measure = performance.measureUserAgentSpecificMemory?.bind(performance)
const probeEnv = { isSecureContext: self.isSecureContext, crossOriginIsolated: self.crossOriginIsolated, measure }

let result = null
let worker = null
let manualValues = []

const setStatus = (text) => { status.textContent = text }
const show = () => {
  output.textContent = JSON.stringify(result, null, 2)
  copyButton.disabled = false
  manualButton.disabled = !(result?.memory?.residual?.source === null)
}

const sha256Hex = async (bytes) =>
  [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map((b) => b.toString(16).padStart(2, '0')).join('')

const fetchOk = async (url) => {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`${url} ${res.status}: run "wasm-pack build engine/wasm --target web --release --out-dir pkg-web -- --locked"`)
  return res
}

const charging = async () => {
  if ($('charging').value !== 'auto') return $('charging').value === 'true'
  try {
    return (await navigator.getBattery?.())?.charging ?? null
  } catch {
    return null
  }
}

const onWorkerMessage = async (m) => {
  if (m.type === 'progress') setStatus(m.text)
  else if (m.type === 'probe') answerProbe(m, probeEnv, (r) => worker.postMessage(r))
  else if (m.type === 'error') setStatus(`오류: ${m.message}`)
  else if (m.type === 'result') {
    result = m.result
    result.env = {
      ...result.env,
      uaMemoryApi: typeof measure === 'function',
      charging: await charging(),
      device: $('device').value.trim() || null,
    }
    show()
    setStatus(result.memory.residual.source === null
      ? `완료. 잔존 메모리 측정 불가(${result.memory.residual.reason}): 수동 잔존 패스를 실행하세요.`
      : '완료. 결과 JSON을 복사해 저장하세요.')
  } else if (m.type === 'manual-pause') {
    manualForm.hidden = false
    manualLabel.textContent = `회차 ${m.round}/10 해제 완료. DevTools Memory 패널에서 GC(휴지통) 후 이 Worker의 JS heap(MB)을 입력`
    manualInput.value = ''
    manualInput.focus()
  } else if (m.type === 'manual-done') {
    manualForm.hidden = true
    result.memory.residual = {
      source: 'manual',
      bytes: manualValues.map((mb) => Math.round(mb * 2 ** 20)),
      note: 'DevTools heap after GC',
    }
    show()
    setStatus('수동 잔존 패스 완료. 결과 JSON을 다시 복사하세요.')
  }
}

startButton.addEventListener('click', async () => {
  startButton.disabled = true
  try {
    const kind = $('kind').value
    setStatus('하네스 정보 읽는 중')
    const harness = await (await fetchOk('/meta.json')).json()
    const wasmSha256 = await sha256Hex(await (await fetchOk('/pkg/engine_wasm_bg.wasm')).arrayBuffer())
    worker = new Worker(new URL('/web/worker.mjs', location.href), { type: 'module' })
    worker.onmessage = (e) => { onWorkerMessage(e.data).catch((err) => setStatus(`오류: ${err?.message ?? err}`)) }
    worker.onerror = (e) => setStatus(`오류: Worker 시작 실패 (${e.message || '모듈 로드 오류, 콘솔 확인'})`)
    setStatus('Worker 시작')
    worker.postMessage({ type: 'run', kind, harness, wasmSha256 })
  } catch (e) {
    setStatus(`오류: ${e?.message ?? e}`)
  }
})

copyButton.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(JSON.stringify(result, null, 2))
    setStatus('JSON을 클립보드에 복사했습니다.')
  } catch (e) {
    setStatus(`복사 실패(${e?.name ?? e}): 아래 JSON을 직접 선택해 복사하세요.`)
  }
})

manualButton.addEventListener('click', () => {
  manualButton.disabled = true
  manualValues = []
  worker.postMessage({ type: 'manual-residual' })
})

manualForm.addEventListener('submit', (e) => {
  e.preventDefault()
  const mb = Number(manualInput.value)
  if (!Number.isFinite(mb) || mb <= 0) {
    manualLabel.textContent = `${manualLabel.textContent.split(' / ')[0]} / 양수를 다시 입력하세요`
    return
  }
  manualValues.push(mb)
  manualForm.hidden = true
  worker.postMessage({ type: 'manual-continue' })
})
