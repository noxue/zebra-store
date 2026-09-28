// usage: node measure.mjs <label> <z2productId> <field:cost|price|stock|active> <expected> [timeoutSec]
import { admin, note, sleep } from './lib.mjs'
const [label, pid, field, expected, tmo = '420'] = process.argv.slice(2)
const z = await admin('zs2')
const t0 = Date.now()
while ((Date.now() - t0) / 1000 < Number(tmo)) {
  const p = await z.get(`/admin/products/${pid}`); const s = p.skus[0]
  const v = { cost: s.cost_price_amount, price: s.price_amount, stock: String(s.auto_stock_available), active: String(p.is_active) }[field]
  if (v === expected) { note(label, `Z2 product ${pid} ${field} reached ${expected} after ${((Date.now() - t0) / 1000).toFixed(1)} s`); process.exit(0) }
  await sleep(3000)
}
const p = await z.get(`/admin/products/${pid}`); note(label, `TIMEOUT ${tmo}s: Z2 product ${pid} ${field} != ${expected}; sku=${JSON.stringify(p.skus[0])} active=${p.is_active}`)
