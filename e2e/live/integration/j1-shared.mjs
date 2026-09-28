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
// I-024 replay
const w0 = await walletOf(s, uid); const rn = String(Date.now()) + '12345'
const t1 = await call('/shared/commodity/trade', trade(rn)); const w1 = await walletOf(s, uid)
const t2 = await call('/shared/commodity/trade', trade(rn)); const w2 = await walletOf(s, uid)
note('I-024', `trade #1: ${t1.http} ${JSON.stringify(t1.j).slice(0, 220)}`)
note('I-024', `replay same request_no: ${t2.http} ${JSON.stringify(t2.j).slice(0, 220)}; stock type=${typeof t2.j.data?.stock}; wallet ${w0}->${w1}->${w2}`)
// I-025
const bad = await call('/shared/commodity/items', {}, { key: 'wrongkeywrongkeywrongkeywrongkey' }); note('I-025', `wrong sign: ${bad.http} ${JSON.stringify(bad.j)}`)
const arr = await fetch(S + '/shared/commodity/items', { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: 'app_id[]=1&sign=abc' }); note('I-025', `app_id[]=1: ${arr.status} ${(await arr.text()).slice(0, 120)}`)
const mg = await call('/shared/commodity/items', {}, { rawSign: '0e462097431906509019562988736854' }); note('I-025', `0e magic sign: ${mg.http} ${JSON.stringify(mg.j)}`)
const unk = await call('/shared/commodity/items', {}, { id: 999999 }); note('I-025', `unknown app_id: ${unk.http} ${JSON.stringify(unk.j)}`)
// tampered: sign computed for num=1 but send num=5
{ const pairs = Object.entries(trade(String(Date.now()) + '9')).map(([k, v]) => [k, String(v)]).concat([['app_id', String(app_id)]]); const sg = sign(pairs, app_key); pairs[2] = ['num', '5']; const r = await fetch(S + '/shared/commodity/trade', { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams([...pairs, ['sign', sg]]).toString() }); note('I-025', `tampered num: ${r.status} ${(await r.text()).slice(0, 120)}`) }
// I-026 balance 0
const wb = await walletOf(s, uid); await setWallet(s, uid, 0)
const nb = await call('/shared/commodity/trade', trade(String(Date.now()) + '77')); note('I-026', `wallet 0 trade: ${nb.http} ${JSON.stringify(nb.j).slice(0, 160)}; wallet after ${await walletOf(s, uid)}`)
await setWallet(s, uid, Number(wb))
await editProduct(s, st.qaB.product_id, { price: '0' })
const zp = await call('/shared/commodity/trade', trade(String(Date.now()) + '88')); note('I-026', `zero price trade: ${zp.http} ${JSON.stringify(zp.j).slice(0, 160)}; wallet ${await walletOf(s, uid)}`)
await editProduct(s, st.qaB.product_id, { price: '3.40' })
// I-027 rate limit
const codes = {}; let sample
await Promise.all(Array.from({ length: 320 }, async () => { const r = await call('/shared/commodity/items', {}); const k = `${r.http}/${r.j.code}`; codes[k] = (codes[k] || 0) + 1; if (r.j.code !== 200) sample = r.j }))
note('I-027', `320 items calls: ${JSON.stringify(codes)} sample=${JSON.stringify(sample)}`)
