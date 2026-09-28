import fs from 'node:fs'
export default async ({ page, shot, log, admin }) => {
  const reqs = []
  page.on('response', async (r) => { if (/\/admin\/payments/.test(r.url())) { let s = ''; try { const j = await r.json(); s = (j.pagination?.total ?? '') + ' ' + JSON.stringify(j.data).slice(0, 120) } catch { s = 'non-json ' + r.headers()['content-type'] } reqs.push(r.request().method() + ' ' + r.url().replace(/^.*\/api\/v1/, '') + ' => ' + s) } })
  await page.goto(admin + '/payments'); await page.waitForLoadState('networkidle')
  await shot(page, 'd-payments')
  const phs = await page.locator('main input[placeholder]').evaluateAll(els => els.map(e => e.placeholder)); log('placeholders', phs.join(' / '))
  const sels = page.locator('main select'); for (let i = 0; i < await sels.count(); i++) log('select', i, (await sels.nth(i).locator('option').allInnerTexts()).join('/'))
  await page.getByPlaceholder(phs[0]).fill('8'); await page.waitForTimeout(1300); log('user 8 =>', reqs.at(-1)?.slice(0, 250))
  await sels.nth(0).selectOption({ index: 1 }); await page.waitForTimeout(1300); log('status idx1 =>', reqs.at(-1)?.slice(0, 250))
  await sels.nth(0).selectOption({ index: 0 }); await page.waitForTimeout(1000)
  // detail
  await page.locator('tbody tr').first().getByRole('button', { name: /详情|查看/ }).first().click(); await page.waitForTimeout(1000)
  const dlg = page.locator('[role=dialog]').last(); log('detail', (await dlg.innerText().catch(() => 'no dialog')).replace(/\n+/g, ' | ').slice(0, 600)); await shot(page, 'd-payment-detail', false)
  await page.keyboard.press('Escape'); await page.waitForTimeout(500)
  // export
  const dl = page.waitForEvent('download', { timeout: 15000 }).catch(e => null)
  await page.getByRole('button', { name: /导出/ }).click()
  const d = await dl; await page.waitForTimeout(1000)
  log('export req', reqs.filter(r => /export/.test(r)).join(' || ').slice(0, 300))
  if (d) { const p = 'live/admin/out/d-payments-export-' + d.suggestedFilename(); await d.saveAs(p); const c = fs.readFileSync(p, 'utf8'); log('export file', d.suggestedFilename(), 'lines', c.split('\n').length, '\n' + c.split('\n').slice(0, 5).join('\n')) } else log('NO DOWNLOAD', (await page.locator('main').innerText()).match(/导出[^\n]*/g)?.join('/'))
}
