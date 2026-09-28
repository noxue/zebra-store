// Live admin QA helpers (exploratory, not part of the CI suite).
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from '@playwright/test'

export const HERE = path.dirname(fileURLToPath(import.meta.url))
export const SHOTS = path.join(HERE, '..', 'shots', 'admin')
fs.mkdirSync(SHOTS, { recursive: true })

export const SITES = {
  sa: { base: 'https://store.dot2.com', admin: 'https://store.dot2.com/admin', user: 'admin', pass: process.env.SA_PASS || 'ZIWIKm6rBOaxSr5kZs7k' },
  z2: { base: 'https://zs2.dot2.com', admin: 'https://zs2.dot2.com/admin', user: 'admin', pass: process.env.Z2_PASS || 'ecsT9w2sBJbBksAmZs7k' },
}

export const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a)

/** Collects console errors, page errors, HTTP>=400 and non-zero envelopes. */
export function watch(page, sink = []) {
  page.on('console', (m) => { if (m.type() === 'error') sink.push({ kind: 'console', text: m.text().slice(0, 300) }) })
  page.on('pageerror', (e) => sink.push({ kind: 'pageerror', text: String(e).slice(0, 300) }))
  page.on('response', async (r) => {
    const u = r.url()
    if (!u.includes('/api/')) return
    const where = `${r.request().method()} ${new URL(u).pathname}${new URL(u).search}`
    if (r.status() >= 400) { sink.push({ kind: 'http', text: `HTTP ${r.status()} ${where}` }); return }
    try {
      if (!(r.headers()['content-type'] || '').includes('json')) return
      const b = await r.json()
      if (typeof b.status_code === 'number' && b.status_code !== 0) sink.push({ kind: 'api', text: `sc=${b.status_code} ${b.msg} ${where}` })
    } catch { /* ignore */ }
  })
  return sink
}

export async function browser(opts = {}) {
  return chromium.launch({ headless: true, ...opts })
}

const stateFile = (site, user) => path.join(HERE, '.state', `${site}-${user}.json`)

/** Context logged into the admin of `site` (storage state cached on disk). */
export async function adminContext(b, { site = 'sa', user, pass, locale = 'zh-CN', viewport = { width: 1440, height: 900 }, fresh = false, mobile = false } = {}) {
  const s = SITES[site]
  user = user || s.user
  pass = pass || s.pass
  const sf = stateFile(site, user)
  const ctxOpts = { viewport, locale, ignoreHTTPSErrors: true, acceptDownloads: true, ...(mobile ? { isMobile: true, hasTouch: true, deviceScaleFactor: 2 } : {}) }
  if (!fresh && fs.existsSync(sf)) {
    const ctx = await b.newContext({ ...ctxOpts, storageState: sf })
    await ctx.addInitScript((l) => localStorage.setItem('admin_locale', l), locale)
    const p = await ctx.newPage()
    await p.goto(`${s.admin}/`)
    await p.waitForTimeout(1500)
    const ok = !p.url().includes('/login')
    await p.close()
    if (ok) return ctx
    await ctx.close()
  }
  const ctx = await b.newContext(ctxOpts)
  await ctx.addInitScript((l) => localStorage.setItem('admin_locale', l), locale)
  const p = await ctx.newPage()
  await p.goto(`${s.admin}/login`)
  await p.locator('input').first().fill(user)
  await p.locator('input[type=password]').first().fill(pass)
  await p.locator('button[type=submit]').first().click()
  await p.waitForURL((u) => !u.pathname.endsWith('/login'), { timeout: 20000 })
  fs.mkdirSync(path.dirname(sf), { recursive: true })
  await ctx.storageState({ path: sf })
  await p.close()
  return ctx
}

export async function shot(page, name, full = true) {
  const f = path.join(SHOTS, `${name}.png`)
  await page.screenshot({ path: f, fullPage: full })
  return `e2e/live/shots/admin/${name}.png`
}

/** Calls the API with the page's admin token. */
export async function api(page, method, p, body) {
  return page.evaluate(async ({ method, p, body }) => {
    const t = localStorage.getItem('admin_token')
    const r = await fetch(`/api/v1${p}`, { method, headers: { 'content-type': 'application/json', authorization: `Bearer ${t}` }, body: body ? JSON.stringify(body) : undefined })
    const txt = await r.text()
    try { return { http: r.status, ...JSON.parse(txt) } } catch { return { http: r.status, raw: txt.slice(0, 500) } }
  }, { method, p, body })
}

export function save(name, data) {
  const f = path.join(HERE, 'out', `${name}.json`)
  fs.mkdirSync(path.dirname(f), { recursive: true })
  fs.writeFileSync(f, JSON.stringify(data, null, 2))
}
