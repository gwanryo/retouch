// Tests for engine/golden/verify-sources.mjs on a throwaway git repository:
//   node --test engine/golden/test/verify-sources.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../verify-sources.mjs')
const sha = (algo, data) => createHash(algo).update(data).digest('hex')
const LOCK = '# lockfile\nversion = 4\n'
const IMAGE = 'pixels'

/** A git repo with engine/Cargo.lock committed and a golden dir whose sources.json is `edit(spec)`. */
function fixture(edit = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'verify-sources-'))
  const g = (...a) => execFileSync('git', ['-C', root, ...a], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()
  g('init', '-q')
  mkdirSync(join(root, 'engine/golden/images'), { recursive: true })
  writeFileSync(join(root, 'engine/Cargo.lock'), LOCK)
  g('add', '.')
  g('-c', 'user.name=t', '-c', 'user.email=t@t', 'commit', '-q', '-m', 'lock')
  writeFileSync(join(root, 'engine/golden/images/a.jpg'), IMAGE)
  const spec = {
    generator: { commit: g('rev-parse', 'HEAD'), cargo_lock_sha256: sha('sha256', LOCK) },
    files: { 'images/a.jpg': { sha256: sha('sha256', IMAGE) } },
    sources: { a: { sha1: sha('sha1', IMAGE), sha256: sha('sha256', IMAGE) } },
  }
  const cases = { images: { a: 'images/a.jpg' } }
  edit(spec, root, cases)
  writeFileSync(join(root, 'engine/golden/sources.json'), JSON.stringify(spec))
  writeFileSync(join(root, 'engine/golden/cases.json'), JSON.stringify(cases))
  return root
}

function run(root, ...extra) {
  const r = spawnSync(process.execPath, [script, '--dir', join(root, 'engine/golden'), ...extra], { encoding: 'utf8' })
  rmSync(root, { recursive: true, force: true })
  return { code: r.status, out: r.stdout + r.stderr }
}

test('passes for intact files and a verifiable generator record', () => {
  const r = run(fixture())
  assert.equal(r.code, 0, r.out)
  assert.match(r.out, /ok images\/a\.jpg/)
  assert.match(r.out, /ok generator [0-9a-f]{12}/)
})

test('fails on a missing file', () => {
  const r = run(fixture((s) => (s.files['images/b.jpg'] = { sha256: '0'.repeat(64) })))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISSING images\/b\.jpg/)
})

test('fails on a wrong file hash', () => {
  const r = run(fixture((s) => (s.files['images/a.jpg'].sha256 = '0'.repeat(64))))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISMATCH images\/a\.jpg sha256/)
})

test('fails on an unfilled generator record', () => {
  const r = run(fixture((s) => (s.generator.commit = 'PENDING')))
  assert.equal(r.code, 1)
  assert.match(r.out, /UNFILLED generator record/)
})

test('fails on a generator commit that does not exist', () => {
  const r = run(fixture((s) => (s.generator.commit = '0'.repeat(40))))
  assert.equal(r.code, 1)
  assert.match(r.out, /UNKNOWN generator commit/)
})

test('fails when the lockfile hash does not match the generator commit', () => {
  const r = run(fixture((s) => (s.generator.cargo_lock_sha256 = '0'.repeat(64))))
  assert.equal(r.code, 1)
  assert.match(r.out, /MISMATCH generator cargo_lock_sha256/)
})

test('fails when a cases.json image has no provenance record', () => {
  const r = run(fixture((s) => delete s.files['images/a.jpg']))
  assert.equal(r.code, 1)
  assert.match(r.out, /NO PROVENANCE for cases\.json image a: images\/a\.jpg/)
})

test('fails on an empty files list', () => {
  const r = run(fixture((s) => (s.files = {})))
  assert.equal(r.code, 1)
  assert.match(r.out, /EMPTY sources\.json files/)
})

test('checks a downloaded source by SHA-1 and SHA-256', () => {
  let root = fixture()
  const ok = run(root, 'a', join(root, 'engine/golden/images/a.jpg'))
  assert.equal(ok.code, 0, ok.out)
  root = fixture((s) => (s.sources.a.sha1 = '0'.repeat(40)))
  const bad = run(root, 'a', join(root, 'engine/golden/images/a.jpg'))
  assert.equal(bad.code, 1)
  assert.match(bad.out, /MISMATCH source a sha1/)
})
