import fs from 'node:fs'
export default async ({ page, admin, shot, log, problems, api }) => {
  const orig = (await api('GET', '/admin/settings/notification-center')).data
  fs.writeFileSync('live/admin/out/b-backup-notification-center.json', JSON.stringify(orig))
  try {
    await page.goto(`${admin}/settings/notifications`); await page.waitForLoadState('networkidle')
    // enable email with invalid recipient
    await page.getByText('启用邮件通知').click()
    await page.getByPlaceholder(/每行一个邮箱/).fill('not-an-email\nqa-notify@lab.test')
    let n0 = problems.length
    await page.getByRole('button', { name: /保存更改/ }).click(); await page.waitForTimeout(1500)
    await shot(page, 'b-110-invalid-recipient', false)
    log('invalid recipient', JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings/notification-center')).data.channels.email))
    await page.getByPlaceholder(/每行一个邮箱/).fill('qa-notify@lab.test')
    await page.getByRole('button', { name: /保存更改/ }).click(); await page.waitForTimeout(1500)
    log('valid save', JSON.stringify((await api('GET', '/admin/settings/notification-center')).data.channels.email))
    // feishu secret mask
    // test send email
    n0 = problems.length
    await page.getByPlaceholder(/填写邮箱/).fill('qa-notify@lab.test')
    await page.getByRole('button', { name: /发送测试通知/ }).click(); await page.waitForTimeout(5000)
    await shot(page, 'b-111-test-email', false)
    log('test email', JSON.stringify(problems.slice(n0)))
    // telegram test
    const sels = page.locator('main select')
    const labels = await sels.evaluateAll(s => s.map(x => [...x.options].map(o => o.text).join('|')))
    log('selects', JSON.stringify(labels))
    const idx = labels.findIndex(l => l.includes('Telegram') && l.includes('邮件'))
    if (idx >= 0) { await sels.nth(idx).selectOption({ label: labels[idx].split('|').find(t => t.includes('Telegram')) }) }
    await page.getByPlaceholder(/填写邮箱/).fill('123456789')
    n0 = problems.length
    await page.getByRole('button', { name: /发送测试通知/ }).click(); await page.waitForTimeout(5000)
    log('test telegram', JSON.stringify(problems.slice(n0)))
    // logs
    const logs = await api('GET', '/admin/settings/notification-center/logs?page=1&page_size=10')
    log('logs', logs.pagination ? JSON.stringify(logs.pagination) : '', JSON.stringify(logs.data).slice(0, 900))
    await page.reload(); await page.waitForLoadState('networkidle')
    await page.locator('main').evaluate(m => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(500)
    await shot(page, 'b-132-logs', true)
    // filter by status failed and channel email
    const f1 = await api('GET', '/admin/settings/notification-center/logs?page=1&page_size=10&channel=email&status=failed')
    const f2 = await api('GET', '/admin/settings/notification-center/logs?page=1&page_size=10&channel=telegram')
    const f3 = await api('GET', '/admin/settings/notification-center/logs?page=1&page_size=10&status=success')
    log('filter email+failed', f1.data?.length, 'telegram', f2.data?.length, 'success', f3.data?.length)
  } finally {
    const r = await api('PUT', '/admin/settings/notification-center', orig)
    const now = (await api('GET', '/admin/settings/notification-center')).data
    log('restored', r.status_code, JSON.stringify(now) === JSON.stringify(orig))
  }
}
