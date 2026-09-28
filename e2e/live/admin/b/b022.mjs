import fs from 'node:fs'
export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  const bk = JSON.parse(fs.readFileSync(fs.readdirSync('live/admin/out').filter(f => f.startsWith('b-backup-')).sort().map(f => 'live/admin/out/' + f)[0], 'utf8'))
  const orig = bk['settings:site_config'].data
  try {
    await page.goto(`${admin}/settings?tab=template`); await page.waitForLoadState('networkidle')
    const prim = page.locator('input[value="#ff5fa2"]').last()
    // invalid first
    // skip invalid
    await shot(page, 'b-022-invalid-color', false)
    log('after invalid save', JSON.stringify((await api('GET', '/admin/settings?key=site_config')).data.theme))
    log('toasts', await page.locator('[role=alert], .toast, [class*=toast]').allInnerTexts().catch(() => []))
    await page.goto(`${admin}/settings?tab=template`); await page.waitForLoadState('networkidle')
    const t = (await api('GET', '/admin/settings?key=site_config')).data.theme
    await page.locator(`input[value="${t.primary_color}"]`).last().fill('#00aa55')
    await page.getByText('樱花飘落特效').click()
    await page.getByText('暗色', { exact: true }).click()
    await page.getByText('列表模式', { exact: true }).click()
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    const sc = (await api('GET', '/admin/settings?key=site_config')).data
    log('saved theme', JSON.stringify(sc.theme), sc.template_mode)
    const sp = await b.newPage({ viewport: { width: 1440, height: 900 } })
    await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
    log('storefront', JSON.stringify(await sp.evaluate(() => { const cs = getComputedStyle(document.documentElement); const vars = {}; for (const n of ['--zs-primary', '--color-primary', '--primary', '--zs-color-primary']) vars[n] = cs.getPropertyValue(n); return { vars, dark: document.documentElement.className, sakura: !!document.querySelector('canvas') } })))
    await shot(sp, 'b-022-storefront-themed', false)
    // 恢复默认配色
    await page.goto(`${admin}/settings?tab=template`); await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: /恢复默认配色/ }).click(); await page.waitForTimeout(500)
    await shot(page, 'b-022-after-reset-default', false)
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    log('after reset', JSON.stringify((await api('GET', '/admin/settings?key=site_config')).data.theme))
    const lp = await b.newPage(); await lp.goto(admin + '/login'); await lp.waitForTimeout(1500); await shot(lp, 'b-022-admin-login', false)
  } finally {
    await api('PUT', '/admin/settings', { key: 'site_config', value: orig })
    const now = (await api('GET', '/admin/settings?key=site_config')).data
    log('restored equal?', JSON.stringify(now) === JSON.stringify(orig))
  }
}
