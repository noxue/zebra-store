import { zs, load, admin } from './lib.mjs'
const st = load(); const { key, secret } = st.qaCred
const h = await zs('https://store.dot2.com', key, secret, 'GET', '/handshake'); console.log(h.http, JSON.stringify(h.json).slice(0, 700))
const p = await zs('https://store.dot2.com', key, secret, 'GET', '/catalog/products/11'); console.log(p.http, JSON.stringify(p.json).slice(0, 900))
const z = await admin('zs2'); const pr = await z.get('/admin/products/2'); console.log(JSON.stringify(pr.skus[0]))
