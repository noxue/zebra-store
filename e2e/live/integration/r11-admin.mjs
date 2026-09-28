import { admin, user, browser, adminCtx, shot, note, sleep } from './lib.mjs'
import { execSync } from 'node:child_process'
const s = await admin('store')
const ov = await s.get('/admin/resellers/operations/overview'); note('R-018', 'overview: ' + JSON.stringify(ov).slice(0, 400))
const fin = await s.get('/admin/resellers/operations/finance'); note('R-018', 'finance: ' + JSON.stringify(fin).slice(0, 400))
const [bal] = await s.page('/admin/resellers/balance-accounts?page=1&page_size=50'); note('R-018', 'balances: ' + JSON.stringify(bal.map((x) => [x.reseller_id, x.available_amount, x.locked_amount, x.negative_amount])))
const [led] = await s.page('/admin/resellers/ledger-entries?page=1&page_size=200')
const sum = {}; for (const e of led) { const k = e.reseller_id; sum[k] = sum[k] || { available: 0, locked: 0, pending: 0, withdrawn: 0 }; const st = e.status === 'pending_confirm' ? 'pending' : e.status; sum[k][st] = +((sum[k][st] || 0) + Number(e.amount)).toFixed(2) }
note('R-018', 'ledger sums by reseller/status: ' + JSON.stringify(sum))
const b = await browser(); const ctx = await adminCtx(b, 'store', s.token); const p = await ctx.newPage()
for (const [path, n] of [['resellers/operations', 'R018-01-overview'], ['resellers/ledger-entries', 'R018-02-ledger'], ['resellers/balance-accounts', 'R018-03-balances'], ['resellers/site-configs', 'R019-01-site-configs'], ['resellers/product-settings', 'R019-02-product-settings']]) { await p.goto('https://store.dot2.com/admin/' + path); await sleep(3000); await shot(p, 'reseller', n) }
// R-019 admin edits qa-rsl site config, then reset
const cfg = await s.get('/admin/resellers/site-configs/4'); const put = await s.raw('PUT', '/admin/resellers/site-configs/4', { ...(cfg.config || cfg), site_name: 'QA 管理员代改名' })
note('R-019', `admin put site-config: sc=${put.json.status_code} ${put.json.msg}`)
const edge = () => execSync("./edgeget.sh qaint.dot2.com /api/v1/public/config").toString().split('\n').pop()
note('R-019', 'qaint site_name now: ' + JSON.parse(edge()).data.brand.site_name)
const rs = await s.raw('POST', '/admin/resellers/site-configs/4/reset', {}); note('R-019', `admin reset: sc=${rs.json.status_code} ${rs.json.msg}; qaint site_name now: ` + JSON.parse(edge()).data.brand.site_name)
await b.close()
