import fs from 'node:fs'
const C = JSON.parse(fs.readFileSync('live/admin/d/channel.json'))
export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/payment-channels'); await page.waitForLoadState('networkidle')
  await page.locator('tbody tr', { hasText: 'QA-D' }).first().getByRole('button', { name: /Edit/ }).click(); await page.waitForTimeout(1000)
  const dlg = page.locator('[role=dialog]').last()
  await dlg.locator('xpath=.//label[contains(., "Payment Roles Limit")]/following-sibling::div[1]').first().click(); await page.waitForTimeout(400)
  await dlg.getByText('Member', { exact: true }).click(); await page.waitForTimeout(300)
  await dlg.getByText('Edit Channel').click(); await page.waitForTimeout(300)
  const [r2] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT' && /payment-channels/.test(x.url()), { timeout: 6000 }).catch(() => null), dlg.getByRole('button', { name: 'Save', exact: true }).click()])
  await page.waitForTimeout(700); log('roles save', r2 ? JSON.stringify(JSON.parse(r2.request().postData()).payment_roles) + ' => ' + (await r2.text()).slice(0, 100) : 'NO REQUEST')
  const d = (await api('GET', '/admin/payment-channels/' + C.id)).data; log('channel', JSON.stringify(d.payment_roles), d.is_active)
}
