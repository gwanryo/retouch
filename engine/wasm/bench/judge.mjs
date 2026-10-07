// Performance gate judgement (plan 02-1 Task 3.1). Judges every result file of one device
// together against budget.json; no representative run is picked.
// Usage: node engine/wasm/bench/judge.mjs <mobile|desktop> <result.json>...
// Last line: GATE PASS|FAIL|INCOMPLETE, exit code 0|1|2. A file that is not JSON is FAIL.
import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { judge } from './scenario.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const budget = JSON.parse(readFileSync(resolve(here, 'budget.json'), 'utf8'))
const [kind, ...files] = process.argv.slice(2)
const EXIT = { PASS: 0, FAIL: 1, INCOMPLETE: 2 }

const finish = (verdict, lines) => {
  for (const l of lines) console.log(l)
  console.log(`GATE ${verdict}`)
  process.exit(EXIT[verdict])
}

if (!Object.hasOwn(budget, kind ?? '') || files.length === 0) {
  finish('FAIL', ['FAIL usage node engine/wasm/bench/judge.mjs <mobile|desktop> <result.json>... -'])
}

const results = []
const parseErrors = []
files.forEach((file, i) => {
  try {
    results.push(JSON.parse(readFileSync(file, 'utf8')))
  } catch (e) {
    parseErrors.push(`FAIL json ${JSON.stringify(`${file}: ${e.message}`)} - run${i + 1}`)
  }
})
if (parseErrors.length) finish('FAIL', parseErrors)

const { verdict, lines } = judge(results, budget, kind)
finish(verdict, [...files.map((f, i) => `INFO file ${f} - run${i + 1}`), ...lines])
