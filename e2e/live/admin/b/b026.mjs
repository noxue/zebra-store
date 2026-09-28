import fs from 'node:fs'
export default async ({ page, admin, shot, log, problems, api }) => {
  const bk = JSON.parse(fs.readFileSync(fs.readdirSync('live/admin/out').filter(f => f.startsWith('b-backup-')).sort().map(f => 'live/admin/out/' + f)[0], 'utf8'))
  const orig = bk['/admin/settings/smtp'].data
  const F = (label) => page.locator(`xpath=//label[normalize-space(translate(.,"*",""))="${label}"]/following-sibling::div[1]//input`).first()
  try {
    await page.goto(`${admin}/settings?tab=smtp`); await page.waitForLoadState('networkidle')
    const labels = await page.locator('main label').allInnerTexts(); log('labels', labels.join(' / '))
    // validation: port 0 / invalid from
    const n0 = problems.length
    await page.locator('main input[type=number]').first().fill('0')
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    await shot(page, 'b-026-invalid-port', false)
    log('invalid port', JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings/smtp')).data.port))
    await page.goto(`${admin}/settings?tab=smtp`); await page.waitForLoadState('networkidle')
    const inputs = page.locator('main input:not([type=number]):not([type=checkbox])')
    log('placeholders', JSON.stringify(await inputs.evaluateAll(e => e.map(x => x.placeholder + '|' + x.type))))
    await page.getByPlaceholder(/smtp\./i).first().fill('smtp.qa-invalid.test')
    await page.locator('main input[type=password]').first().fill('QaSecret-123')
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    let s = (await api('GET', '/admin/settings/smtp')).data
    log('after save', JSON.stringify({ host: s.host, password: s.password, has: s.has_password }))
    await page.reload(); await page.waitForLoadState('networkidle')
    log('password input after reload', await page.locator('main input[type=password]').first().inputValue())
    await shot(page, 'b-026-masked')
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    s = (await api('GET', '/admin/settings/smtp')).data; log('after resave has_password', s.has_password)
    // generic endpoint leak check
    for (const k of ['smtp_config']) { const r = await api('GET', '/admin/settings?key=' + k); log('generic GET', k, JSON.stringify(r.data).slice(0, 200)) }
    // test send
    const tn = problems.length
    await page.getByPlaceholder(/@/).last().fill('qa-test@lab.test')
    await page.getByRole('button', { name: /发送测试|测试邮件/ }).first().click(); await page.waitForTimeout(8000)
    await shot(page, 'b-026-test-send', false)
    log('test send', JSON.stringify(problems.slice(tn)), await page.locator('[class*=toast], [role=alert], [role=status]').allInnerTexts().catch(() => ''))
  } finally {
    // restore everything except password (can't clear via patch)
    const r = await api('PUT', '/admin/settings/smtp', { ...orig, password: '' })
    const s = (await api('GET', '/admin/settings/smtp')).data
    log('restored', r.status_code, JSON.stringify(s))
  }
}
