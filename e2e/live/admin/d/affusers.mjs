export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/affiliates/users'); await page.waitForLoadState('networkidle')
  const phs = await page.locator('main input[placeholder]').evaluateAll(els => els.map(e => e.placeholder)); log('placeholders', phs.join(' / '))
  const row = () => page.locator('tbody tr').filter({ hasText: 'qa-user-d1790358804' }).first()
  const act = async (name) => { const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PATCH', { timeout: 6000 }).catch(() => null), (async () => { await row().getByRole('button', { name }).click(); await page.waitForTimeout(400); const c = page.locator('[role=dialog] button', { hasText: /^确认$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click() })()]); await page.waitForTimeout(900); log(name, r ? (await r.text()).slice(0, 120) : 'NO REQUEST', '| row:', (await row().innerText()).replace(/\s+/g, ' ').slice(-30)) }
  await act('停用'); await shot(page, 'd-aff-user-disabled')
  await act('启用')
  await row().locator('[role=checkbox]').click(); await page.waitForTimeout(300)
  const bb = page.getByRole('button', { name: /批量停用/ }); log('batch btn', await bb.count())
  if (await bb.count()) { const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PATCH', { timeout: 6000 }).catch(() => null), (async () => { await bb.click(); await page.waitForTimeout(400); const c = page.locator('[role=dialog] button', { hasText: /^确认$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click() })()]); await page.waitForTimeout(900); log('batch disable', r ? (await r.text()).slice(0, 120) : 'none') }
  await row().locator('[role=checkbox]').click().catch(() => {}); await page.waitForTimeout(300)
  const be = page.getByRole('button', { name: /批量启用/ }); if (await be.count()) { await be.click(); await page.waitForTimeout(400); const c = page.locator('[role=dialog] button', { hasText: /^确认$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click(); await page.waitForTimeout(900) }
  log('final', (await row().innerText()).replace(/\s+/g, ' ').slice(-30))
  await page.goto(admin + '/affiliates/commissions'); await page.waitForLoadState('networkidle'); const sels = page.locator('main select'); for (let i = 0; i < await sels.count(); i++) log('comm select', i, (await sels.nth(i).locator('option').allInnerTexts()).join('/'))
  await page.goto(admin + '/affiliates/withdraws'); await page.waitForLoadState('networkidle'); const s2 = page.locator('main select'); for (let i = 0; i < await s2.count(); i++) log('wd select', i, (await s2.nth(i).locator('option').allInnerTexts()).join('/'))
}
