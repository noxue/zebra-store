export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  const orig = (await api('GET', '/admin/settings/captcha')).data
  require_ok: try {
    await page.goto(`${admin}/settings?tab=captcha`); await page.waitForLoadState('networkidle')
    await shot(page, 'b-028-tab')
    // turnstile without keys -> validation?
    const sel = page.locator('main select').first()
    log('provider options', await sel.locator('option').allTextContents().catch(() => []))
    await sel.selectOption({ label: 'Cloudflare Turnstile' }).catch(e => log('sel', e.message.slice(0, 60)))
    await page.waitForTimeout(300)
    const n0 = problems.length
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    await shot(page, 'b-028-turnstile-nokey', false)
    log('turnstile no key', JSON.stringify(problems.slice(n0)), (await api('GET', '/admin/settings/captcha')).data.provider)
    await api('PUT', '/admin/settings/captcha', orig)
    await page.reload(); await page.waitForLoadState('networkidle')
    await page.locator('main select').first().selectOption({ label: '图片验证码（base64Captcha）' })
    await page.waitForTimeout(300)
    // enable login scene: find switch near text 登录
    const sw = page.locator('main [role=switch]')
    log('switch count', await sw.count(), await page.locator('main').innerText().then(t => t.slice(0, 600).replace(/\n/g, ' / ')))
    await page.getByText(/^(用户)?登录$/).first().click().catch(async () => { await sw.first().click() })
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    const now = (await api('GET', '/admin/settings/captcha')).data
    log('saved', now.provider, JSON.stringify(now.scenes))
    const sp = await b.newPage({ viewport: { width: 1440, height: 900 } })
    await sp.goto(base + '/auth/login'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
    log('storefront captcha img', await sp.locator('img[src^="data:image"], img[alt*=aptcha], img[src*=captcha]').count())
    await shot(sp, 'b-028-storefront-login', false)
  } finally {
    const r = await api('PUT', '/admin/settings/captcha', orig)
    const now = (await api('GET', '/admin/settings/captcha')).data
    log('restored', r.status_code, now.provider, JSON.stringify(now.scenes))
  }
}
