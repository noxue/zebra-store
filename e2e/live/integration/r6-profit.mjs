import { user, admin, walletOf, load, note, sleep, browser, userCtx, shot } from './lib.mjs'
const st = load(); const no = st.lastOrder.order_no
const buyer = await user('store', 'qa-sbuyer@lab.test'); const main = await buyer.raw('GET', `/orders/${no}`); note('R-011', `GET order via MAIN host: sc=${main.json.status_code} ${main.json.msg}`)
const { Api } = await import('./lib.mjs'); const sk = new Api('https://sakura.dot2.com', buyer.token); const o = await sk.get(`/orders/${no}`)
note('R-011', `buyer sees order on MAIN host: status=${o.status} total=${o.total_amount}; wallet now ${await walletOf(await admin('store'), st.sBuyerUid)}`)
const [lst] = await buyer.page('/orders?page=1&page_size=20'); const [lst2] = await sk.page('/orders?page=1&page_size=20'); note('R-011', `main-site order list contains it: ${lst.some((x) => x.order_no === no)}; sakura list contains it: ${lst2.some((x) => x.order_no === no)}; main list size=${lst.length} sakura list size=${lst2.length}`)
const r = await user('store', 'reseller-sakura@lab.test', { register: false }); const me = await r.get('/me')
const ro = await r.get(`/reseller/orders/${no}`); note('R-012', `reseller order detail: profit=${ro.profit_amount} buyer=${ro.buyer_email || ro.user_email || JSON.stringify(ro).match(/"[a-z_]*email[a-z_]*":"[^"]*"/)?.[0]} base=${ro.base_amount || ''} ${JSON.stringify(ro).slice(0, 300)}`)
const other = await user('store', 'reseller-neon@lab.test', { register: false }); const x = await other.raw('GET', `/reseller/orders/${no}`); note('R-012', `neon reseller fetching sakura's order: sc=${x.json.status_code} ${x.json.msg}`)
const t0 = Date.now(); let e
while (Date.now() - t0 < 150000) { const [led] = await r.page('/reseller/ledger-entries?page=1&page_size=10'); e = led.find((l) => l.order_id === o.id || l.order_id === ro.order_id); if (e && e.status === 'available') break; if (e && !st._first) { note('R-013', `ledger first seen: ${JSON.stringify(e).slice(0, 200)}`); st._first = 1 } await sleep(5000) }
note('R-013', `ledger entry after ${((Date.now() - t0) / 1000).toFixed(0)}s: ${JSON.stringify(e).slice(0, 250)}`)
const b = await browser(); const ctx = await userCtx(b, r.token, me); const p = await ctx.newPage()
for (const [path, name] of [[`/reseller/orders/${no}`, 'R012-order-detail'], ['/reseller/ledger', 'R013-ledger'], ['/reseller/finance', 'R013-finance']]) { await p.goto('https://store.dot2.com' + path); await sleep(3000); await shot(p, 'reseller', name) }
await b.close()
