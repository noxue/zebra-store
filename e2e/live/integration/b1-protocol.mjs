import { zs, zsDecrypt, load, save, admin, editProduct, setWallet, walletOf, note, sleep } from './lib.mjs'
const st = load(); const { key, secret } = st.qaCred; const S = 'https://store.dot2.com'
const s = await admin('store'); const uid = st.qaZ2Uid
await setWallet(s, uid, 100)
const call = (m, p, o) => zs(S, key, secret, m, p, o)
const w = async () => walletOf(s, uid)
const A = st.qaA, B = st.qaB
// ---- I-067 quote lock, price rises after quote
let w0 = await w()
const q1 = await call('POST', '/orders/quote', { body: { items: [{ sku_id: A.sku_id, quantity: 1 }] } })
note('I-067', `quote@5.00: http ${q1.http} ${JSON.stringify(q1.json.data)}`)
await editProduct(s, A.product_id, { price: '6.00' })
const o1 = await call('POST', '/orders', { body: { quote_id: q1.json.data.quote_id, items: [{ sku_id: A.sku_id, quantity: 1 }], downstream_order_no: 'QA-DN-1-' + Date.now(), callback: false }, headers: { 'Idempotency-Key': 'qa-k1-' + Date.now() } })
let w1 = await w()
note('I-067', `price raised to 6.00 then order with quote: http ${o1.http} status=${o1.json.data?.status} unit=${o1.json.data?.items?.[0]?.unit_price} total=${o1.json.data?.total}; wallet ${w0} -> ${w1}`)
// price lowered after quote: pay current
const q2 = await call('POST', '/orders/quote', { body: { items: [{ sku_id: A.sku_id, quantity: 1 }] } })
await editProduct(s, A.product_id, { price: '4.50' })
const o2 = await call('POST', '/orders', { body: { quote_id: q2.json.data.quote_id, items: [{ sku_id: A.sku_id, quantity: 1 }], downstream_order_no: 'QA-DN-2-' + Date.now() }, headers: { 'Idempotency-Key': 'qa-k2-' + Date.now() } })
let w2 = await w()
note('I-067', `quote@${q2.json.data?.items?.[0]?.unit_price}, price lowered to 4.50, order: http ${o2.http} unit=${o2.json.data?.items?.[0]?.unit_price} total=${o2.json.data?.total}; wallet ${w1} -> ${w2}`)
await editProduct(s, A.product_id, { price: '5.00' })
// quote mismatch
const q3 = await call('POST', '/orders/quote', { body: { items: [{ sku_id: A.sku_id, quantity: 1 }] } })
const o3 = await call('POST', '/orders', { body: { quote_id: q3.json.data.quote_id, items: [{ sku_id: A.sku_id, quantity: 2 }], downstream_order_no: 'QA-DN-3-' + Date.now() }, headers: { 'Idempotency-Key': 'qa-k3-' + Date.now() } })
note('I-067', `order qty differs from quote: http ${o3.http} ${JSON.stringify(o3.json).slice(0, 200)}`)
st.q_expire = { quote_id: q3.json.data.quote_id, expires_at: q3.json.data.expires_at }; save(st)
// ---- I-070 encrypted delivery
const d = o1.json.data.items[0].delivery
note('I-070', `delivery object keys=${Object.keys(d || {}).join(',')} encrypted=${d?.encrypted} alg=${d?.alg}`)
if (d?.encrypted) note('I-070', 'decrypted: ' + JSON.stringify(zsDecrypt(d, secret)).slice(0, 200))
// ---- I-069 idempotency
const idem = 'qa-idem-' + Date.now(); const dn = 'QA-DN-IDEM-' + Date.now()
const body = { items: [{ sku_id: B.sku_id, quantity: 1 }], downstream_order_no: dn }
const i1 = await call('POST', '/orders', { body, headers: { 'Idempotency-Key': idem } })
let w3 = await w()
const i2 = await call('POST', '/orders', { body, headers: { 'Idempotency-Key': idem } })
let w4 = await w()
note('I-069', `same key twice: ${i1.http}/${i1.json.data?.order_no} then ${i2.http}/${i2.json.data?.order_no} status=${i2.json.data?.status}; wallet ${w2}->${w3}->${w4}`)
const i3 = await call('POST', '/orders', { body: { ...body, items: [{ sku_id: B.sku_id, quantity: 2 }] }, headers: { 'Idempotency-Key': idem } })
note('I-069', `same key different body: http ${i3.http} ${JSON.stringify(i3.json).slice(0, 250)}`)
const i4 = await call('POST', '/orders', { body, headers: { 'Idempotency-Key': idem + '-other' } })
let w5 = await w()
note('I-069', `same downstream_order_no new key: http ${i4.http} order=${i4.json.data?.order_no} (orig ${i1.json.data?.order_no}); wallet ${w4}->${w5}`)
const i5 = await call('POST', '/orders', { body })
note('I-069', `missing Idempotency-Key: http ${i5.http} ${JSON.stringify(i5.json).slice(0, 200)}`)
// ---- I-068 multi item direct
const m1 = await call('POST', '/orders', { body: { items: [{ sku_id: A.sku_id, quantity: 1 }, { sku_id: B.sku_id, quantity: 2 }], downstream_order_no: 'QA-DN-M-' + Date.now() }, headers: { 'Idempotency-Key': 'qa-m-' + Date.now() } })
let w6 = await w()
note('I-068', `multi-item direct: http ${m1.http} order=${m1.json.data?.order_no} status=${m1.json.data?.status} total=${m1.json.data?.total} items=${JSON.stringify(m1.json.data?.items?.map((i) => [i.sku_id, i.quantity, i.unit_price, i.status, !!i.delivery]))}; wallet ${w5}->${w6}`)
if (m1.json.data?.items) for (const it of m1.json.data.items) if (it.delivery) note('I-068', 'row decrypt: ' + zsDecrypt(it.delivery, secret).payload?.slice(0, 80))
// lookups
const g = await call('GET', '/orders', { query: 'downstream_order_no=' + dn }); note('I-069', `list by downstream_order_no: http ${g.http} n=${g.json.data?.items?.length}`)
const c = await call('POST', `/orders/${i1.json.data.order_no}/cancel`); note('I-069', `cancel paid order: http ${c.http} ${JSON.stringify(c.json).slice(0, 200)}`)
st.o1 = o1.json.data.order_no; st.m1 = m1.json.data?.order_no; save(st)
