export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  try {
    await page.goto(`${admin}/settings?tab=navigation`); await page.waitForLoadState('networkidle')
    await page.getByRole('switch').first().click()
    await page.getByRole('button', { name: /添加导航项/ }).click(); await page.waitForTimeout(500)
    await shot(page, 'b-024-add-item')
    // save empty item to test validation
    const n0 = problems.length
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    await shot(page, 'b-024-empty-save', false)
    log('empty save', JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings?key=nav_config')).data))
    const inputs = page.locator('main input:visible')
    log('inputs', await inputs.count(), await page.locator('main input:visible').evaluateAll(els => els.map(e => e.placeholder)))
    const ph = await page.locator('main input:visible').evaluateAll(els => els.map(e => e.placeholder))
    for (let i = 0; i < ph.length; i++) {
      if (/terms|url|链接|http/i.test(ph[i])) await inputs.nth(i).fill('https://example.com/qa-nav')
      else if (ph[i] === '0') await inputs.nth(i).fill('1')
      else await inputs.nth(i).fill('QA外链')
    }
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    log('saved nav', JSON.stringify((await api('GET', '/admin/settings?key=nav_config')).data))
    const sp = await b.newPage({ viewport: { width: 1440, height: 900 } })
    await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1000)
    log('storefront nav', JSON.stringify(await sp.evaluate(() => [...document.querySelectorAll('header a, header button')].map(a => a.innerText.trim() + '|' + (a.getAttribute('href') || '') + '|' + (a.getAttribute('target') || '')).filter(s => s.length > 2))))
    await shot(sp, 'b-024-storefront', false)
  } finally {
    log('restore', JSON.stringify(await api('PUT', '/admin/settings', { key: 'nav_config', value: {} })))
    log('now', JSON.stringify((await api('GET', '/admin/settings?key=nav_config')).data))
  }
}
