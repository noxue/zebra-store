import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
export default async ({ page, api, log }) => {
  const call = (m, p, b, tok) => page.evaluate(async ({ m, p, b, tok }) => (await (await fetch('/api/v1' + p, { method: m, headers: { 'content-type': 'application/json', ...(tok ? { authorization: 'Bearer ' + tok } : {}) }, body: b ? JSON.stringify(b) : undefined })).json()), { m, p, b, tok })

  const tok = (await call('POST', '/auth/login', { email: U.email, password: U.password })).data.token
  for (const k of ['A', 'B']) {
    const no = O[k].order?.order_no
    const d = await call('GET', '/orders/' + no, null, tok); log(k, 'status', d.data?.status, d.data?.id)
    if (d.data?.status === 'pending_payment') { const r = await call('POST', '/payments', { order_no: no, use_balance: true }, tok); log(k, 'pay', JSON.stringify(r).slice(0, 250)) }
    const d2 = await call('GET', '/orders/' + no, null, tok); log(k, 'status now', d2.data?.status)
  }
}
