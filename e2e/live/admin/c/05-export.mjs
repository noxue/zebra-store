import fs from 'node:fs'
import { selectOption, TS } from './common.mjs'
export default async ({ page, api, shot, log, admin }) => {
  const P = `qa-product ${TS}`
  await page.goto(`${admin}/card-secret-exports`)
  await page.waitForLoadState('networkidle')
  await selectOption(page.locator('select').filter({ has: page.locator('option', { hasText: P }) }), new RegExp(P))
  await page.waitForTimeout(1200)
  await selectOption(page.locator('select').filter({ has: page.locator('option', { hasText: `QA-STD-${TS}` }) }).first(), new RegExp(`QA-STD-${TS}`))
  await page.waitForTimeout(1200)
  const before = (await api('GET', '/admin/card-secrets/stats?product_id=13&sku_id=17')).data
  const qty = page.locator('input[type=number]').first()
  // over-available quantity validation
  await qty.fill('999')
  log('submit disabled at 999?', await page.getByRole('button', { name: 'Export Secrets' }).isDisabled())
  await qty.fill('0')
  log('submit disabled at 0?', await page.getByRole('button', { name: 'Export Secrets' }).isDisabled())
  await qty.fill('5')
  await page.getByRole('checkbox').first().click()
  await shot(page, 'c-export-form')
  await page.getByRole('button', { name: 'Export Secrets' }).click()
  await page.waitForTimeout(700)
  const dlg = page.locator('[role=dialog]').last()
  log('confirm', (await dlg.innerText()).replace(/\s+/g, ' ').slice(0, 300))
  const dlP = page.waitForEvent('download', { timeout: 8000 }).catch(() => null)
  await dlg.getByRole('button', { name: 'Confirm' }).click()
  await page.waitForTimeout(2500)
  const dl = await dlP
  if (dl) log('auto download', dl.suggestedFilename(), JSON.stringify(fs.readFileSync(await dl.path(), 'utf8')))
  await shot(page, 'c-export-result')
  const res = page.locator('[role=dialog]').last()
  log('result dialog', (await res.innerText().catch(() => '')).replace(/\s+/g, ' ').slice(0, 500))
  const btn = res.getByRole('button', { name: 'Download' })
  if (await btn.count()) {
    const [d2] = await Promise.all([page.waitForEvent('download', { timeout: 8000 }), btn.click()])
    log('manual download', d2.suggestedFilename(), JSON.stringify(fs.readFileSync(await d2.path(), 'utf8')))
  }
  const after = (await api('GET', '/admin/card-secrets/stats?product_id=13&sku_id=17')).data
  log('stats before', JSON.stringify(before), 'after', JSON.stringify(after))
}
