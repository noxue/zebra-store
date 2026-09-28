// node i4-acgbuy.mjs <tag> <commodityId> <downUidKey> [race]
import { acgAdmin, acgMember, acgUser, admin, walletOf, load, note } from './lib.mjs'
const [tag, cid, uidKey, race] = process.argv.slice(2); const st = load()
const s = await acgAdmin(); await acgMember('qaacgbuyer')
const row = await acgUser(s, 'qaacgbuyer'); if (Number(row.balance) < 500) await s.post('/admin/api/user/recharge', { id: row.id, action: 1, amount: '1000', log: 'qa', total: 0 })
const m = await acgMember('qaacgbuyer')
const S = await admin('store'); const uid = uidKey === 'acgdown6' ? 6 : st[uidKey]
const w0 = await walletOf(S, uid)
const body = { item_id: cid, num: 1, pay_id: 1, contact: 'qaacgbuyer' }; if (race) body.race = race
const t0 = Date.now(); const out = await m.post('/user/api/order/trade', body)
const w1 = await walletOf(S, uid)
note(tag, `ACG member trade item #${cid}${race ? ' race=' + race : ''}: ${Date.now() - t0}ms code=${out.code} msg=${out.msg} tradeNo=${out.data?.tradeNo} secret=${JSON.stringify(out.data?.secret || '').slice(0, 80)}; store wallet(uid ${uid}) ${w0} -> ${w1}`)
