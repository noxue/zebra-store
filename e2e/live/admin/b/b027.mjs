import fs from 'node:fs'
export default async ({ page, admin, shot, log, problems, api }) => {
  const orig = (await api('GET', '/admin/settings/order-email-template')).data
  fs.writeFileSync('live/admin/out/b-backup-order-email.json', JSON.stringify(orig))
  log('scenes', Object.keys(orig.templates))
  try {
    await page.goto(`${admin}/settings?tab=order_email_template`); await page.waitForLoadState('networkidle')
    await shot(page, 'b-027-tab')
    const subj = page.locator('main input:visible').first()
    log('subject value', await subj.inputValue())
    await subj.fill('QA主题 {{order_no}}')
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    const now = (await api('GET', '/admin/settings/order-email-template')).data
    log('saved default zh-CN', JSON.stringify(now.templates.default?.['zh-CN']?.subject), 'changed keys', Object.keys(now.templates).filter(k => JSON.stringify(now.templates[k]) !== JSON.stringify(orig.templates[k])))
    // empty subject validation
    await subj.fill(''); await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    await shot(page, 'b-027-empty-subject', false)
    log('empty subject saved as', JSON.stringify((await api('GET', '/admin/settings/order-email-template')).data.templates.default?.['zh-CN']?.subject))
    await page.reload(); await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: /重置|恢复默认/ }).first().click(); await page.waitForTimeout(800)
    await shot(page, 'b-027-reset-confirm', false)
    const confirm = page.locator('[role=dialog] button').filter({ hasText: /确认|确定|重置/ }).last()
    if (await confirm.count()) await confirm.click()
    await page.waitForTimeout(1500)
    const after = (await api('GET', '/admin/settings/order-email-template')).data
    log('after reset subject', JSON.stringify(after.templates.default?.['zh-CN']?.subject))
  } finally {
    const r = await api('PUT', '/admin/settings/order-email-template', orig)
    log('restored', r.status_code, JSON.stringify((await api('GET', '/admin/settings/order-email-template')).data) === JSON.stringify(orig))
  }
}
