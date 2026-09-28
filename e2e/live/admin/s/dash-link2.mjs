export default async ({ page, log, shot, admin }) => {
  await page.goto(admin + '/products?product_id=4'); await page.waitForLoadState('networkidle'); await page.waitForTimeout(2500)
  log('url', page.url())
  log('text', (await page.locator('main').innerText()).slice(0, 1200).replace(/\n/g,' | '))
  log('dialogs', await page.locator('[role=dialog]').count())
  await shot(page, 'dash-alert-link', false)
}
