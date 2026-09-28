export default async ({ page, admin, shot, log, problems, api }) => {
  const orig = (await api('GET', '/admin/settings?key=upstream_sync_config')).data
  const dorig = (await api('GET', '/admin/settings?key=dashboard_config')).data
  log('orig', JSON.stringify(orig), JSON.stringify(dorig))
  try {
    await page.goto(`${admin}/settings?tab=upstream_sync`); await page.waitForLoadState('networkidle')
    const nums = page.locator('main input[type=number]')
    log('fields', await page.locator('main label').allInnerTexts(), 'values', await nums.evaluateAll(e => e.map(x => x.value)))
    await nums.first().fill('0')
    const n0 = problems.length
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    await shot(page, 'b-031-zero', false)
    log('zero ->', JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings?key=upstream_sync_config')).data))
    await nums.first().fill('7'); await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    log('7 ->', JSON.stringify((await api('GET', '/admin/settings?key=upstream_sync_config')).data))
    await page.goto(`${admin}/settings?tab=dashboard`); await page.waitForLoadState('networkidle')
    log('dash fields', await page.locator('main label').allInnerTexts())
  } finally {
    await api('PUT', '/admin/settings', { key: 'upstream_sync_config', value: orig })
    log('restored', JSON.stringify((await api('GET', '/admin/settings?key=upstream_sync_config')).data))
  }
}
