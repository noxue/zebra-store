import { admin, load, walletOf, note } from './lib.mjs'
const st = load(); const tag = process.argv[2] || st.lastOrder.tag; const site = process.argv[3] || 'zs2'
const z = await admin(site); const s = await admin('store')
const [rows] = await z.page('/admin/procurement-orders?page=1&page_size=10')
const mine = rows.filter((r) => String(r.local_order_no || '').startsWith(st.lastOrder.order_no))
for (const r of mine) note(tag, `${site} procurement #${r.id} local=${r.local_order_no} status=${r.status} upstream_order_no=${r.upstream_order_no} upstream_amount=${r.upstream_amount} conn=${r.connection_id} err=${r.error_message || r.last_error || ''}`)
if (!mine.length) console.log(JSON.stringify(rows.slice(0, 3)).slice(0, 1500))
const w = await walletOf(s, st.qaZ2Uid); note(tag, `S wallet qa-z2 ${st.lastBuy?.before} -> ${w}`)
const p0 = mine[0] && await z.get(`/admin/procurement-orders/${mine[0].id}`); if (p0) console.log(JSON.stringify(p0).slice(0, 2500))
