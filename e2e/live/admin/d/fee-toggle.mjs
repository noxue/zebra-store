export default async ({ page, shot, log, admin }) => {
  await page.goto(admin + '/order-refunds'); await page.waitForLoadState('networkidle')
  await page.locator('tbody tr', { hasText: '手动退款' }).first().getByRole('button', { name: '查看' }).click(); await page.waitForTimeout(800)
  const dlg = page.locator('[role=dialog]').last(); const cb = dlg.locator('[role=checkbox]').first()
  log('before', await cb.getAttribute('aria-checked')); await cb.click(); await page.waitForTimeout(300); log('after click', await cb.getAttribute('aria-checked'))
  const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PATCH', { timeout: 6000 }).catch(() => null), dlg.getByRole('button', { name: /保存手续费状态/ }).click()])
  log('body', r?.request().postData(), (await r?.text())?.slice(0, 140)); await page.waitForTimeout(800)
  log('after save', await cb.getAttribute('aria-checked')); await shot(page, 'd-refund-fee-toggle', false)
  await page.keyboard.press('Escape'); await page.waitForTimeout(400); await page.reload(); await page.waitForLoadState('networkidle')
  log('row', (await page.locator('tbody tr', { hasText: '手动退款' }).first().innerText()).replace(/\s+/g, ' '))
}
