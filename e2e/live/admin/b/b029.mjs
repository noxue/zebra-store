export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  const reg = (await api('GET', '/admin/settings?key=registration_config')).data
  const oc = (await api('GET', '/admin/settings?key=order_config')).data
  log('current order_config', JSON.stringify(oc), 'reg', JSON.stringify(reg))
  try {
    await page.goto(`${admin}/settings?tab=basic`); await page.waitForLoadState('networkidle')
    await page.locator('main [role=switch]').first().click() // 开放注册 off
    // domain allowlist on with empty list
    await page.locator('main [role=switch]').nth(2).click(); await page.waitForTimeout(300)
    await shot(page, 'b-029-allowlist-on')
    const n0 = problems.length
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    log('save', JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings?key=registration_config')).data))
    const sp = await b.newPage({ viewport: { width: 1440, height: 900 } })
    await sp.goto(base + '/auth/register'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1200)
    await shot(sp, 'b-029-storefront-register-closed', false)
    log('register page', (await sp.locator('main, body').first().innerText()).slice(0, 300).replace(/\n/g, ' / '))
    const r = await sp.evaluate(async () => (await fetch('/api/v1/auth/register', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'qa-reg-closed@lab.test', password: 'Qa123456!', agreement_accepted: true }) })).json())
    log('register api when closed', JSON.stringify(r).slice(0, 200))
    // order config
    await page.getByText('订单支付超时分钟数').first().waitFor()
    const oi = page.locator('xpath=//label[contains(.,"订单支付超时分钟数")]/following-sibling::div[1]//input').first()
    await oi.fill('-5'); await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    log('neg timeout -> order_config', JSON.stringify((await api('GET', '/admin/settings?key=order_config')).data))
    await shot(page, 'b-030-neg-timeout', false)
  } finally {
    log('restore reg', (await api('PUT', '/admin/settings', { key: 'registration_config', value: reg })).status_code, JSON.stringify((await api('GET', '/admin/settings?key=registration_config')).data))
    log('restore order', (await api('PUT', '/admin/settings', { key: 'order_config', value: oc })).status_code, JSON.stringify((await api('GET', '/admin/settings?key=order_config')).data))
  }
}
