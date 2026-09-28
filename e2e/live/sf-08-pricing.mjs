// F-045..F-049 pricing on the checkout page (member with 95% level) + F-043 wallet payment.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const summary = async (p) => { const t = await txt(p); const i = t.indexOf('商品数量'); return t.slice(i, i + 260) }
async function buyNow(page, diag, slug, sku, qty) {
  await go(page, `${S}/products/${slug}`, diag); await sleep(800)
  if (sku) await page.getByText(sku, { exact: true }).first().click()
  const q = page.locator('main input:visible').first()
  await q.fill(String(qty)); await page.keyboard.press('Tab'); await sleep(500)
  await page.getByRole('button', { name: /立即购买/ }).first().click()
  await page.waitForURL(/checkout/, { timeout: 15000 }); await sleep(2500)
}
async function coupon(page, code) {
  const inp = page.locator('input[placeholder*="优惠券"]')
  await inp.fill(code); await page.keyboard.press('Enter'); await inp.blur(); await sleep(2500)
}
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
const R = {}
// D: promotion on gcp
await buyNow(page, diag, 'gcp-account', null, 1)
R.promo = { sum: await summary(page), shot: await shot(page, 'F046-checkout-promo-d', true) }
await go(page, `${S}/products/gcp-account`, diag); await sleep(800)
R.promoDetail = { t: (await txt(page)).match(/价格.{0,60}/)?.[0], shot: await shot(page, 'F046-detail-promo-d') }
// A: coupon below min
await buyNow(page, diag, 'google-account', '新号', 2)
R.qty2 = { sum: await summary(page) }
await coupon(page, 'QASF10')
R.couponBelowMin = { sum: await summary(page), msg: (await txt(page)).match(/.{0,20}(门槛|最低|未满|不满足|不可用|无效).{0,30}/)?.[0], shot: await shot(page, 'F045-coupon-below-min-d', true) }
// B: wholesale x3 + coupon
await buyNow(page, diag, 'google-account', '新号', 3)
R.wholesale3 = { sum: await summary(page), shot: await shot(page, 'F048-wholesale-x3-d', true) }
await coupon(page, 'QASF10')
R.wholesaleCoupon = { sum: await summary(page), shot: await shot(page, 'F045-wholesale-coupon-d', true) }
// C: coupon that disables wholesale
await coupon(page, 'QASFNOWHL')
R.noWholesaleCoupon = { sum: await summary(page), shot: await shot(page, 'F049-coupon-no-wholesale-d', true) }
// bogus coupon
await coupon(page, 'NOPE-XYZ')
R.bogus = { sum: await summary(page), msg: (await txt(page)).match(/.{0,20}(优惠券).{0,40}/g)?.slice(0, 4), shot: await shot(page, 'F045-coupon-bogus-d', true) }
// submit with QASF10, wallet
await coupon(page, 'QASF10'); await sleep(500)
await page.getByText('优先使用余额支付').click(); await sleep(1500)
R.beforeSubmit = { sum: await summary(page), shot: await shot(page, 'F043-before-submit-d', true) }
await page.getByRole('button', { name: /提交订单/ }).click()
await sleep(5000)
R.afterSubmit = { url: page.url(), t: (await txt(page)).slice(0, 1500), shot: await shot(page, 'F043-after-submit-d', true) }
record({ flow: 'F-045..049,F-043', R, diag })
await ctx.storageState({ path: '.state/sf-d.json' })
await b.close()
