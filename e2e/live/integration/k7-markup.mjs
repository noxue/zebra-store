import { admin, updateConn, browser, adminCtx, shot, note, sleep, load } from './lib.mjs'
const st = load(); const s = await admin('store')
await updateConn(s, 3, '', { price_markup_percent: '25', price_rounding_mode: 'none' })
const b = await browser(); const ctx = await adminCtx(b, 'store', s.token); const p = await ctx.newPage()
p.on('response', async (r) => { if (r.url().includes('reapply-markup')) note('I-003', 'reapply -> ' + (await r.text()).slice(0, 200)) })
await p.goto('https://store.dot2.com/admin/site-connections'); await sleep(2500)
await p.locator('tr', { hasText: 'QA 异次元' }).getByRole('button', { name: '重新应用加价' }).click(); await sleep(1200)
await shot(p, 'integration', 'I003-01-reapply-confirm', false)
const c = p.getByRole('button', { name: /^(确认|确定|重新应用加价)$/ }); if (await c.count()) await c.last().click(); await sleep(4000)
for (const k of ['3:4', '3:5']) { const x = await s.get('/admin/products/' + st.sp[k].product_id); note('I-003', `${x.title['zh-CN']}: cost=${x.skus[0].cost_price_amount} price=${x.skus[0].price_amount} (markup 25%)`) }
await updateConn(s, 3, '', { price_markup_percent: '25', price_rounding_mode: 'ceil_int' }).catch((e) => note('I-003', 'rounding mode set failed ' + e.message.slice(0, 100)))
await b.close()
