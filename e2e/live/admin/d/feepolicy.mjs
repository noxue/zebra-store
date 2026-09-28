import fs from 'node:fs'
const C = JSON.parse(fs.readFileSync('live/admin/d/channel.json'))
export default async ({ page, shot, log, admin, api }) => {
  const orig = (await api('GET', '/admin/settings?key=payment_config')).data; fs.writeFileSync('live/admin/out/d-backup-payment-config.json', JSON.stringify(orig)); log('backup', JSON.stringify(orig))
  try {
    await page.goto(admin + '/payment-channels'); await page.waitForLoadState('networkidle')
    await page.locator('main [role=switch]').first().click()
    const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), page.getByRole('button', { name: /Save changes/ }).click()])
    await page.waitForTimeout(700); log('save', r?.request().postData(), (await r?.text())?.slice(0, 200))
    await page.reload(); await page.waitForLoadState('networkidle'); log('persisted switch', await page.locator('main [role=switch]').first().getAttribute('aria-checked'))
    // scope: payment roles on test channel via API-ish UI
    await page.locator('tbody tr', { hasText: 'QA-D' }).first().getByRole('button', { name: /Edit/ }).click(); await page.waitForTimeout(1000)
    const dlg = page.locator('[role=dialog]').last()
    const ms = dlg.locator('xpath=.//label[contains(., "Payment Roles Limit")]/following-sibling::div[1]').first()
    await ms.click(); await page.waitForTimeout(400); await shot(page, 'd-channel-roles-open', false)
    const opt = page.getByRole('option').or(page.locator('[role=listbox] *')).filter({ hasText: /Member|member/ }).first()
    if (await opt.count()) { await opt.click(); } else log('no member option; options:', (await page.locator('[role=option]').allInnerTexts()).join('/'))
    await page.keyboard.press('Escape').catch(() => {}); await page.waitForTimeout(300)
    const dlg2 = page.locator('[role=dialog]').last()
    const [r2] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT' && /payment-channels/.test(x.url()), { timeout: 6000 }).catch(() => null), dlg2.getByRole('button', { name: 'Save', exact: true }).click().catch(e => log('save fail', e.message.slice(0, 60)))])
    await page.waitForTimeout(700); log('roles save', r2 ? JSON.parse(r2.request().postData()).payment_roles + ' => ' + (await r2.text()).slice(0, 120) : 'NO REQUEST')
    log('channel roles', JSON.stringify((await api('GET', '/admin/payment-channels/' + C.id)).data.payment_roles), (await api('GET', '/admin/payment-channels/' + C.id)).data.is_active)
  } finally {
    const rr = await api('PUT', '/admin/settings', { key: 'payment_config', value: orig }); log('restore', rr.status_code, JSON.stringify((await api('GET', '/admin/settings?key=payment_config')).data))
  }
}
