import path from 'node:path'
import { dialog, field, fieldBox, selectOption, apiResponse, TS, toast } from './common.mjs'
const LOGO = path.resolve('fixtures/logo.png')
export default async ({ page, api, shot, log, admin, base }) => {
  await page.goto(`${admin}/products`)
  await page.waitForLoadState('networkidle')
  await page.getByRole('button', { name: 'Add product' }).click()
  const dlg = dialog(page, 'Add product')
  // empty submit validation
  await dlg.getByRole('button', { name: 'Create now' }).click()
  await page.waitForTimeout(600)
  log('empty submit:', await toast(page), '| errors:', (await dlg.locator('.text-danger-text, [class*=danger]').allInnerTexts()).join(' / ').slice(0, 400))
  await shot(page, 'c-prod-validation')
  const T = `qa-product ${TS}`
  for (const [tab, label] of [['Simplified Chinese', 'Product title (Simplified Chinese)'], ['Traditional Chinese', 'Product title (Traditional Chinese)'], ['English', 'Product title (English)']]) {
    await dlg.getByRole('tab', { name: tab, exact: true }).click()
    await field(dlg, label).fill(T + (tab === 'English' ? '' : ' 中'))
  }
  await field(dlg, 'Slug (URL identifier)').fill(`qa-product-${TS}`)
  await selectOption(field(dlg, 'Category', 'select'), /qa-cat 0926$/)
  await selectOption(field(dlg, 'Purchase type', 'select'), 'Guest purchase')
  await selectOption(field(dlg, 'Fulfillment type', 'select'), 'Auto')
  const skus = [{ code: `QA-STD-${TS}`, spec: 'Standard', price: '10.00', cost: '4.00' }, { code: `QA-PRO-${TS}`, spec: 'Pro', price: '25.00', cost: '9.50' }]
  for (const [i, s] of skus.entries()) {
    await dlg.getByRole('button', { name: 'Add SKU' }).click()
    const item = dlg.locator('div.space-y-3.rounded-zs-sm', { hasText: 'SKU code' }).nth(i)
    await field(item, 'SKU code').fill(s.code)
    await field(item, 'Spec label (English)').fill(s.spec)
    await field(item, 'SKU price').fill(s.price)
    await field(item, 'Cost Price').fill(s.cost)
  }
  // image upload
  await fieldBox(dlg, 'Product images').getByRole('button').first().click()
  const picker = dialog(page, 'Upload New')
  await picker.getByRole('tab', { name: 'Upload New' }).click()
  await apiResponse(page, 'POST', /\/admin\/upload/, () => picker.locator('input[type="file"]').setInputFiles(LOGO))
  await picker.getByRole('button', { name: 'Confirm' }).click()
  // tags
  const tagBox = fieldBox(dlg, 'Product tags')
  await tagBox.locator('input').fill('qa-tag'); await tagBox.locator('input').press('Enter')
  await tagBox.locator('input').fill('qa-tag'); await tagBox.locator('input').press('Enter')
  log('tags now', (await tagBox.innerText()).replace(/\s+/g, ' '))
  // description
  await dlg.locator('textarea').first().fill('qa description')
  await shot(page, 'c-prod-form')
  const r = await apiResponse(page, 'POST', /\/admin\/products$/, () => dlg.getByRole('button', { name: 'Create now' }).click())
  log('created', JSON.stringify(r.data ?? r).slice(0, 800))
  await page.waitForTimeout(1000)
  await shot(page, 'c-prod-list')
  const pub = await page.evaluate(async (slug) => (await (await fetch(`/api/v1/public/products/${slug}`)).json()), `qa-product-${TS}`)
  log('public detail', JSON.stringify(pub).slice(0, 900))
}
