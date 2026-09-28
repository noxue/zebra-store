import fs from 'node:fs'
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, shot, log, admin, api }) => {
  const no = O.D.order.order_no
  await page.goto(admin + '/orders'); await page.waitForLoadState('networkidle')
  await page.getByPlaceholder('订单号').fill(no); await page.waitForTimeout(1500)
  await page.locator('tbody tr').first().getByRole('button', { name: '查看详情' }).click(); await page.waitForTimeout(1500)
  await page.locator('[role=dialog]').last().getByRole('button', { name: '交付录入' }).click(); await page.waitForTimeout(800)
  const m = page.locator('[role=dialog]').last(); await shot(page, 'd-deliver-modal', false)
  // empty submit
  const [r0] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /fulfill/.test(x.url()), { timeout: 4000 }).catch(() => null), m.getByRole('button', { name: /提交|确认交付|交付/ }).last().click()])
  await page.waitForTimeout(700); log('empty submit', r0 ? (await r0.text()).slice(0, 150) : 'NO REQUEST', (await m.innerText().catch(() => '')).match(/请[^\n]*/g)?.join('/'))
  await m.locator('textarea').first().fill('QA 人工交付：账号 qa-seat@example.com / 密码 Seat#123')
  await m.getByRole('button', { name: /添加/ }).first().click().catch(() => {})
  const ins = m.locator('input'); log('inputs', await ins.count())
  if (await ins.count() >= 2) { await ins.nth(0).fill('账号'); await ins.nth(1).fill('qa-seat@example.com') }
  const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /fulfill/.test(x.url()), { timeout: 6000 }).catch(() => null), m.getByRole('button', { name: /提交|确认交付|交付/ }).last().click()])
  await page.waitForTimeout(1200)
  log('deliver', r ? new URL(r.url()).pathname + ' ' + r.request().postData() + ' => ' + (await r.text()).slice(0, 200) : 'NO REQUEST')
  await shot(page, 'd-deliver-after', false)
  const o = (await api('GET', '/admin/orders?order_no=' + no)).data[0]; log('D status', o.status, o.children?.map(c => c.status).join(','))
  // storefront view
  const tok = await page.evaluate(async (U) => (await (await fetch('/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: U.email, password: U.password }) })).json()).data.token, U)
  const sd = await page.evaluate(async ({ no, tok }) => (await (await fetch('/api/v1/orders/' + no, { headers: { authorization: 'Bearer ' + tok } })).json()), { no, tok })
  log('user sees', sd.data.status, JSON.stringify(sd.data.fulfillment || sd.data.children?.[0]?.fulfillment).slice(0, 300))
  // mark completed via list button
  await page.keyboard.press('Escape'); await page.waitForTimeout(300); await page.keyboard.press('Escape'); await page.waitForTimeout(300)
  await page.reload(); await page.waitForLoadState('networkidle'); await page.getByPlaceholder('订单号').fill(no); await page.waitForTimeout(1500)
  const btn = page.locator('tbody tr').first().getByRole('button', { name: '标记完成' })
  if (await btn.count()) { await btn.click(); await page.waitForTimeout(400); const c = page.locator('[role=dialog] button', { hasText: /^确认$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click(); await page.waitForTimeout(1200) }
  log('D after mark completed', (await api('GET', '/admin/orders?order_no=' + no)).data[0].status)
}
