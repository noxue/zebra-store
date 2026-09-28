import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, shot, log, admin }) => {
  await page.goto(admin + '/users/' + U.id); await page.waitForLoadState('networkidle')
  for (const tab of ['订单记录', '支付记录', '优惠券记录', '钱包明细']) {
    await page.getByRole('button', { name: tab }).or(page.getByRole('tab', { name: tab })).first().click(); await page.waitForTimeout(1200)
    const rows = await page.locator('main table').last().locator('tbody tr').allInnerTexts()
    log(tab, rows.length, rows.slice(0, 4).map(r => r.replace(/\s+/g, ' ').slice(0, 110)).join(' || '))
    await shot(page, 'd-user-tab-' + tab)
  }
  await page.getByRole('button', { name: '查看订单列表' }).click(); await page.waitForTimeout(1500); log('view orders url', page.url())
  await page.goBack(); await page.waitForTimeout(1000)
  await page.getByRole('button', { name: '查看支付列表' }).click(); await page.waitForTimeout(1500); log('view payments url', page.url(), (await page.locator('main input[placeholder="用户ID"]').inputValue().catch(() => '?')))
}
