export default async ({ page, log, admin, api, shot }) => {
  await page.goto(admin + '/payment-channels'); await page.waitForLoadState('networkidle')
  const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'DELETE', { timeout: 6000 }).catch(() => null), (async () => { await page.locator('tbody tr', { hasText: 'QA-D' }).first().getByRole('button', { name: /删除/ }).click(); await page.waitForTimeout(500); await shot(page, 'd-channel-delete-confirm', false); const c = page.locator('[role=dialog] button', { hasText: /^确认$|^删除$|^确定$/ }).last(); if (await c.isVisible().catch(() => false)) await c.click() })()])
  await page.waitForTimeout(800); log('delete', r ? (await r.text()).slice(0, 150) : 'NO REQUEST')
  log('channels', JSON.stringify((await api('GET', '/admin/payment-channels?page=1&page_size=50')).data.map(c => [c.id, c.name])))
}
