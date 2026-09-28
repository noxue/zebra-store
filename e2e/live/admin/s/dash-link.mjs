export default async ({ page, log, shot, admin, api }) => {
  await page.goto(admin + '/'); await page.waitForLoadState('networkidle')
  const links = page.locator('main a')
  log('links', await links.evaluateAll((as) => as.map((a) => a.getAttribute('href') + ' ' + a.textContent.trim().slice(0, 30))))
  const link = page.locator('main a[href*="product_id"]').first()
  if (await link.count()) { const href = await link.getAttribute('href'); await link.click(); await page.waitForLoadState('networkidle'); await page.waitForTimeout(800); log('alert link', href, '->', page.url()); await shot(page, 'dash-alert-link'); log('products text', (await page.locator('main').innerText()).slice(0, 900).replace(/\n/g,' | ')) }
  else log('no alert link')
  const rk = await api('GET', '/admin/dashboard/rankings?range=7d'); log('rankings', JSON.stringify(rk.data).slice(0, 900))
}
