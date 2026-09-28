import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, log }) => {
  const call = (m, p, b, tok) => page.evaluate(async ({ m, p, b, tok }) => (await (await fetch('/api/v1' + p, { method: m, headers: { 'content-type': 'application/json', ...(tok ? { authorization: 'Bearer ' + tok } : {}) }, body: b ? JSON.stringify(b) : undefined })).json()), { m, p, b, tok })
  const tok = (await call('POST', '/auth/login', { email: U.email, password: U.password })).data.token
  const pv = await call('POST', '/orders/preview', { items: [{ product_id: 2, sku_id: 3, quantity: 1 }] }, tok); log('manual preview', JSON.stringify(pv).slice(0, 400))
  const out = {}
  const a = await call('POST', '/orders/create-and-pay', { items: [{ product_id: 8, sku_id: 12, quantity: 1 }], use_balance: true }, tok); log('A', JSON.stringify(a).slice(0, 300)); out.A = a.data
  const b = await call('POST', '/orders/create-and-pay', { items: [{ product_id: 2, sku_id: 3, quantity: 1 }], use_balance: true, manual_form_data: {} }, tok); log('B', JSON.stringify(b).slice(0, 300)); out.B = b.data
  const c = await call('POST', '/orders', { items: [{ product_id: 8, sku_id: 12, quantity: 1 }] }, tok); log('C', JSON.stringify(c).slice(0, 300)); out.C = c.data
  const w = await call('GET', '/wallet', null, tok); log('wallet', JSON.stringify(w).slice(0, 200))
  fs.writeFileSync(new URL('./orders.json', import.meta.url), JSON.stringify(out, null, 1))
}
