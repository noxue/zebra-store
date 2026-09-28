export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/member-levels'); await page.waitForLoadState('networkidle')
  const row = page.locator('tbody tr', { hasText: 'qa-vip-d' }).first()
  log('row', (await row.innerText()).replace(/\s+/g, ' '))
  await shot(page, 'd-levels-with-qa')
  // edit: set default then check
  await row.getByRole('button', { name: /编辑/ }).click(); await page.waitForTimeout(700)
  const dlg = page.locator('[role=dialog]').last()
  await dlg.locator('input[type=number]').nth(0).fill('120');
  const [r0] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 5000 }).catch(() => null), dlg.getByRole('button', { name: /保存/ }).click()])
  await page.waitForTimeout(700); log('discount 120', r0 ? (await r0.text()).slice(0, 160) : 'NO REQUEST', (await dlg.innerText().catch(() => '')).match(/请[^\n]*|不能[^\n]*|范围[^\n]*/g)?.join('/'), (await page.locator('[role=alert],[role=status]').allInnerTexts().catch(() => [])).join('|').slice(0, 120))
  await shot(page, 'd-level-discount120', false)
  await page.keyboard.press('Escape'); await page.waitForTimeout(400)
  // delete while assigned to user 8
  const del = async (label) => { const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'DELETE', { timeout: 6000 }).catch(() => null), (async () => { await page.locator('tbody tr', { hasText: 'qa-vip-d' }).first().getByRole('button', { name: /删除/ }).click(); await page.waitForTimeout(400); const c = page.locator('[role=dialog] button', { hasText: /^确认$|^删除$|^确定$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click() })()]); await page.waitForTimeout(900); log(label, r ? (await r.text()).slice(0, 160) : 'NO REQUEST', (await page.locator('[role=alert],[role=status]').allInnerTexts().catch(() => [])).join('|').slice(0, 120)) }
  await del('delete while in use')
  const u = (await api('GET', '/admin/users/8')).data; log('user level after', u.member_level_id)
  const lv = (await api('GET', '/admin/member-levels?page=1&page_size=50')).data; log('levels now', JSON.stringify(lv.map(l => [l.id, l.slug])))
  if (lv.find(l => l.slug === 'qa-vip-d')) { log('unassign', (await api('PUT', '/admin/users/8/member-level', { member_level_id: 0 })).status_code); await page.reload(); await page.waitForLoadState('networkidle'); await del('delete after unassign') }
  const tokPrice = await page.evaluate(async () => { const t = (await (await fetch('/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'qa-user-d1790358804@lab.test', password: 'NewPass12345!' }) })).json()).data.token; return (await (await fetch('/api/v1/orders/preview', { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + t }, body: JSON.stringify({ items: [{ product_id: 8, sku_id: 12, quantity: 1 }] }) })).json()).data.total_amount })
  log('price after level deleted', tokPrice, 'user level', (await api('GET', '/admin/users/8')).data.member_level_id)
}
