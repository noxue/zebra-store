import crypto from 'node:crypto'
import { load, note, admin } from './lib.mjs'
const st = load(); const s = await admin('store')
const p8 = await s.get('/admin/procurement-orders/8')
const body = JSON.stringify({ event: 'order.status_changed', order_id: Number(p8.upstream_order_id || 0), order_no: p8.upstream_order_no, status: 'canceled', downstream_order_no: p8.local_order_no })
const ts = String(Math.floor(Date.now() / 1000))
for (const [lbl, h] of [
  ['no headers', {}],
  ['valid key, bad signature', { 'Dujiao-Next-Api-Key': st.djCred.key, 'Dujiao-Next-Timestamp': ts, 'Dujiao-Next-Signature': 'deadbeef' }],
  ['other connection key (acg conn) w/ sig', { 'Dujiao-Next-Api-Key': 'e009beb09bd2475ab3328403908c7f55e4ce826998d08948f54b4b02bd7364b5', 'Dujiao-Next-Timestamp': ts, 'Dujiao-Next-Signature': crypto.createHmac('sha256', 'x').update(body).digest('hex') }],
]) {
  const r = await fetch('https://store.dot2.com/api/v1/upstream/callback', { method: 'POST', headers: { 'Content-Type': 'application/json', ...h }, body })
  note('I-044', `forged callback (${lbl}): http ${r.status} ${(await r.text()).slice(0, 120)}`)
}
const after = await s.get('/admin/procurement-orders/8'); note('I-044', `procurement #8 status before=${p8.status} after=${after.status}`)
