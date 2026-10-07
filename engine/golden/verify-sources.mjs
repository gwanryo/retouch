// Verifies golden image provenance against <dir>/sources.json (default dir: this file's folder).
//   node engine/golden/verify-sources.mjs [--dir <golden dir>]
//       committed files (SHA-256) and the generator record: the commit must exist in this
//       repository and its engine/Cargo.lock must hash to cargo_lock_sha256
//   node engine/golden/verify-sources.mjs [--dir <golden dir>] <name> <path>
//       a downloaded source (SHA-1 + SHA-256)
// Every image in cases.json must have a record in sources.json `files`.
// Exits 1 on any mismatch, missing file, unfilled or unverifiable generator record.
// The generator check needs the full history (CI checks out with fetch-depth: 0).
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const args = process.argv.slice(2)
let dir = dirname(fileURLToPath(import.meta.url))
if (args[0] === '--dir') {
  dir = resolve(args[1])
  args.splice(0, 2)
}
const spec = JSON.parse(readFileSync(resolve(dir, 'sources.json'), 'utf8'))
const digest = (algo, bytes) => createHash(algo).update(bytes).digest('hex')
let failed = false
const fail = (msg) => {
  console.error(msg)
  failed = true
}
const check = (label, path, want) => {
  if (!existsSync(path)) return fail(`MISSING ${label}: ${path}`)
  const bytes = readFileSync(path)
  let ok = true
  for (const [algo, value] of Object.entries(want)) {
    const got = digest(algo, bytes)
    if (got !== value) {
      fail(`MISMATCH ${label} ${algo}: got ${got}, want ${value}`)
      ok = false
    }
  }
  if (ok) console.log(`ok ${label}`)
}

/** git output as bytes, or null when the command fails. */
const git = (...gitArgs) => {
  try {
    return execFileSync('git', ['-C', dir, ...gitArgs], { stdio: ['ignore', 'pipe', 'ignore'] })
  } catch {
    return null
  }
}

/** Provenance of the processed files: which engine-cli commit and lockfile produced them. */
const checkGenerator = (gen = {}) => {
  const { commit = '', cargo_lock_sha256: lockSha = '' } = gen
  if (!/^[0-9a-f]{40}$/.test(commit) || !/^[0-9a-f]{64}$/.test(lockSha)) {
    return fail(`UNFILLED generator record: commit=${commit} cargo_lock_sha256=${lockSha}`)
  }
  if (!git('cat-file', '-e', `${commit}^{commit}`)) {
    return fail(`UNKNOWN generator commit ${commit} (not in this repository; shallow clone?)`)
  }
  const lock = git('show', `${commit}:engine/Cargo.lock`)
  if (!lock) return fail(`MISSING generator engine/Cargo.lock at ${commit}`)
  const got = digest('sha256', lock)
  if (got !== lockSha) return fail(`MISMATCH generator cargo_lock_sha256: got ${got} at ${commit}, want ${lockSha}`)
  console.log(`ok generator ${commit.slice(0, 12)}`)
}

const [name, path] = args
if (name) {
  const src = spec.sources[name]
  if (!src) throw new Error(`unknown source ${name}`)
  check(`source ${name}`, resolve(path), { sha1: src.sha1, sha256: src.sha256 })
} else {
  const files = Object.entries(spec.files ?? {})
  if (files.length === 0) fail('EMPTY sources.json files: no provenance records')
  // every image the goldens decode must have a provenance record, or its byte check is skipped
  const cases = JSON.parse(readFileSync(resolve(dir, 'cases.json'), 'utf8'))
  for (const [name, file] of Object.entries(cases.images ?? {})) {
    if (!spec.files?.[file]) fail(`NO PROVENANCE for cases.json image ${name}: ${file}`)
  }
  for (const [file, meta] of files) {
    check(file, resolve(dir, file), { sha256: meta.sha256 })
  }
  checkGenerator(spec.generator)
}
process.exit(failed ? 1 : 0)
