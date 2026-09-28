// continue checkout on a host with stored buyer state: node d2-pay.mjs <tag> <host> <stateFile>
import { user, admin, browser, shot, watch, load, save, note, sleep, walletOf } from './lib.mjs'
const [tag, host = 'zs2.dot2.com', stateFile = '.state/z2buyer.json'] = process.argv.slice(2)
const b = await browser(); const errs = []
const ctx = await b.newContext({ storageState: stateFile, viewport: { width: 1366, height: 900 }, locale: 'zh-CN' }); const p = await ctx.newPage(); watch(p, errs)
let order
p.on('response', async (r) => { if (r.request().method() === 'POST' && /\/api\/v1\/orders/.test(r.url())) { try { const j = await r.json(); if (j.data?.order_no) order = j.data; note(tag, `POST ${new URL(r.url()).pathname} -> ${JSON.stringify(j).slice(0, 400)}`) } catch {} } })
await p.goto(`https://${host}/checkout`); await sleep(3000)
const bal = p.getByText('优先使用余额支付'); await bal.click(); await sleep(800)
await shot(p, 'integration', `${tag}-04-checkout-balance`)
await p.getByRole('button', { name: /提交订单并支付/ }).click(); await sleep(6000)
console.log('url', p.url())
await shot(p, 'integration', `${tag}-05-after-pay`)
if (order) {
  for (let i = 0; i < 20; i++) { await sleep(3000); await p.goto(`https://${host}/orders/${order.order_no}`); await sleep(2500); const t = await p.locator('main').innerText(); if (/QA-INT|ZS-LAB|ACG|DJ-LAB/.test(t) && !/处理中/.test(t.slice(0, 80))) break }
  await shot(p, 'integration', `${tag}-06-order-detail`)
  console.log((await p.locator('main').innerText()).slice(0, 1500))
  const st = load(); st.lastOrder = { tag, host, order_no: order.order_no }; save(st)
}
console.log('ERRS', errs)
await b.close()
