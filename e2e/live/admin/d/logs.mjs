import fs from 'node:fs'
export default async ({ page, shot, log, admin, api }) => {
  const reqs = []
  page.on('response', async (r) => { if (/\/admin\/(user-login-logs|wallet\/recharges)/.test(r.url())) { try { const j = await r.json(); reqs.push(r.url().replace(/^.*\/api\/v1/, '') + ' => ' + (j.pagination?.total ?? '') + ' ' + JSON.stringify(j.data).slice(0, 200)) } catch {} } })
  await page.goto(admin + '/user-login-logs'); await page.waitForLoadState('networkidle'); await shot(page, 'd-login-logs')
  const phs = await page.locator('main input[placeholder]').evaluateAll(els => els.map(e => e.placeholder)); log('placeholders', phs.join(' / '))
  const sels = page.locator('main select'); for (let i = 0; i < await sels.count(); i++) log('select', i, (await sels.nth(i).locator('option').allInnerTexts()).join('/'))
  const f = async (ph, v) => { await page.getByPlaceholder(ph).first().fill(v); await page.waitForTimeout(1400); log(ph, v, '=>', reqs.at(-1)?.slice(0, 300)); await page.getByPlaceholder(ph).first().fill(''); await page.waitForTimeout(900) }
  await f(phs[0], '8'); if (phs[1]) await f(phs[1], 'qa-user-d'); if (phs[2]) await f(phs[2], '143.246.59.65')
  if (await sels.count() > 1) { await sels.nth(0).selectOption({ index: 2 }); await page.waitForTimeout(1300); log('select0 idx2 =>', reqs.at(-1)?.slice(0, 300)); await sels.nth(1).selectOption({ index: 1 }).catch(() => {}); await page.waitForTimeout(1300); log('select1 idx1 =>', reqs.at(-1)?.slice(0, 300)) }
  await shot(page, 'd-login-logs-filtered')
  await page.goto(admin + '/wallet-recharges'); await page.waitForLoadState('networkidle'); await shot(page, 'd-wallet-recharges')
  log('recharges', reqs.at(-1)?.slice(0, 300))
  const sels2 = page.locator('main select'); for (let i = 0; i < await sels2.count(); i++) log('rselect', i, (await sels2.nth(i).locator('option').allInnerTexts()).join('/'))
  // wallet config
  const orig = await api('GET', '/admin/settings?key=wallet_config'); fs.writeFileSync('live/admin/out/d-backup-wallet.json', JSON.stringify(orig.data)); log('wallet backup', JSON.stringify(orig))
}
