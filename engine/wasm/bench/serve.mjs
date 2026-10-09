// Static server for the browser Worker bench (plan 02-1 Task 3.2). Cross-origin isolated
// (COOP/COEP) so performance.measureUserAgentSpecificMemory() is available. A request is served
// only when req.url (raw: no decoding, no normalisation) is exactly a key of the route table.
// Usage (repository root, after the Step 5 builds):
//   node engine/wasm/bench/serve.mjs      -> http://localhost:8787/
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import { readFileSync } from 'node:fs'
import { createServer as createHttpServer } from 'node:http'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const bench = dirname(fileURLToPath(import.meta.url))
const engine = resolve(bench, '../..')
const JS = 'text/javascript'
const JSON_TYPE = 'application/json'

export const DEFAULT_ROUTES = Object.freeze({
  '/': { file: resolve(bench, 'web/index.html'), type: 'text/html; charset=utf-8' },
  '/web/page.mjs': { file: resolve(bench, 'web/page.mjs'), type: JS },
  '/web/worker.mjs': { file: resolve(bench, 'web/worker.mjs'), type: JS },
  '/web/protocol.mjs': { file: resolve(bench, 'web/protocol.mjs'), type: JS },
  '/scenario.mjs': { file: resolve(bench, 'scenario.mjs'), type: JS },
  '/budget.json': { file: resolve(bench, 'budget.json'), type: JSON_TYPE },
  '/pkg/engine_wasm.js': { file: resolve(engine, 'wasm/pkg-web/engine_wasm.js'), type: JS },
  '/pkg/engine_wasm_bg.wasm': { file: resolve(engine, 'wasm/pkg-web/engine_wasm_bg.wasm'), type: 'application/wasm' },
  '/inputs/original.jpg': { file: resolve(engine, 'golden/images/lake_2048x1365_baseline420.jpg'), type: 'image/jpeg' },
  '/inputs/answer.png': { file: resolve(engine, 'target/perf/answer_2048.png'), type: 'image/png' },
  '/inputs/player.json': { file: resolve(engine, 'golden/recipes/player_warm_near.json'), type: JSON_TYPE },
})

/** Harness files, in hashing order (relative to the bench directory). */
export const HARNESS_FILES = Object.freeze([
  'scenario.mjs',
  'budget.json',
  'web/protocol.mjs',
  'web/page.mjs',
  'web/worker.mjs',
  'web/index.html',
  'serve.mjs',
])

/** SHA-256 of the harness files concatenated in HARNESS_FILES order. */
export const harnessSha256 = (dir = bench) => {
  const h = createHash('sha256')
  for (const f of HARNESS_FILES) h.update(readFileSync(resolve(dir, f)))
  return h.digest('hex')
}

export const gitHead = () => {
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: bench, encoding: 'utf8' }).trim()
  } catch {
    return 'unknown'
  }
}

const defaultMeta = () => ({ commit: gitHead(), sha256: harnessSha256(bench) })

const HEADERS = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
  'Cache-Control': 'no-store',
}

export const createServer = (routes = DEFAULT_ROUTES, meta = defaultMeta) =>
  createHttpServer(async (req, res) => {
    const send = (status, type, body) => {
      res.writeHead(status, { ...HEADERS, 'Content-Type': type })
      res.end(req.method === 'HEAD' ? undefined : body)
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') return send(405, 'text/plain; charset=utf-8', 'method not allowed\n')
    try {
      if (req.url === '/meta.json') return send(200, JSON_TYPE, JSON.stringify(meta()))
      const route = Object.hasOwn(routes, req.url) ? routes[req.url] : null
      if (!route) return send(404, 'text/plain; charset=utf-8', `not found: ${req.url}\n`)
      let body
      try {
        body = await readFile(route.file)
      } catch {
        return send(404, 'text/plain; charset=utf-8', `not found: ${req.url} (missing file ${route.file})\n`)
      }
      return send(200, route.type, body)
    } catch (e) {
      return send(500, 'text/plain; charset=utf-8', `${e?.message ?? e}\n`)
    }
  })

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const host = '127.0.0.1'
  const port = 8787
  createServer().listen(port, host, () => {
    console.log(`bench server: http://localhost:${port}/ (listening on ${host}:${port}, COOP/COEP on)`)
  })
}
