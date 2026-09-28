export default async ({ page, shot, log, admin }) => {
  await page.goto(`${admin}/security`); await page.waitForLoadState('networkidle')
  await page.locator('header button[title]').first().click(); await page.waitForTimeout(800)
  await shot(page, 'a-sidebar-collapsed', false)
  const w = await page.locator('aside').first().evaluate((e) => e.getBoundingClientRect().width)
  log('aside width', w)
  await page.locator('header button[title]').first().click(); await page.waitForTimeout(500)
}
