// Bench static server (plan 02-1 Task 3, table 3.4 "serve"). Port 0, routes mapped to fixture
// files in a temporary directory.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { request } from 'node:http'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { DEFAULT_ROUTES, HARNESS_FILES, createServer, harnessSha256 } from '../serve.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const bench = resolve(here, '..')

const MIME = {
  '/': 'text/html; charset=utf-8',
  '/web/page.mjs': 'text/javascript',
  '/web/worker.mjs': 'text/javascript',
  '/web/protocol.mjs': 'text/javascript',
  '/scenario.mjs': 'text/javascript',
  '/budget.json': 'application/json',
  '/pkg/engine_wasm.js': 'text/javascript',
  '/pkg/engine_wasm_bg.wasm': 'application/wasm',
  '/inputs/original.jpg': 'image/jpeg',
  '/inputs/answer.png': 'image/png',
  '/inputs/player.json': 'application/json',
}

/** Routes with the default MIME types, every file a small fixture in a temporary directory. */
const fixtureRoutes = () => {
  const dir = mkdtempSync(join(tmpdir(), 'serve-'))
  const routes = {}
  let i = 0
  for (const [url, route] of Object.entries(DEFAULT_ROUTES)) {
    const file = join(dir, `f${i++}`)
    writeFileSync(file, `body of ${url}`)
    routes[url] = { ...route, file }
  }
  return { dir, routes }
}

const listen = async (server) => {
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  return server.address().port
}

/** Raw GET: the path goes out byte for byte (fetch would normalise `..` and `%2e`). */
const get = (port, path) =>
  new Promise((resolveGet, reject) => {
    const req = request({ host: '127.0.0.1', port, path, method: 'GET' }, (res) => {
      const chunks = []
      res.on('data', (c) => chunks.push(c))
      res.on('end', () => resolveGet({ status: res.statusCode, headers: res.headers, body: Buffer.concat(chunks).toString('utf8') }))
    })
    req.on('error', reject)
    req.end()
  })

const withServer = async (server, f) => {
  const port = await listen(server)
  try {
    await f(port)
  } finally {
    await new Promise((r) => server.close(r))
  }
}

test('S1 serves every route with its MIME and isolation headers', async () => {
  assert.deepEqual(Object.keys(DEFAULT_ROUTES).sort(), Object.keys(MIME).sort())
  const { routes } = fixtureRoutes()
  const meta = () => ({ commit: 'c', sha256: 'h' })
  await withServer(createServer(routes, meta), async (port) => {
    for (const [url, type] of Object.entries({ ...MIME, '/meta.json': 'application/json' })) {
      const res = await get(port, url)
      assert.equal(res.status, 200, url)
      assert.equal(res.headers['content-type'], type, url)
      assert.equal(res.headers['cross-origin-opener-policy'], 'same-origin', url)
      assert.equal(res.headers['cross-origin-embedder-policy'], 'require-corp', url)
      assert.equal(res.headers['cache-control'], 'no-store', url)
      if (url !== '/meta.json') assert.equal(res.body, `body of ${url}`, url)
    }
  })
})

test('S2 every import in page.mjs and worker.mjs is a served route', () => {
  for (const name of ['page.mjs', 'worker.mjs']) {
    const src = readFileSync(resolve(bench, 'web', name), 'utf8')
    const specs = [
      ...[...src.matchAll(/import\s[^'"]*?from\s*['"]([^'"]+)['"]/g)].map((m) => m[1]),
      ...[...src.matchAll(/new URL\(\s*['"]([^'"]+)['"]/g)].map((m) => m[1]),
    ]
    assert.ok(specs.length > 0, `${name} has no imports`)
    for (const s of specs) assert.ok(s in DEFAULT_ROUTES, `${name}: ${s} is not a served route`)
  }
  const html = readFileSync(resolve(bench, 'web', 'index.html'), 'utf8')
  for (const m of html.matchAll(/src="([^"]+)"/g)) assert.ok(m[1] in DEFAULT_ROUTES, `index.html: ${m[1]}`)
})

test('S3 missing file -> 404', async () => {
  const { dir, routes } = fixtureRoutes()
  routes['/inputs/answer.png'] = { ...routes['/inputs/answer.png'], file: join(dir, 'does-not-exist.png') }
  await withServer(createServer(routes, () => ({})), async (port) => {
    const res = await get(port, '/inputs/answer.png')
    assert.equal(res.status, 404)
    assert.equal(res.headers['cross-origin-embedder-policy'], 'require-corp')
  })
})

test('S4 paths outside the table -> 404', async () => {
  const { routes } = fixtureRoutes()
  await withServer(createServer(routes, () => ({})), async (port) => {
    for (const path of [
      '/inputs/../../Cargo.toml',
      '/web/../../../Cargo.lock',
      '/pkg/%2e%2e/%2e%2e/Cargo.toml',
      '/web/page.mjs/../worker.mjs',
      '/WEB/page.mjs',
      '/web/page.mjs?x=1',
    ]) {
      const res = await get(port, path)
      assert.equal(res.status, 404, path)
    }
  })
})

test('S5 worker is a module worker', () => {
  const src = readFileSync(resolve(bench, 'web', 'page.mjs'), 'utf8')
  assert.match(src, /new Worker\(.*,\s*\{\s*type:\s*'module'\s*\}\)/)
})

test('S6 meta.json reports the harness hash', async () => {
  const { routes } = fixtureRoutes()
  const injected = { commit: 'abc', sha256: 'def' }
  await withServer(createServer(routes, () => injected), async (port) => {
    const res = await get(port, '/meta.json')
    assert.deepEqual(JSON.parse(res.body), injected)
  })

  assert.deepEqual(HARNESS_FILES, [
    'scenario.mjs',
    'budget.json',
    'web/protocol.mjs',
    'web/page.mjs',
    'web/worker.mjs',
    'web/index.html',
    'serve.mjs',
  ])
  const dir = mkdtempSync(join(tmpdir(), 'harness-'))
  mkdirSync(join(dir, 'web'))
  for (const f of HARNESS_FILES) writeFileSync(join(dir, f), `content of ${f}\n`)
  const before = harnessSha256(dir)
  assert.match(before, /^[0-9a-f]{64}$/)
  mkdirSync(join(dir, 'results', '2026-10-07'), { recursive: true })
  writeFileSync(join(dir, 'results', '2026-10-07', 'desktop-chrome-run1.json'), '{}')
  writeFileSync(join(dir, 'PERF.md'), '# perf')
  assert.equal(harnessSha256(dir), before, 'result files do not change the hash')
  writeFileSync(join(dir, 'scenario.mjs'), 'content of scenario.mjs\n ')
  assert.notEqual(harnessSha256(dir), before, 'one more byte in scenario.mjs changes the hash')

  // Default meta: the real harness and HEAD.
  await withServer(createServer(routes), async (port) => {
    const meta = JSON.parse((await get(port, '/meta.json')).body)
    assert.match(meta.commit, /^[0-9a-f]{40}$/)
    assert.equal(meta.sha256, harnessSha256(bench))
  })
})
