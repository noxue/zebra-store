export default async ({ page, api, shot, log, admin, problems }) => {
  // B-014 system update dialog
  await page.goto(`${admin}/`); await page.waitForLoadState('networkidle')
  problems.length = 0
  const btn = page.locator('header button[title]').filter({ hasNot: page.locator('text=退出') })
  const titles = await btn.evaluateAll((els) => els.map((e) => e.getAttribute('title')))
  log('header buttons', titles)
  const upd = page.locator('header button[title]').nth(titles.findIndex((t) => /更新|update|版本/i.test(t || '')))
  await upd.click(); await page.waitForTimeout(1500)
  log('update dialog text', (await page.locator('[role=dialog]').innerText()).replace(/\s+/g, ' '))
  await shot(page, 'a-system-update-dialog', false)
  log('problems on dialog', JSON.stringify(problems))
  await page.keyboard.press('Escape')

  // B-045 audit logs
  await page.goto(`${admin}/authz-audit-logs`); await page.waitForLoadState('networkidle')
  await shot(page, 'a-audit-logs')
  const rowsText = await page.locator('tbody tr').allInnerTexts()
  log('audit rows', rowsText.length, rowsText.slice(0, 8).map((r) => r.replace(/\s+/g, ' ').slice(0, 200)))
  // filter by action select
  const sel = page.locator('select').filter({ has: page.locator('option', { hasText: 'role_create' }) }).first()
  const opts = await page.locator('select option').allInnerTexts()
  log('select options', opts.join(' | ').slice(0, 1500))
  for (const act of ['role_create', 'policy_grant', 'admin_role_assign', 'admin_roles_update', 'admin_create']) {
    if (!opts.includes(act)) continue
    await page.locator('select').filter({ has: page.locator(`option:text-is("${act}")`) }).first().selectOption({ label: act })
    await page.waitForTimeout(1500)
    const t = await page.locator('tbody tr').allInnerTexts()
    log('filter', act, t.length, t.slice(0, 3).map((r) => r.replace(/\s+/g, ' ').slice(0, 220)))
  }
  await shot(page, 'a-audit-logs-filtered')
  const all = await api('GET', '/admin/authz/audit-logs?page=1&page_size=30')
  log('api audit', JSON.stringify((all.data || []).slice(0, 12).map((x) => [x.action, x.role, x.target_username, x.object, x.method, x.request_id ? 'rid' : 'no-rid'])))
  const byRole = await api('GET', '/admin/authz/audit-logs?role=role:qa-orders-viewer')
  log('api by role full', (byRole.data || []).length, JSON.stringify((byRole.data || []).map((x) => x.action)))
  const byRole2 = await api('GET', '/admin/authz/audit-logs?role=qa-orders-viewer')
  log('api by role short', (byRole2.data || []).length)
  log('pagination', JSON.stringify(all.pagination))
}
