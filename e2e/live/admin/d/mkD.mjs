import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, log }) => {
  const call = (m, p, b, tok) => page.evaluate(async ({ m, p, b, tok }) => (await (await fetch('/api/v1' + p, { method: m, headers: { 'content-type': 'application/json', ...(tok ? { authorization: 'Bearer ' + tok } : {}) }, body: b ? JSON.stringify(b) : undefined })).json()), { m, p, b, tok })
  const tok = (await call('POST', '/auth/login', { email: U.email, password: U.password })).data.token
  const d = await call('POST', '/orders/create-and-pay', { items: [{ product_id: 2, sku_id: 3, quantity: 1 }], use_balance: true }, tok)
  log('D', JSON.stringify(d).slice(0, 200))
  const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url))); O.D = d.data; fs.writeFileSync(new URL('./orders.json', import.meta.url), JSON.stringify(O, null, 1))
  const s = await call('GET', '/orders/' + d.data.order.order_no, null, tok); log('D status', s.data.status)
}
