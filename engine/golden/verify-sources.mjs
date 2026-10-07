// Verifies golden image provenance against <dir>/sources.json (default dir: this file's folder).
//   node engine/golden/verify-sources.mjs [--dir <golden dir>]
//       committed files (SHA-256), a provenance record for every cases.json image, and a
//       well-formed `reproduce` record for every processed file (processing not "none…")
//   node engine/golden/verify-sources.mjs [--dir <golden dir>] --processed <files-key> <path>
//       a processed file regenerated with the current engine-cli (SHA-256 == files[key].sha256)
//   node engine/golden/verify-sources.mjs [--dir <golden dir>] <name> <path>
//       a downloaded source (SHA-1 + SHA-256)
// Provenance never depends on git commits: the repository squash-merges, so branch commits do
// not survive. Processed files are proven by regenerating them (CI does the source-free fixture
// on every run); `reproduce` records when and with what that last succeeded.
// Exits 1 on any mismatch, missing file or missing/malformed record.
import { createHash } from 'node:crypto'
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

const isProcessed = (meta) => !String(meta.processing ?? '').startsWith('none')
const HEX = (n) => new RegExp(`^[0-9a-f]{${n}}$`)

/** Shape of a processed file's reproduce record (audit data; only the format is checked). */
const checkReproduce = (file, rep) => {
  if (!rep) return fail(`NO REPRODUCE RECORD ${file}`)
  const problems = []
  if (!['ci', 'manual'].includes(rep.how)) problems.push(`how=${rep.how}`)
  if (rep.last_ok !== null && !/^\d{4}-\d{2}-\d{2}$/.test(String(rep.last_ok))) problems.push(`last_ok=${rep.last_ok}`)
  if (typeof rep.engine_version !== 'string' || rep.engine_version === '') problems.push('engine_version')
  if (!HEX(40).test(String(rep.engine_tree))) problems.push('engine_tree')
  if (!HEX(64).test(String(rep.cargo_lock_sha256))) problems.push('cargo_lock_sha256')
  if (typeof rep.rustc !== 'string' || rep.rustc === '') problems.push('rustc')
  if (problems.length > 0) return fail(`BAD REPRODUCE RECORD ${file}: ${problems.join(', ')}`)
  if (rep.last_ok === null) console.log(`UNVERIFIED reproduce ${file} (${rep.how}): never reproduced`)
  else console.log(`ok reproduce ${file} (${rep.how}, last ${rep.last_ok})`)
}

if (args[0] === '--processed') {
  const [, key, path] = args
  const meta = spec.files?.[key]
  if (!meta) fail(`UNKNOWN processed file ${key}`)
  else check(`processed ${key}`, resolve(path), { sha256: meta.sha256 })
} else if (args[0]) {
  const [name, path] = args
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
    if (isProcessed(meta)) checkReproduce(file, meta.reproduce)
  }
}
process.exit(failed ? 1 : 0)
