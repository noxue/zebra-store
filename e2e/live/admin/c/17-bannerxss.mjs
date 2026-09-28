export default async ({ api, log, base, b }) => {
  const bn = (await api('GET', '/admin/banners/1')).data
  const u = await api('PUT', '/admin/banners/1', { ...bn, open_in_new_tab: true, link_value: 'javascript:document.title="QA-XSS-"+document.domain' })
  log('update', u.status_code, u.msg)
  const ctx = await b.newContext({ viewport: { width: 1440, height: 900 } })
  const sp = await ctx.newPage()
  await sp.goto(`${base}/`); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
  await sp.getByRole('button', { name: 'Close' }).click().catch(() => {})
  await sp.waitForTimeout(500)
  log('buttons', (await sp.locator('main button, main a').allInnerTexts()).slice(0, 8).join(' / '))
  const popP = ctx.waitForEvent('page', { timeout: 5000 }).catch(() => null)
  const btn = sp.getByRole('button', { name: /Learn more|了解更多|Learn More/i }).first()
  log('cta count', await btn.count())
  if (await btn.count()) await btn.evaluate((e) => e.click())
  const pop = await popP
  await sp.waitForTimeout(1500)
  log('popup', !!pop, pop ? await pop.title().catch((e) => 'err ' + e.message) : '', 'opener title', await sp.title())
  await ctx.close()
  const u2 = await api('PUT', '/admin/banners/1', { ...bn, open_in_new_tab: false, link_value: 'https://example.com/qa' })
  log('restore', u2.status_code, (await api('GET', '/admin/banners/1')).data.link_value)
}
