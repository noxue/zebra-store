// Live QA helpers for the 对接 / 分站 flows (exploratory, not part of the CI suite).
// Talks to the real dot2.com lab through public hostnames only.
import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { fileURLToPath } from 'node:url'
import { chromium } from '@playwright/test'

export const HERE = path.dirname(fileURLToPath(import.meta.url))
export const SHOT_ROOT = path.join(HERE, '..', 'shots')
const STATE = path.join(HERE, '.state')
fs.mkdirSync(STATE, { recursive: true })

export const PASS = {
  store: process.env.SA_PASS || 'ZIWIKm6rBOaxSr5kZs7k',
  zs2: process.env.Z2_PASS || 'ecsT9w2sBJbBksAmZs7k',
  dujiao: process.env.DJ_PASS || 'zRBpsLHIqEAHiRHoZs7k',
  acgAdmin: ['admin@acg.lab', process.env.ACG_PASS || 'rcA3kFvLbLTkjlFzZs7k'],
  user: process.env.USER_PASS || 'KKYrp7h0Mspnfmp1Zs7k',
}
export const url = (n) => `https://${n}.dot2.com`
export const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a)
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const _fetch = globalThis.fetch
globalThis.fetch = async (...a) => { for (let i = 0; ; i++) { try { return await _fetch(...a) } catch (e) { if (i >= 4) throw e; await sleep(1500 * (i + 1)) } } }

// ------------------------------------------------------------------ persisted scratch state
const stFile = path.join(STATE, 'state.json')
export function load() { try { return JSON.parse(fs.readFileSync(stFile, 'utf8')) } catch { return {} } }
export function save(st) { fs.writeFileSync(stFile, JSON.stringify(st, null, 1)) }

// ------------------------------------------------------------------ envelope API client
export class ApiError extends Error {
  constructor(method, p, http, body) {
    super(`${method} ${p} -> http ${http} ${JSON.stringify(body).slice(0, 400)}`)
    Object.assign(this, { http, body, code: body?.status_code, msg: body?.msg })
  }
}
export class Api {
  constructor(base, token = '') { this.base = base.replace(/\/$/, ''); this.token = token }
  async raw(method, p, body, headers = {}) {
    const h = { Accept: 'application/json', ...headers }
    if (body !== undefined) h['Content-Type'] = 'application/json'
    if (this.token) h.Authorization = 'Bearer ' + this.token
    const t0 = Date.now()
    const res = await fetch(this.base + '/api/v1' + p, { method, headers: h, body: body === undefined ? undefined : JSON.stringify(body) })
    const text = await res.text()
    let json; try { json = JSON.parse(text) } catch { json = { raw: text.slice(0, 300) } }
    return { http: res.status, json, ms: Date.now() - t0, headers: res.headers }
  }
  async call(method, p, body, ok = [0]) {
    const r = await this.raw(method, p, body)
    if (!r.json || !ok.includes(r.json.status_code)) throw new ApiError(method, p, r.http, r.json)
    return r.json.data
  }
  get(p, ok) { return this.call('GET', p, undefined, ok) }
  post(p, b = {}, ok) { return this.call('POST', p, b, ok) }
  put(p, b = {}, ok) { return this.call('PUT', p, b, ok) }
  patch(p, b = {}, ok) { return this.call('PATCH', p, b, ok) }
  del(p, ok) { return this.call('DELETE', p, undefined, ok) }
  async page(p) { const r = await this.raw('GET', p); if (r.json.status_code !== 0) throw new ApiError('GET', p, r.http, r.json); return [r.json.data, r.json.pagination] }
}

const tokFile = path.join(STATE, 'tokens.json')
const toks = () => { try { return JSON.parse(fs.readFileSync(tokFile, 'utf8')) } catch { return {} } }
const putTok = (k, v) => { const t = toks(); t[k] = v; fs.writeFileSync(tokFile, JSON.stringify(t, null, 1)) }

/** Admin API for a compatible instance with a cached token (login is rate limited). */
export async function admin(site) {
  const api = new Api(url(site))
  const k = `admin:${site}`
  const t = toks()[k]
  if (t) { api.token = t; const r = await api.raw('GET', '/admin/authz/me'); if (r.json.status_code === 0) return api }
  api.token = ''
  api.token = (await api.post('/admin/login', { username: 'admin', password: PASS[site] })).token
  putTok(k, api.token)
  return api
}

/** Storefront user (registers when missing). host: which site host to talk to. */
export async function user(site, email, { host, register = true, password = PASS.user } = {}) {
  const api = new Api(url(host || site))
  const k = `user:${site}:${email}`
  const t = toks()[k]
  if (t) { api.token = t; const r = await api.raw('GET', '/me'); if (r.json.status_code === 0) return api }
  api.token = ''
  if (register) {
    const r = await api.raw('POST', '/auth/register', { email, password, agreement_accepted: true })
    if (r.json.status_code === 0) api.token = r.json.data.token
  }
  if (!api.token) api.token = (await api.post('/auth/login', { email, password, remember_me: true })).token
  putTok(k, api.token)
  return api
}

export async function walletOf(adm, uid) {
  const w = await adm.get(`/admin/users/${uid}/wallet`)
  return String((w.account || w).balance)
}
export async function setWallet(adm, uid, want) {
  const bal = Number(await walletOf(adm, uid))
  const diff = Number(want) - bal
  if (Math.abs(diff) < 0.005) return
  await adm.post(`/admin/users/${uid}/wallet/adjust`, { amount: Math.abs(diff).toFixed(2), operation: diff > 0 ? 'add' : 'subtract', remark: 'qa-int' })
}

// ------------------------------------------------------------------ zebra-store protocol signer
export function zsHeaders(key, secret, method, p, query = '', body = '', { ts, nonce } = {}) {
  ts = ts ?? Math.floor(Date.now() / 1000)
  nonce = nonce ?? crypto.randomBytes(12).toString('hex')
  const canonQ = query ? query.split('&').map((kv) => kv.split('=')).sort((a, b) => a[0] === b[0] ? (a[1] < b[1] ? -1 : 1) : a[0] < b[0] ? -1 : 1).map((a) => a.join('=')).join('&') : ''
  const canon = `ZS1\n${method}\n${p}\n${canonQ}\n${ts}\n${nonce}\n${crypto.createHash('sha256').update(body).digest('hex')}`
  const sig = crypto.createHmac('sha256', secret).update(canon).digest('hex')
  return { 'ZS-Key': key, 'ZS-Timestamp': String(ts), 'ZS-Nonce': nonce, 'ZS-Signature': `v1=${sig}` }
}
export async function zs(base, key, secret, method, p, { query = '', body, headers = {}, sig = {} } = {}) {
  const b = body === undefined ? '' : JSON.stringify(body)
  const full = '/api/v1/zs' + p
  const h = { ...zsHeaders(key, secret, method, full, query, b, sig), ...headers }
  if (b) h['Content-Type'] = 'application/json'
  const t0 = Date.now()
  const res = await fetch(base + full + (query ? '?' + query : ''), { method, headers: h, body: b || undefined })
  const text = await res.text()
  let json; try { json = JSON.parse(text) } catch { json = { raw: text.slice(0, 300) } }
  return { http: res.status, json, ms: Date.now() - t0, retryAfter: res.headers.get('retry-after'), sentHeaders: h }
}
export function zsDecrypt(delivery, secret) {
  const key = crypto.createHash('sha256').update('zs-delivery:' + secret).digest()
  const buf = Buffer.from(delivery.ciphertext, 'base64')
  const d = crypto.createDecipheriv('aes-256-gcm', key, Buffer.from(delivery.nonce, 'base64'))
  d.setAuthTag(buf.subarray(buf.length - 16))
  return JSON.parse(Buffer.concat([d.update(buf.subarray(0, buf.length - 16)), d.final()]).toString())
}

// ------------------------------------------------------------------ acg-faka session (form + cookies)
export class Acg {
  constructor(role) { this.base = url('acg'); this.referer = this.base + (role === 'admin' ? '/admin/' : '/user/'); this.cookies = {} }
  async req(method, p, data) {
    const body = data ? new URLSearchParams(Object.entries(data).flatMap(([k, v]) => Array.isArray(v) ? v.map((x) => [k, String(x)]) : [[k, String(v)]])).toString() : undefined
    const h = { Referer: this.referer, Cookie: Object.entries(this.cookies).map(([k, v]) => `${k}=${v}`).join('; ') }
    if (body) h['Content-Type'] = 'application/x-www-form-urlencoded'
    const res = await fetch(this.base + p, { method, headers: h, body, redirect: 'manual' })
    for (const c of res.headers.getSetCookie?.() || []) { const [kv] = c.split(';'); const i = kv.indexOf('='); this.cookies[kv.slice(0, i)] = kv.slice(i + 1) }
    return res.text()
  }
  async post(p, data = {}) { const t = await this.req('POST', p, data); try { return JSON.parse(t) } catch { return { raw: t.slice(0, 300) } } }
}
export async function acgAdmin() {
  const s = new Acg('admin')
  const out = await s.post('/admin/api/authentication/login', { username: PASS.acgAdmin[0], password: PASS.acgAdmin[1] })
  if (out.code !== 200) throw new Error('acg admin login ' + JSON.stringify(out))
  await s.req('GET', '/admin/dashboard/index?agree=1')
  return s
}
export async function acgMember(name) {
  const s = new Acg('user')
  let out = await s.post('/user/api/authentication/login', { username: name, password: PASS.user })
  if (out.code !== 200) { out = await s.post('/user/api/authentication/register', { username: name, password: PASS.user }) }
  if (out.code !== 200) throw new Error('acg member ' + JSON.stringify(out))
  return s
}
export async function acgUser(s, name) {
  const out = await s.post('/admin/api/user/data', { page: 1, limit: 20, 'equal-username': name })
  return (out.data?.list || [])[0]
}

// ------------------------------------------------------------------ browser
export async function browser() { return chromium.launch({ headless: true }) }
/** Context with an admin token injected (no UI login needed). */
export async function adminCtx(b, site, token, locale = 'zh-CN') {
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 }, locale, ignoreHTTPSErrors: true })
  await ctx.addInitScript(([t, l]) => { localStorage.setItem('admin_token', t); localStorage.setItem('admin_is_super', '1'); localStorage.setItem('admin_locale', l) }, [token, locale])
  return ctx
}
/** Context with a storefront user token injected for the given hosts. */
export async function userCtx(b, token, profile, { mobile = false } = {}) {
  const ctx = await b.newContext({ viewport: mobile ? { width: 390, height: 844 } : { width: 1366, height: 900 }, locale: 'zh-CN', ignoreHTTPSErrors: true })
  if (token) await ctx.addInitScript(([t, p]) => { localStorage.setItem('user_token', t); localStorage.setItem('user_profile', JSON.stringify(p)) }, [token, profile || {}])
  return ctx
}
export async function shot(page, group, name, full = true) {
  const dir = path.join(SHOT_ROOT, group)
  fs.mkdirSync(dir, { recursive: true })
  await page.screenshot({ path: path.join(dir, `${name}.png`), fullPage: full })
  return `e2e/live/shots/${group}/${name}.png`
}
export function watch(page, sink = []) {
  page.on('pageerror', (e) => sink.push('pageerror ' + String(e).slice(0, 200)))
  page.on('response', async (r) => {
    const u = r.url(); if (!u.includes('/api/')) return
    const where = `${r.request().method()} ${new URL(u).pathname}`
    if (r.status() >= 400) { sink.push(`HTTP ${r.status()} ${where}`); return }
    try { const j = await r.json(); if (typeof j.status_code === 'number' && j.status_code !== 0) sink.push(`sc=${j.status_code} ${j.msg} ${where}`) } catch { /* not json */ }
  })
  return sink
}

// ------------------------------------------------------------------ evidence journal
const journal = path.join(STATE, 'journal.log')
export function note(id, text) { const line = `${new Date().toISOString()} [${id}] ${text}`; fs.appendFileSync(journal, line + '\n'); console.log(line) }

/** PUT a store product keeping everything but the given sku price / active flag. */
export async function editProduct(adm, pid, { price, active } = {}) {
  const d = await adm.get(`/admin/products/${pid}`)
  const body = {
    category_id: d.category_id, slug: d.slug, title: d.title, description: d.description, content: d.content,
    images: d.images || [], tags: d.tags || [], price_amount: String(price ?? d.price_amount), purchase_type: d.purchase_type,
    fulfillment_type: d.fulfillment_type, is_active: active ?? d.is_active, sort_order: d.sort_order,
    min_purchase_quantity: d.min_purchase_quantity, max_purchase_quantity: d.max_purchase_quantity, stock_display_mode: d.stock_display_mode,
    manual_form_schema: d.manual_form_schema, seo_meta: d.seo_meta,
    skus: d.skus.map((s) => ({ id: s.id, sku_code: s.sku_code, spec_values: s.spec_values, price_amount: String(price ?? s.price_amount), cost_price_amount: s.cost_price_amount, manual_stock_total: s.manual_stock_total, is_active: s.is_active, sort_order: s.sort_order })),
  }
  const r = await adm.raw('PUT', `/admin/products/${pid}`, body)
  if (r.json.status_code !== 0) throw new Error('editProduct ' + JSON.stringify(r.json))
  return r.json.data
}

/** PUT a site connection keeping fields, overriding some. Secret must be supplied (never returned). */
export async function updateConn(adm, id, secret, patch = {}) {
  const c = await adm.get(`/admin/site-connections/${id}`)
  const body = { name: c.name, base_url: c.base_url, api_key: c.api_key, api_secret: secret, protocol: c.protocol, callback_url: c.callback_url, retry_max: c.retry_max, retry_intervals: c.retry_intervals, exchange_rate: c.exchange_rate, price_markup_percent: c.price_markup_percent, price_rounding_mode: c.price_rounding_mode, auto_sync_price: c.auto_sync_price, extra: c.extra, ...patch }
  const r = await adm.raw('PUT', `/admin/site-connections/${id}`, body)
  if (r.json.status_code !== 0) throw new Error('updateConn ' + JSON.stringify(r.json))
  return r.json.data
}
