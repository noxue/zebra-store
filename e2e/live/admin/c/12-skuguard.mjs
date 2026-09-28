import { dialog, TS } from './common.mjs'
export default async ({ page, shot, log, admin }) => {
  await page.goto(`${admin}/products`)
  await page.waitForLoadState('networkidle')
  await page.locator('tbody tr', { hasText: `qa-product-${TS}` }).getByRole('button', { name: 'Edit' }).click()
  const dlg = dialog(page, 'Edit product')
  await page.waitForTimeout(1500)
  const sku = dlg.locator('div.space-y-3.rounded-zs-sm', { hasText: 'SKU code' }).filter({ has: page.locator(`input[value="QA-STD-${TS}"]`) })
  log('sku blocks', await sku.count())
  await sku.getByRole('switch').first().click()
  await dlg.getByRole('button', { name: 'Save changes' }).click()
  await page.waitForTimeout(1800)
  log('msg:', (await page.locator('[role=status]').allInnerTexts()).join(' | '), '| dlg err:', (await dlg.locator('.text-danger-text').allInnerTexts()).join('/'))
  await shot(page, 'c-sku-disable-guard')
  // mobile check of product list + edit dialog later
}
