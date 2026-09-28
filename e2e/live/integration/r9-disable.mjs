import { admin, user, browser, adminCtx, userCtx, shot, watch, note, sleep } from './lib.mjs'
import { execSync } from 'node:child_process'
const s = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', s.token); const p = await ctx.newPage(); watch(p, errs)
const net = []; p.on('response', async (x) => { if (x.url().includes('/resellers/profiles/') && x.request().method() === 'POST') net.push(`${new URL(x.url()).pathname} ${(await x.text()).slice(0, 150)}`) })
await p.goto('https://store.dot2.com/admin/resellers/profiles'); await sleep(3000)
const row = () => p.locator('tr', { hasText: 'qa-rsl@lab.test' }).first()
await row().getByRole('button', { name: '禁用' }).click(); await sleep(1200)
const ta = p.locator('[role=dialog] textarea:visible, [role=dialog] input:visible'); if (await ta.count()) await ta.first().fill('QA 禁用测试')
await shot(p, 'reseller', 'R020-01-disable-dialog', false)
await p.locator('[role=dialog] button:visible').last().click(); await sleep(1500)
const c = p.getByRole('button', { name: '确认' }); if (await c.count()) await c.last().click(); await sleep(2500)
const edge = (path) => { try { return execSync(`./edgeget.sh qaint.dot2.com '${path}'`).toString() } catch (e) { return String(e.stdout || e) } }
note('R-020', 'after disable, qaint public config: ' + edge('/api/v1/public/config').replace(/\s+/g, ' ').slice(0, 220))
const u = await user('store', 'qa-rsl@lab.test'); const me = await u.get('/me')
const d = await u.raw('GET', '/reseller/dashboard'); note('R-020', `console dashboard while disabled: sc=${d.json.status_code} ${d.json.msg}`)
const sc = await u.raw('PUT', '/reseller/site-config', { site_name: 'should-fail' }); note('R-020', `console write while disabled: sc=${sc.json.status_code} ${sc.json.msg}`)
const uc = await userCtx(b, u.token, me); const up = await uc.newPage(); await up.goto('https://store.dot2.com/reseller'); await sleep(3000); await shot(up, 'reseller', 'R020-02-console-disabled')
await row().getByRole('button', { name: '恢复' }).click(); await sleep(1200)
await p.locator('[role=dialog] button:visible').last().click().catch(() => {}); await sleep(1500)
const c2 = p.getByRole('button', { name: '确认' }); if (await c2.count()) await c2.last().click(); await sleep(2500)
note('R-020', 'after restore, qaint public config: ' + edge('/api/v1/public/config').replace(/\s+/g, ' ').slice(0, 160))
note('R-020', net.join(' || ')); await shot(p, 'reseller', 'R020-03-restored', false)
console.log('ERRS', errs); await b.close()
