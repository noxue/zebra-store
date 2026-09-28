import { zs, zsHeaders, load, note } from './lib.mjs'
const st = load(); const { key, secret } = st.qaCred; const S = 'https://store.dot2.com'
// replay same nonce
const nonce = 'qareplaynonce' + Date.now()
const ts = Math.floor(Date.now() / 1000)
const a = await zs(S, key, secret, 'GET', '/handshake', { sig: { ts, nonce } })
const b = await zs(S, key, secret, 'GET', '/handshake', { sig: { ts, nonce } })
note('I-072', `nonce replay: first ${a.http}, replay ${b.http} ${JSON.stringify(b.json)}`)
const c = await zs(S, key, secret, 'GET', '/handshake', { sig: { ts: ts - 400 } })
note('I-072', `timestamp -400s: ${c.http} ${JSON.stringify(c.json)}`)
const d = await zs(S, key, 'wrong-secret', 'GET', '/handshake')
note('I-072', `bad signature: ${d.http} ${JSON.stringify(d.json)}`)
const e = await zs(S, key, secret, 'GET', '/handshake', { sig: { nonce: 'short' } })
note('I-072', `nonce too short: ${e.http} ${JSON.stringify(e.json).slice(0, 160)}`)
// query canonicalisation
const f = await zs(S, key, secret, 'GET', '/catalog/products', { query: 'limit=2&cursor=' })
note('I-072', `signed query limit=2&cursor=: ${f.http} n=${f.json.data?.items?.length} next=${f.json.data?.next_cursor} total=${f.json.data?.total}`)
// rate limit: 130 parallel-ish requests
const t0 = Date.now(); const codes = {}; let ra
await Promise.all(Array.from({ length: 130 }, async () => { const r = await zs(S, key, secret, 'GET', '/handshake'); codes[r.http] = (codes[r.http] || 0) + 1; if (r.http === 429) ra = [r.retryAfter, JSON.stringify(r.json)] }))
note('I-072', `130 requests in ${Date.now() - t0} ms: ${JSON.stringify(codes)} retry-after=${ra?.[0]} body=${ra?.[1]}`)
