// usage: node live/run.mjs <script.mjs> [site] [locale]
// The script default-exports async ({ b, ctx, page, api, shot, watch, log, admin, base }) => {}
import path from 'node:path'
import { adminContext, api, browser, log, shot, watch, SITES } from './lib.mjs'

const [file, site = 'sa', locale = 'zh-CN'] = process.argv.slice(2)
const mod = await import(path.resolve(file))
const b = await browser()
const ctx = await adminContext(b, { site, locale })
const page = await ctx.newPage()
const problems = watch(page, [])
try {
  await page.goto(`${SITES[site].admin}/`)
  await page.waitForLoadState('networkidle')
  await mod.default({ b, ctx, page, api: (m, p, body) => api(page, m, p, body), shot, watch, log, admin: SITES[site].admin, base: SITES[site].base, problems, SITES })
} catch (e) {
  log('ERROR', e.message.split('\n').slice(0, 6).join(' | '))
  try { await shot(page, 'error-' + path.basename(file, '.mjs')) } catch { /* */ }
} finally {
  log('PROBLEMS', JSON.stringify(problems, null, 0).slice(0, 3000))
  await b.close()
}
