export default async ({ page, log, admin }) => {
  await page.goto(admin + '/users/8'); await page.waitForLoadState('networkidle')
  await page.getByText('查看订单列表').click(); await page.waitForTimeout(1500); log('orders url', page.url(), await page.locator('main input[placeholder="用户ID"]').inputValue().catch(() => '?'), await page.locator('main tbody tr').count())
  await page.goBack(); await page.waitForTimeout(1200)
  await page.getByText('查看支付列表').click(); await page.waitForTimeout(1500); log('payments url', page.url(), await page.locator('main input[placeholder="用户ID"]').inputValue().catch(() => '?'), await page.locator('main tbody tr').count())
}
