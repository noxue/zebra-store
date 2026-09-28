import { user, browser, userCtx, shot, note, sleep, load } from './lib.mjs'
const st = load(); const u = await user('store', 'qa-rsl@lab.test'); const me = await u.get('/me'); const pid = st.qaA.product_id
const tries = [
  ['markup_percent 15', { sku_id: 0, is_listed: true, pricing_mode: 'markup_percent', markup_percent: '15' }],
  ['markup_percent 60 (> max 50)', { sku_id: 0, is_listed: true, pricing_mode: 'markup_percent', markup_percent: '60' }],
  ['markup_percent -10', { sku_id: 0, is_listed: true, pricing_mode: 'markup_percent', markup_percent: '-10' }],
  ['markup_fixed 1.00', { sku_id: 0, is_listed: true, pricing_mode: 'fixed_markup', fixed_markup_amount: '1.00' }],
  ['fixed_price 4.00 (below base 5.20)', { sku_id: 0, is_listed: true, pricing_mode: 'fixed_price', fixed_price_amount: '4.00' }],
  ['fixed_price 9.00 (> base*1.5=7.80)', { sku_id: 0, is_listed: true, pricing_mode: 'fixed_price', fixed_price_amount: '9.00' }],
  ['fixed_price 7.00', { sku_id: 0, is_listed: true, pricing_mode: 'fixed_price', fixed_price_amount: '7.00' }],
]
for (const [lbl, s] of tries) {
  const pv = await u.raw('POST', `/reseller/product-settings/${pid}/preview`, { settings: [s] })
  const sv = await u.raw('PUT', `/reseller/product-settings/${pid}`, { settings: [s] })
  note('R-009', `${lbl}: preview sc=${pv.json.status_code} ${pv.json.msg} ${JSON.stringify(pv.json.data).slice(0, 160)} | save sc=${sv.json.status_code} ${sv.json.msg}`)
}
const fin = await u.raw('PUT', `/reseller/product-settings/${pid}`, { settings: [tries[0][1]] }); note('R-009', 'final 15% saved: sc=' + fin.json.status_code)
process.exit(0)
const b = await browser(); const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage()
await p.goto('https://store.dot2.com/reseller/products'); await sleep(3500); await shot(p, 'reseller', 'R009-01-products')
const row = p.locator('tr, [class*=card]', { hasText: 'QA 对接卡 A' }).first(); console.log('row', (await row.innerText().catch(() => '')).replace(/\s+/g, ' ').slice(0, 300))
await b.close()
