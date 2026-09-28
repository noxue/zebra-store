// F-007 stock display modes on the QA manual product (10 stock).
import { launch, open, go, shot, record, S, sleep, noAnnouncement, adm } from './sf-lib.mjs'
import fs from 'node:fs'
const fx = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
const p = (await adm(`products/${fx.manual}`)).data
const put = (mode) => adm(`products/${fx.manual}`, { method: 'PUT', body: { ...p, stock_display_mode: mode, payment_channel_ids: [], skus: p.skus.map((s) => ({ id: s.id, sku_code: s.sku_code, spec_values: s.spec_values, price_amount: s.price_amount, cost_price_amount: s.cost_price_amount, manual_stock_total: s.manual_stock_total, is_active: s.is_active, sort_order: s.sort_order })) } })
const b = await launch()
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
const out = {}
for (const mode of ['exact', 'status', 'range', 'hidden']) {
  const r = await put(mode)
  await go(page, `${S}/products/qa-sf-manual`, diag); await sleep(1500)
  const t = (await page.locator('body').innerText()).replace(/\s+/g, ' ')
  out[mode] = { put: [r.status_code, r.msg], badge: t.match(/人工交付 (\S+)/)?.[1], sku: t.match(/选择规格 .{0,50}/)?.[0], shot: await shot(page, `F007-stock-${mode}-d`) }
}
await put('exact')
record({ flow: 'F-007', out, diag })
await b.close()
