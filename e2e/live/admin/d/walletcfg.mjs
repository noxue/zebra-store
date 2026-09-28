export default async ({ page, shot, log, admin, api }) => {
  const orig = (await api('GET', '/admin/settings?key=wallet_config')).data
  const pub = async () => page.evaluate(async () => { const j = await (await fetch('/api/v1/public/config')).json(); return JSON.stringify(j.data?.wallet ?? j.data?.wallet_only_payment ?? Object.keys(j.data)).slice(0, 300) })
  log('public before', await pub())
  try {
    await page.goto(admin + '/wallet-config'); await page.waitForLoadState('networkidle')
    await page.locator('main [role=switch]').first().click()
    const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), page.getByRole('button', { name: /保存/ }).click()])
    await page.waitForTimeout(800); log('save', r?.request().postData(), (await r?.text())?.slice(0, 200)); await shot(page, 'd-wallet-config-saved', false)
    log('public after', await pub())
  } finally {
    const rr = await api('PUT', '/admin/settings', { key: 'wallet_config', value: orig }); log('restore', rr.status_code, JSON.stringify((await api('GET', '/admin/settings?key=wallet_config')).data)); log('public restored', await pub())
  }
}
