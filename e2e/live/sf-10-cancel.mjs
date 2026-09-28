// F-053 cancel pending order via UI; F-072 order list filters; coupon usage returned.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, userToken } from './sf-lib.mjs'
const [orderNo] = process.argv.slice(2)
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
for (const mobile of [true, false]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile, storage: `.state/sf-${m}.json` }); await noAnnouncement(ctx)
  await go(page, S + '/me/orders', diag); await sleep(1500)
  const list = { t: (await txt(page)).slice(0, 1800), shot: await shot(page, `F072-orders-${m}`, true) }
  // filter by status
  const sel = page.locator('select:visible').first()
  let filt = {}
  if (await sel.count()) {
    const opts = await sel.locator('option').allInnerTexts()
    filt.opts = opts
  } else {
    await page.getByText('全部状态').first().click(); await sleep(500)
    filt.opts = (await page.locator('[role=option], [role=listbox] *').allInnerTexts()).slice(0, 20)
    filt.shotOpen = await shot(page, `F072-status-dropdown-${m}`)
    await page.getByRole('option', { name: /待支付/ }).first().click().catch(() => page.getByText('待支付').last().click())
  }
  if (await sel.count()) await sel.selectOption({ label: '待支付' }).catch(() => {})
  await page.getByRole('button', { name: /筛选/ }).first().click(); await sleep(1500)
  filt.pending = { t: (await txt(page)).match(/匹配订单总数.{0,200}/)?.[0], shot: await shot(page, `F072-filter-pending-${m}`, true) }
  // search by order no
  await page.getByRole('button', { name: /重置/ }).first().click(); await sleep(1000)
  await page.locator('input[placeholder*="订单号"]').fill(orderNo || 'DJ'); await page.getByRole('button', { name: /筛选/ }).first().click(); await sleep(1500)
  filt.search = { t: (await txt(page)).match(/匹配订单总数.{0,300}/)?.[0], shot: await shot(page, `F072-search-${m}`, true) }
  await page.getByText('余额充值订单').first().click(); await sleep(1500)
  filt.recharge = { t: (await txt(page)).slice(0, 600), shot: await shot(page, `F072-recharge-tab-${m}`, true) }
  record({ flow: 'F-072', mobile, list, filt, diag })
  if (mobile && orderNo) {
    await go(page, `${S}/orders/${orderNo}`, diag); await sleep(1500)
    const d0 = { t: (await txt(page)).slice(0, 1500), shot: await shot(page, `F053-pending-detail-${m}`, true) }
    const cancel = page.getByRole('button', { name: /取消订单/ }).first()
    const has = await cancel.isVisible().catch(() => false)
    let confirmShot, after
    if (has) {
      await cancel.click(); await sleep(800)
      confirmShot = await shot(page, `F053-cancel-confirm-${m}`)
      const confirm = page.getByRole('dialog').getByRole('button', { name: /确认|确定|取消订单/ }).last()
      if (await confirm.isVisible().catch(() => false)) await confirm.click()
      await sleep(2500)
      after = { t: (await txt(page)).slice(0, 700), shot: await shot(page, `F053-after-cancel-${m}`, true) }
    }
    const tok = await userToken('qa-sf-mmuh9ewag@lab.test')
    const pv = await api('orders/preview', { method: 'POST', body: { items: [{ product_id: 7, sku_id: 10, quantity: 3 }], coupon_code: 'QASF10' }, token: tok })
    record({ flow: 'F-053', d0, has, confirmShot, after, couponAfterCancel: { code: pv.status_code, msg: pv.msg, discount: pv.data?.discount_amount }, diag })
  }
  await ctx.close()
}
await b.close()
