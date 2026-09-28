// Shared helpers for live storefront QA against the dot2.com lab.
import { chromium, devices } from '@playwright/test'
import fs from 'node:fs'
import crypto from 'node:crypto'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
export const SHOTS = path.join(here, 'shots', 'storefront')
fs.mkdirSync(SHOTS, { recursive: true })
export const LOGF = path.join(here, 'storefront-log.jsonl')

export const S = 'https://store.dot2.com'
export const SITES = { S, SK: 'https://sakura.dot2.com', NE: 'https://neon.dot2.com', MA: 'https://matcha.dot2.com' }
export const ADMIN = { username: 'admin', password: 'ZIWIKm6rBOaxSr5kZs7k' }
export const TESTPW = 'KKYrp7h0Mspnfmp1Zs7k'
export const QAPW = 'QaTest#2026sf'

export async function api(p, { method = 'GET', body, token, base = S, headers = {} } = {}) {
  const res = await fetch(`${base}/api/v1/${p.replace(/^\//, '')}`, {
    method,
    headers: { 'content-type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}), ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await res.text()
  let json
  try { json = JSON.parse(text) } catch { json = { raw: text } }
  return { http: res.status, ...json }
}

let _admin
export async function adminToken() {
  if (_admin) return _admin
  const f = path.join(here, '.state', 'sf-admin-token.json')
  try { const c = JSON.parse(fs.readFileSync(f, 'utf8')); if (c.exp > Date.now() + 600000) { const chk = await api('admin/authz/me', { token: c.token }); if (chk.status_code === 0) return (_admin = c.token) } } catch {}
  for (let i = 0; i < 10; i++) {
    const r = await api('admin/login', { method: 'POST', body: ADMIN })
    if (r.data?.token) { _admin = r.data.token; fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, JSON.stringify({ token: _admin, exp: Date.parse(r.data.expires_at) })); return _admin }
    const m = /(\d+)/.exec(r.msg || ''); await new Promise((res) => setTimeout(res, ((m ? +m[1] : 30) + 2) * 1000))
  }
  throw new Error('admin login failed')
}
export async function adm(p, opts = {}) { return api(`admin/${p}`, { ...opts, token: await adminToken() }) }

export async function userToken(email, password = QAPW, base = S) {
  const r = await api('auth/login', { method: 'POST', body: { email, password }, base })
  return r.data?.token
}

export async function launch() { return chromium.launch({ headless: true }) }

/** New context+page with diagnostics collection. */
export async function open(browser, { mobile = false, locale = 'zh-CN', storage } = {}) {
  const ctx = await browser.newContext({
    ...(mobile ? { ...devices['iPhone 13'], viewport: { width: 390, height: 844 } } : { viewport: { width: 1440, height: 900 } }),
    locale,
    storageState: storage,
    ignoreHTTPSErrors: true,
  })
  const page = await ctx.newPage()
  const diag = { console: [], http: [], pageerrors: [], slow: [] }
  page.on('console', (m) => { if (m.type() === 'error' || m.type() === 'warning') diag.console.push(`[${m.type()}] ${m.text().slice(0, 300)} @${page.url()}`) })
  page.on('pageerror', (e) => diag.pageerrors.push(`${e.message.slice(0, 300)} @${page.url()}`))
  page.on('response', (r) => { if (r.status() >= 400) diag.http.push(`${r.status()} ${r.request().method()} ${r.url()}`) })
  page.on('requestfailed', (r) => { const f = r.failure()?.errorText || ''; if (!/ERR_ABORTED/.test(f)) diag.http.push(`FAILED ${r.url()} ${f}`) })
  return { ctx, page, diag }
}

export async function shot(page, name, full = false) {
  const f = path.join(SHOTS, `${name}.png`)
  await page.screenshot({ path: f, fullPage: full })
  return `e2e/live/shots/storefront/${name}.png`
}

/** Navigate and measure time to network idle. */
export async function go(page, url, diag) {
  const t0 = Date.now()
  await page.goto(url, { waitUntil: 'domcontentloaded' })
  try { await page.waitForLoadState('networkidle', { timeout: 15000 }) } catch {}
  const ms = Date.now() - t0
  if (ms > 2000 && diag) diag.slow.push(`${ms}ms ${url}`)
  return ms
}

export function record(entry) {
  fs.appendFileSync(LOGF, JSON.stringify({ t: new Date().toISOString(), ...entry }) + '\n')
  console.log(JSON.stringify(entry))
}

/** Log in through the real UI. */
export async function uiLogin(page, base, email, password = QAPW) {
  await page.goto(`${base}/auth/login`)
  await page.locator('input[type=email], input[name=email], input[autocomplete=email], input[autocomplete=username]').first().fill(email)
  await page.locator('input[type=password]').first().fill(password)
  await page.locator('form button[type=submit], button:has-text("登录"), button:has-text("Login"), button:has-text("Sign in")').first().click()
  await page.waitForURL((u) => !u.pathname.startsWith('/auth/login'), { timeout: 20000 })
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** Pre-dismisses the home announcement (current version) for every page of the context. */
export async function noAnnouncement(ctx, base = S) {
  const c = await api('public/config', { base })
  const v = c.data?.announcement?.version || ''
  await ctx.addInitScript((ver) => { try { localStorage.setItem('announcement_dismiss', JSON.stringify({ version: ver, mode: 'forever' })) } catch {} }, v)
}

/** RFC 6238 TOTP (SHA1, 6 digits, 30 s) for a base32 secret. */
export function totp(secret, offset = 0) {
  const alpha = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
  const clean = secret.replace(/[\s=]/g, '').toUpperCase()
  let bits = ''
  for (const ch of clean) bits += alpha.indexOf(ch).toString(2).padStart(5, '0')
  const bytes = []
  for (let i = 0; i + 8 <= bits.length; i += 8) bytes.push(parseInt(bits.slice(i, i + 8), 2))
  const key = Buffer.from(bytes)
  const counter = Math.floor(Date.now() / 1000 / 30) + offset
  const buf = Buffer.alloc(8); buf.writeBigUInt64BE(BigInt(counter))
  const h = crypto.createHmac('sha1', key).update(buf).digest()
  const o = h[h.length - 1] & 0xf
  const code = ((h.readUInt32BE(o) & 0x7fffffff) % 1000000).toString().padStart(6, '0')
  return code
}
