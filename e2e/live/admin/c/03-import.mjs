import { selectOption, apiResponse, field, TS, toast } from './common.mjs'
export default async ({ page, api, shot, log, admin }) => {
  const P = `qa-product ${TS}`
  await page.goto(`${admin}/card-secret-imports`)
  await page.waitForLoadState('networkidle')
  await shot(page, 'c-import-empty')
  const productSelect = page.locator('select').filter({ has: page.locator('option', { hasText: P }) })
  await selectOption(productSelect, new RegExp(P))
  const skuSelect = page.locator('select').filter({ has: page.locator('option', { hasText: `QA-STD-${TS}` }) })
  log('sku options', (await skuSelect.locator('option').allTextContents()).join(' / '))
  await selectOption(skuSelect, new RegExp(`QA-STD-${TS}`))
  const statsBefore = (await api('GET', `/admin/card-secrets/stats?product_id=13`)).data
  // paste 10 lines incl. a duplicate + blank line + whitespace
  const lines = Array.from({ length: 9 }, (_, i) => `QA-${TS}-P${i + 1}`)
  const pasted = [...lines, `  QA-${TS}-P3  `, ''].join('\n')
  const dedupe = page.getByRole('switch').first()
  log('dedupe default on?', await dedupe.getAttribute('aria-checked'))
  if ((await dedupe.getAttribute('aria-checked')) !== 'true') await dedupe.click()
  await field(page, 'Secrets', 'textarea').fill(pasted)
  const r = await apiResponse(page, 'POST', /\/admin\/card-secrets\/batch$/, () => page.getByRole('button', { name: 'Submit batch' }).click())
  log('batch result', JSON.stringify(r.data ?? r).slice(0, 400), 'toast', await toast(page))
  await shot(page, 'c-import-paste')
  // paste same again -> all duplicates
  await field(page, 'Secrets', 'textarea').fill(lines.slice(0, 2).join('\n'))
  await page.getByRole('button', { name: 'Submit batch' }).click()
  await page.waitForTimeout(1500)
  log('re-paste dup toast:', await toast(page))
  // CSV import on PRO sku
  await selectOption(skuSelect, new RegExp(`QA-PRO-${TS}`))
  const csv = ['secret', ...Array.from({ length: 5 }, (_, i) => `QA-${TS}-CSV${i + 1}`)].join('\n')
  await page.locator('input[type="file"][accept=".csv"]').setInputFiles({ name: 'cards.csv', mimeType: 'text/csv', buffer: Buffer.from(csv) })
  const r2 = await apiResponse(page, 'POST', /\/admin\/card-secrets\/import$/, () => page.getByRole('button', { name: 'Start import' }).click())
  log('csv result', JSON.stringify(r2.data ?? r2).slice(0, 400), 'toast', await toast(page))
  await shot(page, 'c-import-csv')
  // TXT import through API (UI accepts .csv only)
  const txtRes = await page.evaluate(async ({ TS }) => {
    const fd = new FormData()
    fd.append('file', new Blob([`QA-${TS}-TXT1\nQA-${TS}-TXT2\n`], { type: 'text/plain' }), 'cards.txt')
    fd.append('product_id', '13'); fd.append('sku_id', '18')
    const r = await fetch('/api/v1/admin/card-secrets/import', { method: 'POST', headers: { authorization: `Bearer ${localStorage.getItem('admin_token')}` }, body: fd })
    return r.text()
  }, { TS })
  log('txt via API', txtRes.slice(0, 300))
  // bad file: set a .txt into csv input via UI (bypass accept)
  await page.locator('input[type="file"][accept=".csv"]').setInputFiles({ name: 'evil.php', mimeType: 'application/x-php', buffer: Buffer.from('<?php echo 1;') })
  await page.getByRole('button', { name: 'Start import' }).click()
  await page.waitForTimeout(1500)
  log('php upload toast:', await toast(page))
  const statsAfter = (await api('GET', `/admin/card-secrets/stats?product_id=13`)).data
  log('stats before', JSON.stringify(statsBefore), 'after', JSON.stringify(statsAfter))
  const batches = await api('GET', `/admin/card-secrets/batches?product_id=13`)
  log('batches', JSON.stringify(batches.data).slice(0, 700))
}
