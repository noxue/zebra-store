import fs from 'node:fs'
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/orders'); await page.waitForLoadState('networkidle')
  const tryStatus = async (k, label) => {
    await page.getByPlaceholder('订单号').fill(O[k].order_no || O[k].order.order_no); await page.waitForTimeout(1500)
    const row = page.locator('tbody tr').first()
    await row.locator('select').selectOption({ label })
    const [res] = await Promise.all([page.waitForResponse(r => r.request().method() !== 'GET' && r.url().includes('/admin/orders/'), { timeout: 6000 }).catch(() => null), row.getByRole('button', { name: '更新状态' }).click()])
    await page.waitForTimeout(400)
    const c = page.locator('[role=dialog] button', { hasText: /确认|确定/ }).last()
    let r2 = res
    if (!res && await c.isVisible().catch(() => false)) { [r2] = await Promise.all([page.waitForResponse(r => r.request().method() !== 'GET' && r.url().includes('/admin/orders/'), { timeout: 6000 }).catch(() => null), c.click()]) ; log('(confirm dialog)') }
    await page.waitForTimeout(1200)
    log(k, '->', label, r2 ? JSON.stringify(r2.request().postDataJSON()) + ' ' + (await r2.text()).slice(0, 160) : 'NO REQUEST', '| toast:', (await page.locator('[role=status],[role=alert]').allInnerTexts().catch(() => [])).join('|').slice(0, 120))
    await shot(page, `d-order-status-${k}-${label}`, false)
  }
  await tryStatus('B', '待支付')
  await tryStatus('B', '已退款')
  await tryStatus('C', '已完成')
  await tryStatus('C', '已取消')
  const c = await api('GET', '/admin/orders?order_no=' + O.C.order_no); log('C now', c.data?.[0]?.status)
  const a = await api('PATCH', `/admin/orders/${(await api('GET', '/admin/orders?order_no=' + O.A.order.order_no)).data[0].id}`, { status: 'pending_payment' }); log('A completed -> pending via API', JSON.stringify(a).slice(0, 160))
}
