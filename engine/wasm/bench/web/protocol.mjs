// Page <-> Worker protocol helpers (plan 02-1 Task 3.3). Pure: no browser globals, no wasm
// import, so node tests can import it.

export const INPUT_URLS = Object.freeze({
  jpeg: '/inputs/original.jpg',
  answerPng: '/inputs/answer.png',
  playerRecipe: '/inputs/player.json',
})

const RECOVERY =
  'run "cargo run --locked --release --manifest-path engine/Cargo.toml -p engine-cli -- gen-answer --input engine/golden/images/lake_2048x1365_baseline420.jpg --recipe engine/golden/recipes/warm_contrast.json --seed 0 --out-dir engine/target/perf" and "wasm-pack build engine/wasm --target web --release --out-dir pkg-web -- --locked"'

/** Fetch the three bench inputs; a missing one rejects with its URL, status and the fix. */
export const loadInputs = async (fetchFn) => {
  const responses = await Promise.all(Object.values(INPUT_URLS).map((url) => fetchFn(url)))
  Object.values(INPUT_URLS).forEach((url, i) => {
    if (!responses[i].ok) throw new Error(`${url} ${responses[i].status}: ${RECOVERY}`)
  })
  const [jpeg, answerPng, playerRecipe] = responses
  return {
    jpeg: new Uint8Array(await jpeg.arrayBuffer()),
    answerPng: new Uint8Array(await answerPng.arrayBuffer()),
    playerRecipe: await playerRecipe.text(),
  }
}

/**
 * One `performance.measureUserAgentSpecificMemory()` reading. `env = { isSecureContext,
 * crossOriginIsolated, measure }`. Never rejects: a missing prerequisite, an exception or a
 * timeout comes back as `{ bytes: null, reason }`.
 */
export const probeMemory = async (env, limitMs = 30000) => {
  for (const k of ['isSecureContext', 'crossOriginIsolated', 'measure']) {
    if (!env?.[k]) return { bytes: null, reason: k }
  }
  let timer
  const timeout = new Promise((resolve) => {
    timer = setTimeout(() => resolve({ bytes: null, reason: 'timeout' }), limitMs)
  })
  const measured = (async () => {
    try {
      const r = await env.measure()
      return Number.isInteger(r?.bytes) && r.bytes > 0 ? { bytes: r.bytes, reason: null } : { bytes: null, reason: 'invalid' }
    } catch (e) {
      return { bytes: null, reason: e?.name || 'Error' }
    }
  })()
  try {
    return await Promise.race([measured, timeout])
  } finally {
    clearTimeout(timer)
  }
}

/** Page side: answer one `{type:'probe', id}` with exactly one `probe-result`. */
export const answerProbe = async (msg, env, post, limitMs) => {
  const { bytes, reason } = await probeMemory(env, limitMs)
  post({ type: 'probe-result', id: msg.id, bytes, reason })
}

/**
 * Worker side: ask the page for a reading and wait for the result with the same id.
 * `onMessage(listener)` subscribes and returns an unsubscribe function.
 */
export const awaitProbe = (post, onMessage, id) =>
  new Promise((resolve) => {
    const off = onMessage((m) => {
      if (m?.type !== 'probe-result' || m.id !== id) return
      off()
      resolve({ bytes: m.bytes, reason: m.reason })
    })
    post({ type: 'probe', id })
  })
