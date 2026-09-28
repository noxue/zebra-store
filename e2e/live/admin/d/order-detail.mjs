import fs from 'node:fs'
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
export default async ({ page, shot, log, admin }) => {
  await page.goto(admin + '/orders'); await page.waitForLoadState('networkidle')
  for (const k of ['B', 'A']) {
    const no = O[k].order.order_no
    await page.getByPlaceholder('订单号').fill(no); await page.waitForTimeout(1500)
    const row = page.locator('tbody tr').first()
    log(k, 'row', (await row.innerText()).replace(/\s+/g, ' ').slice(0, 300))
    log(k, 'row select opts', await row.locator('select').count() ? (await row.locator('select option').allInnerTexts()).join('/') : 'no select')
    await row.getByRole('button', { name: '查看详情' }).click(); await page.waitForTimeout(1500)
    const dlg = page.locator('[role=dialog]').last()
    await shot(page, `d-order-detail-${k}`, false)
    // scroll dialog content screenshot
    const txt = await dlg.innerText(); log(k, 'detail text', txt.replace(/\n+/g, ' | ').slice(0, 1500))
    await dlg.screenshot({ path: `live/shots/admin/d-order-detail-${k}-dialog.png` }).catch(() => {})
    await page.keyboard.press('Escape'); await page.waitForTimeout(500)
    await page.getByPlaceholder('订单号').fill('')
  }
}
