// buy one product via API (create-and-pay with wallet) as a given user: node d4-buynow.mjs <tag> <site> <email> <product_id> <sku_id> [qty] [host]
import { user, load, save, note, sleep } from './lib.mjs'
const [tag, site, email, pid, sid, qty = '1', host] = process.argv.slice(2)
const u = await user(site, email, { host })
const r = await u.raw('POST', '/orders/create-and-pay', { items: [{ product_id: Number(pid), sku_id: Number(sid), quantity: Number(qty) }], channel_id: 0, use_balance: true })
const d = r.json.data
note(tag, `create-and-pay on ${host || site}: sc=${r.json.status_code} msg=${r.json.msg} order=${d?.order_no || d?.order?.order_no} paid=${d?.order_paid} total=${d?.order?.total_amount}`)
const no = d?.order_no || d?.order?.order_no
if (no) { const st = load(); st.lastOrder = { tag, host: host || site, order_no: no }; save(st) }
for (let i = 0; i < 8 && no; i++) { await sleep(2500); const o = await u.get(`/orders/${no}`); const kids = o.children?.length ? o.children : [o]; const txt = kids.map((c) => `${c.status}:${(c.fulfillment?.payload || '').split('\n')[0]}`).join(','); if (i === 7 || !/paid|pending/.test(o.status)) { note(tag, `buyer view: order ${no} status=${o.status} children=${txt}`); break } }
