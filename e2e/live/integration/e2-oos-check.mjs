import { zs, load, note, user, browser, userCtx, shot, sleep, admin, walletOf, setWallet } from './lib.mjs'
const st = load(); const { key, secret } = st.qaCred; const S = 'https://store.dot2.com'
const q = await zs(S, key, secret, 'POST', '/orders/quote', { body: { items: [{ sku_id: st.qaB.sku_id, quantity: 1 }, { sku_id: st.qaA.sku_id, quantity: 1 }] } })
note('I-074', `direct quote (B oos + A): http ${q.http} ${JSON.stringify(q.json.data)}`)
const o = await zs(S, key, secret, 'POST', '/orders', { body: { items: [{ sku_id: st.qaB.sku_id, quantity: 1 }], downstream_order_no: 'QA-OOS-' + Date.now() }, headers: { 'Idempotency-Key': 'qa-oos-' + Date.now() } })
note('I-074', `direct order of oos: http ${o.http} ${JSON.stringify(o.json).slice(0, 250)}`)
// low balance quote
const s = await admin('store'); const w = await walletOf(s, st.qaZ2Uid); await setWallet(s, st.qaZ2Uid, 1)
const q2 = await zs(S, key, secret, 'POST', '/orders/quote', { body: { items: [{ sku_id: st.qaA.sku_id, quantity: 1 }] } })
note('I-073', `quote with wallet 1.00: sufficient_balance=${q2.json.data?.sufficient_balance} balance=${q2.json.data?.balance} total=${q2.json.data?.total}`)
const o2 = await zs(S, key, secret, 'POST', '/orders', { body: { quote_id: q2.json.data.quote_id, items: [{ sku_id: st.qaA.sku_id, quantity: 1 }], downstream_order_no: 'QA-NOBAL-' + Date.now() }, headers: { 'Idempotency-Key': 'qa-nobal-' + Date.now() } })
note('I-073', `direct order with wallet 1.00: http ${o2.http} ${JSON.stringify(o2.json).slice(0, 200)}`)
await setWallet(s, st.qaZ2Uid, Number(w))
// Z2 buyer: UI product page + API attempt
const u = await user('zs2', 'qa-buyer@lab.test'); const me = await u.get('/me')
const z = await admin('zs2'); const wb = await walletOf(z, me.id)
const r = await u.raw('POST', '/orders/create-and-pay', { items: [{ product_id: 3, sku_id: 3, quantity: 1 }], channel_id: 0, use_balance: true })
note('I-074', `Z2 buyer create-and-pay oos item: sc=${r.json.status_code} msg=${r.json.msg}; buyer wallet ${wb} -> ${await walletOf(z, me.id)}`)
const b = await browser(); const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage()
await p.goto('https://zs2.dot2.com/products/upstream-2-12-1790358901682'); await sleep(3000)
await shot(p, 'integration', 'I074-01-z2-soldout', false)
note('I-074', 'Z2 product page buttons: ' + (await p.locator('button:visible').allInnerTexts()).filter(Boolean).slice(-4).join(' | '))
await b.close()
