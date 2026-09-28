// node d6-uibuy.mjs <tag> <site(for token)> <email> <host> <slug> [formValue] [waitSec]
import { user, browser, userCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const [tag, site, email, host, slug, formValue = '', waitSec = '60', group = 'integration'] = process.argv.slice(2)
const u = await user(site, email); const me = await u.get('/me')
const b = await browser(); const errs = []
const ctx = await b.newContext({ viewport: { width: 1366, height: 900 }, locale: 'zh-CN' })
await ctx.addInitScript(([t, p]) => { localStorage.setItem('user_token', t); localStorage.setItem('user_profile', JSON.stringify(p)) }, [u.token, me])
const p = await ctx.newPage(); watch(p, errs)
let order; const msgs = []
p.on('response', async (r) => { if (r.request().method() === 'POST' && /\/api\/v1\/orders\/create/.test(r.url())) { try { const j = await r.json(); msgs.push(`sc=${j.status_code} ${j.msg}`); if (j.data?.order?.order_no || j.data?.order_no) order = j.data.order?.order_no || j.data.order_no } catch {} } })
await p.goto(`https://${host}/products/${slug}`); await sleep(3000)
await shot(p, group, `${tag}-a-product`, false)
await p.getByRole('button', { name: /立即购买/ }).first().click(); await sleep(3000)
if (formValue) { const inp = p.locator('main input[type=text]:visible, main input:not([type]):visible').first(); await inp.fill(formValue) }
await p.getByText('优先使用余额支付').click().catch(() => {}); await sleep(800)
await shot(p, group, `${tag}-b-checkout`, true)
await p.getByRole('button', { name: /提交订单并支付/ }).click(); await sleep(5000)
await shot(p, group, `${tag}-c-after-submit`, false)
note(tag, `UI submit on ${host}: ${msgs.join(' ; ')} order=${order} url=${p.url()} errs=${errs.join(' / ').slice(0, 300)}`)
if (order) {
  const t0 = Date.now(); let t = ''
  while ((Date.now() - t0) / 1000 < Number(waitSec)) { await p.goto(`https://${host}/orders/${order}`); await sleep(3000); t = await p.locator('main').innerText(); if (/已交付|已完成/.test(t)) break; await sleep(4000) }
  note(tag, `order ${order} detail after ${((Date.now() - t0) / 1000).toFixed(0)}s: ${t.replace(/\s+/g, ' ').slice(0, 400)}`)
  await shot(p, group, `${tag}-d-order`)
  const st = load(); st.lastOrder = { tag, host, order_no: order }; save(st)
}
await b.close()
