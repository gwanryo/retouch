// Capture desktop/mobile × light/dark screenshots of a running dev or preview
// server. Used for PR screenshots (see .github/pull_request_template.md).
//
//   node scripts/screenshot.mjs [baseUrl] [outDir] [--viewport]
//   default baseUrl: http://127.0.0.1:5173/retouch/   default outDir: ./shots
//   --viewport captures only the first screen (for README/OG images) instead of the full page.
import { chromium, devices } from '@playwright/test'
import { mkdirSync } from 'node:fs'
import { resolve } from 'node:path'

const args = process.argv.slice(2)
const fullPage = !args.includes('--viewport')
const positional = args.filter((a) => !a.startsWith('--'))
const base = positional[0] ?? 'http://127.0.0.1:5173/retouch/'
const out = resolve(positional[1] ?? 'shots')
mkdirSync(out, { recursive: true })

const targets = [
  { name: 'desktop-light', viewport: { width: 1440, height: 900 }, colorScheme: 'light' },
  { name: 'desktop-dark', viewport: { width: 1440, height: 900 }, colorScheme: 'dark' },
  { name: 'mobile-light', ...devices['iPhone 12'], colorScheme: 'light' },
  { name: 'mobile-dark', ...devices['iPhone 12'], colorScheme: 'dark' },
]

const browser = await chromium.launch()
for (const t of targets) {
  const { name, ...ctxOptions } = t
  const ctx = await browser.newContext({ ...ctxOptions, reducedMotion: 'reduce' })
  const page = await ctx.newPage()
  await page.goto(base, { waitUntil: 'networkidle' })
  await page.evaluate(() => document.fonts.ready)
  await page.screenshot({ path: resolve(out, `${name}.png`), fullPage })
  console.log(`saved ${name}.png`)
  await ctx.close()
}
await browser.close()
