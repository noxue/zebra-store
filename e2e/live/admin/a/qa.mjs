// usage: node live/admin/a/qa.mjs <step>
import fs from 'node:fs'
import { adminContext, api, browser, log, shot, watch, SITES, HERE } from '../lib.mjs'
import { totp } from './totp.mjs'
const step = process.argv[2]
const A = SITES.sa.admin
const ST = `${HERE}/out/qa-state.json`
const st = fs.existsSync(ST) ? JSON.parse(fs.readFileSync(ST, 'utf8')) : {}
const saveSt = () => fs.writeFileSync(ST, JSON.stringify(st, null, 2))
const notices = async (p, ms = 2000) => { await p.waitForTimeout(ms); return (await p.locator('[role=status]').allInnerTexts()).map((s) => s.trim()) }
const b = await browser()
const newCtx = () => b.newContext({ viewport: { width: 1440, height: 900 }, locale: 'zh-CN' })
async function uiLogin(p, u, pw) {
  await p.goto(`${A}/login`); await p.waitForLoadState('networkidle')
  await p.locator('input').first().fill(u); await p.locator('input[type=password]').fill(pw)
  await p.locator('button[type=submit]').click(); await p.waitForTimeout(2500)
}
const menu = (p) => p.locator('aside a, nav a').evaluateAll((els) => els.map((e) => e.textContent.trim()).filter(Boolean))
const rawFetch = (p, token, method, path, body) => p.evaluate(async ({ token, method, path, body }) => {
  const r = await fetch(`/api/v1${path}`, { method, headers: { 'content-type': 'application/json', authorization: `Bearer ${token}` }, body: body ? JSON.stringify(body) : undefined })
  let j = null; try { j = await r.json() } catch {}
  return { http: r.status, sc: j?.status_code, msg: j?.msg }
}, { token, method, path, body })

try {
  if (step === 'auditor') {
    const c = await newCtx(); const p = await c.newPage(); const pr = watch(p, [])
    await uiLogin(p, 'qa-auditor', 'QaAudit#2026pass')
    log('after login url', p.url()); await p.waitForLoadState('networkidle')
    await shot(p, 'a-auditor-dashboard')
    log('auditor menu', JSON.stringify(await menu(p)))
    for (const r of ['users', 'orders', 'payments', 'settings', 'authz', 'products']) {
      await p.goto(`${A}/${r}`); await p.waitForTimeout(1500)
      log('visit', r, '->', p.url())
    }
    await shot(p, 'a-auditor-forbidden')
    log('forbidden text', (await p.locator('main').innerText()).replace(/\s+/g, ' ').slice(0, 200))
    const tok = await p.evaluate(() => localStorage.getItem('admin_token'))
    for (const [m, path] of [['GET', '/admin/users'], ['GET', '/admin/orders'], ['GET', '/admin/dashboard/overview'], ['PUT', '/admin/settings'], ['GET', '/admin/authz/admins'], ['POST', '/admin/authz/admins']])
      log('api', m, path, JSON.stringify(await rawFetch(p, tok, m, path, m === 'GET' ? undefined : {})))
    log('auditor problems', JSON.stringify(pr).slice(0, 1500))
  }
  if (step === 'qaadmin') {
    const c = await newCtx(); const p = await c.newPage(); const pr = watch(p, [])
    await uiLogin(p, 'qa-admin', st.qaPass || 'QaAdmin#2026pass')
    log('url', p.url()); await p.waitForLoadState('networkidle')
    log('qa-admin menu', JSON.stringify(await menu(p)))
    await p.goto(`${A}/orders`); await p.waitForLoadState('networkidle'); await p.waitForTimeout(800)
    log('orders url', p.url(), 'rows', await p.locator('tbody tr').count())
    await shot(p, 'a-qaadmin-orders')
    const tok = await p.evaluate(() => localStorage.getItem('admin_token'))
    st.tok1 = tok; saveSt()
    const ord = await rawFetch(p, tok, 'GET', '/admin/orders?page=1&page_size=1')
    log('GET orders', JSON.stringify(ord))
    log('GET order detail', JSON.stringify(await rawFetch(p, tok, 'GET', '/admin/orders/41')))
    log('PATCH order', JSON.stringify(await rawFetch(p, tok, 'PATCH', '/admin/orders/999999', { status: 'completed' })))
    log('refund', JSON.stringify(await rawFetch(p, tok, 'POST', '/admin/orders/999999/refund-to-wallet', {})))
    // does detail UI try to call forbidden endpoints?
    await p.goto(`${A}/users`); await p.waitForTimeout(1200); log('users ->', p.url())
    // 2FA enable
    await p.goto(`${A}/security`); await p.waitForLoadState('networkidle')
    await p.getByRole('button', { name: '启用两步验证' }).click(); await p.waitForTimeout(2000)
    const secret = (await p.locator('[role=dialog] code').innerText()).trim()
    st.secret = secret; saveSt(); log('secret', secret)
    await shot(p, 'a-2fa-setup', false)
    await p.locator('#totp-setup-code').fill(totp(secret))
    await p.locator('[role=dialog] button').last().click(); await p.waitForTimeout(2000)
    const rc = await p.locator('[role=dialog]').last().innerText()
    st.recovery = rc.match(/[A-Z0-9]{4,}-[A-Z0-9-]{4,}|[a-z0-9]{8,}/gi); saveSt()
    log('recovery dialog', rc.replace(/\s+/g, ' ').slice(0, 400))
    await shot(p, 'a-2fa-recovery', false)
    const closeBtn = p.locator('[role=dialog]').last().locator('button').last()
    log('close disabled before ack', await closeBtn.isDisabled())
    await p.locator('[role=dialog]').last().locator('label').last().click()
    await closeBtn.click(); await p.waitForTimeout(800)
    await shot(p, 'a-2fa-enabled')
    // B-004 password change
    const inputs = p.locator('input[type=password]')
    await inputs.nth(0).fill(st.qaPass || 'QaAdmin#2026pass')
    await inputs.nth(1).fill('QaAdmin#2026new'); await inputs.nth(2).fill('QaAdmin#2026new')
    await p.getByRole('button', { name: '修改密码' }).click(); await p.waitForTimeout(600)
    await shot(p, 'a-pw-confirm', false)
    await p.locator('[role=dialog] button').last().click()
    log('pw notices', await notices(p, 2500), 'url', p.url())
    st.qaPass = 'QaAdmin#2026new'; saveSt()
    log('old token after pw change', JSON.stringify(await rawFetch(p, tok, 'GET', '/admin/orders?page=1&page_size=1')))
    log('qa problems', JSON.stringify(pr).slice(0, 1500))
  }
  if (step === 'totplogin') {
    const c = await newCtx(); const p = await c.newPage(); const pr = watch(p, [])
    await uiLogin(p, 'qa-admin', st.qaPass)
    log('url after pw', p.url()); await shot(p, 'a-2fa-login-challenge')
    const code = p.locator('input').first()
    await code.fill('000000'); await p.locator('button[type=submit]').click()
    log('wrong code notices', await notices(p, 2000))
    await code.fill(totp(st.secret)); await p.locator('button[type=submit]').click(); await p.waitForTimeout(2500)
    log('after totp url', p.url())
    await shot(p, 'a-2fa-login-ok')
    log('problems', JSON.stringify(pr).slice(0, 1200))
  }
  if (step === 'reset2fa') {
    const c = await adminContext(b, {}); const p = await c.newPage(); const pr = watch(p, [])
    await p.goto(`${A}/authz`); await p.waitForLoadState('networkidle')
    const row = p.locator('tr', { hasText: 'qa-admin' })
    log('row', (await row.innerText()).replace(/\s+/g, ' '))
    await row.getByRole('button', { name: /重置/ }).click(); await p.waitForTimeout(500)
    await shot(p, 'a-reset2fa-confirm', false)
    await p.locator('[role=dialog] button').last().click()
    log('notices', await notices(p))
    log('row after', (await row.innerText()).replace(/\s+/g, ' '))
    log('problems', JSON.stringify(pr))
  }
  if (step === 'nototp') {
    const c = await newCtx(); const p = await c.newPage()
    await uiLogin(p, 'qa-admin', st.qaPass)
    log('url (expect dashboard, no TOTP)', p.url())
    st.tok2 = await p.evaluate(() => localStorage.getItem('admin_token')); saveSt()
    await shot(p, 'a-after-reset2fa-login')
  }
  if (step === 'checktok') {
    const c = await newCtx(); const p = await c.newPage(); await p.goto(`${A}/login`)
    log('tok2', JSON.stringify(await rawFetch(p, st.tok2, 'GET', '/admin/authz/me')))
  }
} catch (e) { log('ERR', e.message.split('\n').slice(0, 4).join(' | ')) }
await b.close()
