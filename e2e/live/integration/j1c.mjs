import crypto from 'node:crypto'
import { load, note, admin, walletOf, setWallet, editProduct } from './lib.mjs'
const st = load(); const { app_id, app_key } = st.compat; const S = 'https://store.dot2.com'
const sign = (pairs, key) => { const kept = pairs.filter(([k, v]) => !(v === '' && !k.includes('['))); const top = (k) => k.split('[')[0]; const sorted = kept.map((p, i) => [p, i]).sort((a, b) => top(a[0][0]) < top(b[0][0]) ? -1 : top(a[0][0]) > top(b[0][0]) ? 1 : a[1] - b[1]).map((x) => x[0]); return crypto.createHash('md5').update(sorted.map(([k, v]) => `${k}=${v}`).join('&') + '&key=' + key).digest('hex') }
async function call(path, fields, { key = app_key, id = app_id, rawSign, extra = [] } = {}) {
  const pairs = [...Object.entries(fields).map(([k, v]) => [k, String(v)]), ['app_id', String(id)], ...extra]
  const sg = rawSign ?? sign(pairs, key)
  const body = new URLSearchParams([...pairs, ['sign', sg]]).toString()
  const r = await fetch(S + path, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body })
  const t = await r.text(); let j; try { j = JSON.parse(t) } catch { j = { raw: t.slice(0, 120) } }
  return { http: r.status, j }
}
const s = await admin('store'); const uid = st.acgDownUid
const trade = (rn) => ({ shared_code: String(st.qaB.product_id), contact: 'x', num: 1, card_id: 0, device: 0, race: '', request_no: rn })
// I-027 rate limit
const t0 = Date.now(); const codes = {}; let sample
await Promise.all(Array.from({ length: 340 }, async () => { const r = await call('/shared/authentication/connect', {}); const k = `${r.http}/${r.j.code}`; codes[k] = (codes[k] || 0) + 1; if (r.j.code !== 200) sample = r.j }))
note('I-027', `340 connect calls in ${Date.now() - t0} ms: ${JSON.stringify(codes)} sample=${JSON.stringify(sample)}`)
