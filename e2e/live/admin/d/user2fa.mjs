import fs from 'node:fs'
import crypto from 'node:crypto'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
const b32 = (s) => { const a = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'; let bits = ''; for (const c of s.replace(/=+$/, '').toUpperCase()) bits += a.indexOf(c).toString(2).padStart(5, '0'); const out = []; for (let i = 0; i + 8 <= bits.length; i += 8) out.push(parseInt(bits.slice(i, i + 8), 2)); return Buffer.from(out) }
const totp = (secret) => { const c = Buffer.alloc(8); c.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000))); const h = crypto.createHmac('sha1', b32(secret)).update(c).digest(); const o = h[19] & 15; return String(((h.readUInt32BE(o) & 0x7fffffff) % 1e6)).padStart(6, '0') }
export default async ({ page, shot, log, admin }) => {
  const call = (m, p, b, tok) => page.evaluate(async ({ m, p, b, tok }) => (await (await fetch('/api/v1' + p, { method: m, headers: { 'content-type': 'application/json', ...(tok ? { authorization: 'Bearer ' + tok } : {}) }, body: b ? JSON.stringify(b) : undefined })).json()), { m, p, b, tok })
  const tok = (await call('POST', '/auth/login', { email: U.email, password: U.password })).data.token
  const s = await call('POST', '/me/2fa/setup', {}, tok); log('setup', s.status_code)
  const e = await call('POST', '/me/2fa/enable', { code: totp(s.data.secret) }, tok); log('enable', e.status_code, e.msg)
  const l = await call('POST', '/auth/login', { email: U.email, password: U.password }); log('login needs 2fa', JSON.stringify(l).slice(0, 160))
  await page.goto(admin + '/users/' + U.id); await page.waitForLoadState('networkidle')
  await shot(page, 'd-user-2fa-enabled', false)
  const [r] = await Promise.all([page.waitForResponse(x => x.request().method() !== 'GET' && /2fa/.test(x.url()), { timeout: 8000 }).catch(() => null), (async () => { await page.getByRole('button', { name: /重置/ }).first().click(); await page.waitForTimeout(500); const c = page.locator('[role=dialog] button', { hasText: /^确认$|^确定$|重置/ }).last(); if (await c.isVisible().catch(() => false)) await c.click() })()])
  await page.waitForTimeout(1000); log('reset', r ? new URL(r.url()).pathname + ' ' + (await r.text()).slice(0, 150) : 'NO REQUEST')
  await shot(page, 'd-user-2fa-reset', false)
  const l2 = await call('POST', '/auth/login', { email: U.email, password: U.password }); log('login after reset', JSON.stringify(l2).slice(0, 120))
}
