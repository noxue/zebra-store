// Z2 buyer: add A and B to cart, checkout with wallet (UI). args: tag
import { user, admin, browser, userCtx, shot, watch, load, save, note, sleep, walletOf } from './lib.mjs'
const tag = process.argv[2] || 'I068'
const st = load(); const u = await user('zs2', 'qa-buyer@lab.test'); const me = await u.get('/me')
const s = await admin('store'); const before = await walletOf(s, st.qaZ2Uid)
const b = await browser(); const errs = []
const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage(); watch(p, errs)
let created
p.on('response', async (r) => { if (/\/orders\/(create-and-pay|create)|\/pay/.test(r.url()) && r.request().method() === 'POST') { try { const j = await r.json(); if (j.data?.order_no) created = j.data; note(tag, `POST ${new URL(r.url()).pathname} -> ${JSON.stringify(j).slice(0, 300)}`) } catch {} } })
for (const slug of ['upstream-2-11-1790358901663', 'upstream-2-12-1790358901682']) {
  await p.goto(`https://zs2.dot2.com/products/${slug}`); await sleep(2500)
  if (slug.includes('-11-')) await shot(p, 'integration', `${tag}-01-z2-product`, false)
  await p.getByRole('button', { name: /加入购物车/ }).first().click(); await sleep(1200)
}
await p.goto('https://zs2.dot2.com/cart'); await sleep(2500)
await shot(p, 'integration', `${tag}-02-cart`, false)
console.log('cart btns', (await p.locator('button:visible, a:visible').allInnerTexts()).filter(Boolean).join(' | ').slice(0, 600))
await p.getByText('去结算').first().click(); await sleep(3000)
console.log('url', p.url())
await shot(p, 'integration', `${tag}-03-checkout`)
console.log('checkout text', (await p.locator('main').innerText()).slice(0, 1500))
st.lastBuy = { tag, before }; save(st)
await ctx.storageState({ path: '.state/z2buyer.json' })
console.log('ERRS', errs)
await b.close()
