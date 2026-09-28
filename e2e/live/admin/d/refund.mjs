import fs from 'node:fs'
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/orders'); await page.waitForLoadState('networkidle')
  const open = async (no) => {
    await page.getByPlaceholder('订单号').fill(no); await page.waitForTimeout(1500)
    await page.locator('tbody tr').first().getByRole('button', { name: '查看详情' }).click(); await page.waitForTimeout(1500)
    return page.locator('[role=dialog]').last()
  }
  const close = async () => { await page.keyboard.press('Escape'); await page.waitForTimeout(500) }
  const submit = async (dlg, kind, amount, remark) => {
    const form = dlg.locator('form').last()
    await form.locator('input').nth(0).fill(amount); await form.locator('input').nth(1).fill(remark)
    const [res] = await Promise.all([page.waitForResponse(r => r.request().method() === 'POST' && /refund/.test(r.url()), { timeout: 6000 }).catch(() => null), form.locator('button[type=submit]').click()])
    await page.waitForTimeout(400)
    const c = page.locator('[role=dialog] button', { hasText: /^确认$|^确定$/ }).last()
    let r = res
    if (!res && await c.isVisible().catch(() => false)) { [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /refund/.test(x.url()), { timeout: 6000 }).catch(() => null), c.click()]); log('(confirm)') }
    await page.waitForTimeout(1200)
    const err = (await dlg.innerText()).match(/可退款金额：[^\n|]*/)?.[0]
    log(kind, amount, r ? new URL(r.url()).pathname + ' ' + (await r.text()).slice(0, 150) : 'NO REQUEST', '|', err, '| msgs:', (await dlg.locator('.text-danger-text,.text-success-text,[class*=danger]').allInnerTexts()).join('|').slice(0, 150))
  }
  let dlg = await open(O.A.order.order_no)
  await submit(dlg, 'wallet', '0', 'zero'); await submit(dlg, 'wallet', '-1', 'neg'); await submit(dlg, 'wallet', '100', 'over')
  await submit(dlg, 'wallet', '3', 'qa partial wallet refund'); await shot(page, 'd-refund-A-partial', false)
  await close(); dlg = await open(O.A.order.order_no)
  log('A detail status', (await dlg.innerText()).match(/订单状态[\s|]*([^\n]+)/)?.[1])
  await dlg.getByRole('button', { name: '手动退款' }).or(dlg.getByRole('tab', { name: '手动退款' })).first().click(); await page.waitForTimeout(400)
  await submit(dlg, 'manual', '7', 'over manual'); await submit(dlg, 'manual', '6.50', 'qa manual rest')
  await dlg.screenshot({ path: 'live/shots/admin/d-refund-A-full.png' }).catch(() => {})
  log('A now', JSON.stringify((await api('GET', '/admin/orders?order_no=' + O.A.order.order_no)).data[0]).match(/"status":"[^"]+"|"refunded_amount":"[^"]+"/g))
  await close()
  // canceled unpaid C: refund card shown?
  dlg = await open(O.C.order_no); log('C refund card', /退款处理/.test(await dlg.innerText())); await close()
  // B marked refunded via status w/o money: can still refund to wallet?
  dlg = await open(O.B.order.order_no); const t = await dlg.innerText(); log('B refund card', /退款处理/.test(t), t.match(/可退款金额：[^\n]*/)?.[0])
  if (/退款处理/.test(t)) await submit(dlg, 'wallet', '160.55', 'qa restore after status-only refunded')
  await close()
  log('B now', JSON.stringify((await api('GET', '/admin/orders?order_no=' + O.B.order.order_no)).data[0]).match(/"status":"[^"]+"|"refunded_amount":"[^"]+"/g))
  // refunds page
  await page.goto(admin + '/order-refunds'); await page.waitForLoadState('networkidle'); await page.waitForTimeout(800)
  await shot(page, 'd-order-refunds')
  log('refunds page', (await page.locator('main tbody').innerText()).replace(/\s+/g, ' ').slice(0, 600))
}
