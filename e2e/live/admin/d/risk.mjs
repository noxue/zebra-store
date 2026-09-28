import fs from 'node:fs'
export default async ({ page, shot, log, admin, api }) => {
  const orig = await api('GET', '/admin/settings?key=order_risk_control_config'); fs.writeFileSync('live/admin/out/d-backup-risk.json', JSON.stringify(orig.data)); log('backup', JSON.stringify(orig.data).slice(0, 800))
  try {
    await page.goto(admin + '/order-risk-control'); await page.waitForLoadState('networkidle')
    await page.locator('main [role=switch]').first().click(); await page.waitForTimeout(600)
    await shot(page, 'd-risk-enabled')
    log('page text', (await page.locator('main').innerText()).replace(/\n+/g, ' | ').slice(0, 1500))
    const rec = page.getByRole('button', { name: /推荐/ }); log('recommend btn', await rec.count()); if (await rec.count()) { await rec.first().click(); await page.waitForTimeout(500); const c = page.locator('[role=dialog] button', { hasText: /^确认$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click(); await page.waitForTimeout(500) }
    const ta = page.locator('main textarea'); log('textareas', await ta.count()); if (await ta.count()) await ta.first().fill('203.0.113.9\n198.51.100.0/24\nnot-an-ip')
    const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), page.getByRole('button', { name: /保存更改/ }).click()])
    await page.waitForTimeout(800); log('save', r ? r.request().postData()?.slice(0, 900) + ' => ' + (await r.text()).slice(0, 200) : 'NO REQUEST', '| msgs', (await page.locator('main').innerText()).match(/请[^\n]*|无效[^\n]*|不合法[^\n]*/g)?.join('/'))
    await shot(page, 'd-risk-saved')
    if (r) { const t = await r.text(); if (!t.includes('"status_code":0')) { await ta.first().fill('203.0.113.9\n198.51.100.0/24'); const [r2] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), page.getByRole('button', { name: /保存更改/ }).click()]); log('save2', r2 ? (await r2.text()).slice(0, 200) : 'none') } }
    await page.reload(); await page.waitForLoadState('networkidle'); await page.waitForTimeout(600)
    log('persisted', JSON.stringify((await api('GET', '/admin/settings?key=order_risk_control_config')).data).slice(0, 900))
    await shot(page, 'd-risk-reloaded')
  } finally {
    const rr = await api('PUT', '/admin/settings', { key: 'order_risk_control_config', value: orig.data }); log('restore', rr.status_code)
    log('after restore equal', JSON.stringify((await api('GET', '/admin/settings?key=order_risk_control_config')).data) === JSON.stringify(orig.data))
  }
}
