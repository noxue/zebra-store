export default async ({ page, shot, log, admin }) => {
  const reqs = []
  page.on('response', async (r) => { if (/\/admin\/order-refunds/.test(r.url())) { try { const j = await r.json(); reqs.push(r.request().method() + ' ' + r.url().replace(/^.*\/api\/v1/, '') + ' => ' + (j.pagination?.total ?? '') + ' ' + JSON.stringify(j.data).slice(0, 160)) } catch {} } })
  await page.goto(admin + '/order-refunds'); await page.waitForLoadState('networkidle')
  const f = async (ph, v) => { const i = page.locator(`main input[placeholder="${ph}"]`); if (!await i.count()) { log('no input', ph); return } await i.fill(v); await page.waitForTimeout(1300); log(ph, v, '=>', reqs.at(-1)?.slice(0, 160)); await i.fill(''); await page.waitForTimeout(900) }
  const phs = await page.locator('main input[placeholder]').evaluateAll(els => els.map(e => e.placeholder)); log('placeholders', phs.join(' / '))
  await f(phs[0], '8'); await f(phs[1], 'qa-user-d'); await f(phs[2], 'DJ20260926020847959019'); await f(phs[4], 'ChatGPT')
  // manual record detail -> toggle payment fee
  const row = page.locator('tbody tr', { hasText: '手动退款' }).first()
  await row.getByRole('button', { name: '查看' }).click(); await page.waitForTimeout(800)
  const dlg = page.locator('[role=dialog]').last(); await shot(page, 'd-refund-detail', false)
  log('detail', (await dlg.innerText()).replace(/\n+/g, ' | ').slice(0, 500))
  const cb = dlg.locator('[role=checkbox]').first()
  if (await cb.count()) { await cb.click(); const [r] = await Promise.all([page.waitForResponse(x => x.request().method() !== 'GET' && /order-refunds/.test(x.url()), { timeout: 6000 }).catch(() => null), dlg.getByRole('button', { name: /更新|保存/ }).first().click()]); await page.waitForTimeout(800); log('toggle fee', r ? (await r.text()).slice(0, 200) : 'NO REQUEST') }
  await page.keyboard.press('Escape'); await page.waitForTimeout(500)
  // wallet refund record: toggle available?
  await page.locator('tbody tr', { hasText: '钱包退款' }).first().getByRole('button', { name: '查看' }).click(); await page.waitForTimeout(800)
  log('wallet refund detail has checkbox', await page.locator('[role=dialog]').last().locator('[role=checkbox]').count())
}
