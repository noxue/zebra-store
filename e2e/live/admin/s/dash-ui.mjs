export default async ({ page, log, shot, admin, api }) => {
  const reqs = []
  page.on('request', (r) => { if (r.url().includes('/dashboard/')) reqs.push(r.url().replace(/^.*\/api\/v1/, '')) })
  await page.goto(admin + '/'); await page.waitForLoadState('networkidle')
  const sel = page.locator('main select').first()
  log('options', await sel.locator('option').allTextContents())
  for (const v of await sel.locator('option').evaluateAll((os) => os.map((o) => o.value))) {
    reqs.length = 0
    const t0 = Date.now()
    await sel.selectOption(v); await page.waitForLoadState('networkidle'); await page.waitForTimeout(500)
    const kpi = await page.locator('main').innerText()
    log('range', v, Date.now() - t0, 'ms', reqs.join(' ; '))
    log('  text', kpi.slice(0, 300).replace(/\n/g, ' | '))
    await shot(page, `dash-range-${v}`)
  }
  reqs.length = 0
  await page.getByRole('button', { name: /强制刷新|刷新/ }).first().click(); await page.waitForLoadState('networkidle')
  log('force refresh', reqs.join(' ; '))
  // alert link
  await sel.selectOption('7d'); await page.waitForLoadState('networkidle')
  const link = page.locator('main a[href*="product_id"]').first()
  if (await link.count()) { const href = await link.getAttribute('href'); await link.click(); await page.waitForLoadState('networkidle'); log('alert link', href, '->', page.url()); await shot(page, 'dash-alert-link'); log('products text', (await page.locator('main').innerText()).slice(0, 600).replace(/\n/g,' | ')) }
  else log('no alert link')
  const rk = await api('GET', '/admin/dashboard/rankings?range=7d'); log('rankings', JSON.stringify(rk.data).slice(0, 800))
}
