// F-041 guest checkout (up to payment), F-042 member-only, F-070/F-071 guest order lookup.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api } from './sf-lib.mjs'
const b = await launch()
const run = Date.now().toString(36)
const gEmail = `qa-sf-guest${run}@lab.test`, gPw = 'guestpw123'
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
for (const mobile of [false, true]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile }); await noAnnouncement(ctx)
  // F-042
  await go(page, `${S}/products/lab-e2e-card`, diag); await sleep(1000)
  const memberBtn = await page.getByRole('button', { name: /登录后购买/ }).first().isVisible().catch(() => false)
  const s42 = await shot(page, `F042-member-only-${m}`)
  let after42 = ''
  if (memberBtn) { await page.getByRole('button', { name: /登录后购买/ }).first().click(); await sleep(1500); after42 = page.url() }
  record({ flow: 'F-042', mobile, memberBtn, after42, shots: [s42] })
  // F-041 guest checkout
  await go(page, `${S}/products/google-account`, diag); await sleep(800)
  await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  const s1 = await shot(page, `F041-guest-checkout-${m}`, true)
  const t1 = await txt(page)
  const inputs = await page.evaluate(() => [...document.querySelectorAll('main input')].filter((e) => e.offsetParent).map((e) => `${e.type}:${e.placeholder}`))
  const em = page.locator('main input[type=email], main input[placeholder*="邮箱"]').first()
  if (await em.isVisible().catch(() => false)) await em.fill(gEmail)
  const pw = page.locator('main input[type=password], main input[placeholder*="密码"]').first()
  if (await pw.isVisible().catch(() => false)) await pw.fill(gPw)
  await sleep(800)
  const btn = page.getByRole('button', { name: /提交订单/ })
  const disabled = await btn.isDisabled().catch(() => 'n/a')
  const s2 = await shot(page, `F041-guest-checkout-filled-${m}`, true)
  record({ flow: 'F-041', mobile, inputs, disabled, t: t1.slice(t1.indexOf('订单结算'), t1.indexOf('订单结算') + 900), diag, shots: [s1, s2] })
  await ctx.close()
}
// create a pending guest order via API (no payment channel exists) for the lookup flows
const g = await api('guest/orders', { method: 'POST', body: { items: [{ product_id: 7, sku_id: 10, quantity: 1 }], email: gEmail, order_password: gPw } })
record({ flow: 'F-070-setup', g: [g.status_code, g.msg, g.data?.order_no, g.data?.status] })
for (const mobile of [false, true]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile }); await noAnnouncement(ctx)
  await go(page, `${S}/guest/orders`, diag); await sleep(1000)
  const s0 = await shot(page, `F070-guest-lookup-${m}`, true)
  const ins = page.locator('main input:visible')
  // wrong password
  await ins.nth(0).fill(gEmail); await ins.nth(1).fill('wrongpass')
  await page.getByRole('button', { name: /查询|查找|搜索/ }).first().click(); await sleep(2000)
  const wrong = { toast: (await toastsOf(page)).join('/'), t: (await txt(page)).slice(0, 700), shot: await shot(page, `F071-guest-wrong-pw-${m}`, true) }
  // unknown email
  await ins.nth(0).fill(`nobody${run}@lab.test`); await ins.nth(1).fill('wrongpass')
  await page.getByRole('button', { name: /查询|查找|搜索/ }).first().click(); await sleep(2000)
  const unknown = { toast: (await toastsOf(page)).join('/'), t: (await txt(page)).slice(0, 700) }
  await ins.nth(0).fill(gEmail); await ins.nth(1).fill(gPw)
  await page.getByRole('button', { name: /查询|查找|搜索/ }).first().click(); await sleep(2000)
  const ok = { t: (await txt(page)).slice(0, 1200), shot: await shot(page, `F070-guest-found-${m}`, true), ss: await page.evaluate(() => sessionStorage.getItem('guest_order_auth') || localStorage.getItem('guest_order_auth')) }
  // open detail
  const link = page.getByText(/查看详情/).first()
  let detail = {}
  if (await link.isVisible().catch(() => false)) { await link.click(); await sleep(2500); detail = { url: page.url(), t: (await txt(page)).slice(0, 900), shot: await shot(page, `F070-guest-detail-${m}`, true) } }
  // clear saved credentials
  await go(page, `${S}/guest/orders`, diag); await sleep(1000)
  const clr = page.getByRole('button', { name: /清除/ }).first()
  const hasClear = await clr.isVisible().catch(() => false)
  if (hasClear) { await clr.click(); await sleep(800) }
  const afterClear = await page.evaluate(() => sessionStorage.getItem('guest_order_auth') || localStorage.getItem('guest_order_auth'))
  record({ flow: 'F-070/F-071', mobile, wrong, unknown, ok, detail, hasClear, afterClear, diag, shots: [s0] })
  await ctx.close()
}
await b.close()
