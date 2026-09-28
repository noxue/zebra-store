import { user, admin, walletOf, load, note } from './lib.mjs'
const [tag, who = 'reseller-sakura@lab.test'] = process.argv.slice(2)
const r = await user('store', who, { register: false })
const acc = await r.get('/reseller/balance-accounts'); const [led] = await r.page('/reseller/ledger-entries?page=1&page_size=5')
note(tag, `${who} balances: ${JSON.stringify(acc)} | last ledger: ${JSON.stringify((led || []).slice(0, 3).map((e) => [e.id, e.type, e.amount, e.status, e.order_id]))}`)
