import { selectOption, TS } from './common.mjs'
const notes = async (page) => { await page.waitForTimeout(1200); return (await page.locator('[role=status]').allInnerTexts()).join(' | ').slice(0, 300) }
export default async ({ page, api, shot, log, admin }) => {
  await page.goto(`${admin}/products`)
  await page.waitForLoadState('networkidle')
  await page.locator('#admin-products-search').fill('qa-')
  await page.locator('#admin-products-search').press('Enter')
  await page.waitForTimeout(1500)
  const rows = page.locator('tbody tr')
  log('rows for qa-', await rows.count(), (await rows.allInnerTexts()).map((s) => s.replace(/\s+/g, ' ').slice(0, 80)).join(' || '))
  // limit to my two rows
  for (const slug of [`qa-product-${TS}`, `qa-manual-${TS}`]) await rows.filter({ hasText: slug }).getByRole('checkbox').click()
  await shot(page, 'c-prod-batchbar')
  await page.getByRole('button', { name: 'Batch Deactivate' }).click()
  log('deactivate:', await notes(page))
  let mine = (await api('GET', '/admin/products?search=qa-&page=1&page_size=50')).data.filter((p) => [13, 14].includes(p.id)).map((p) => [p.id, p.is_active, p.category_id, p.sort_order])
  log('after deactivate', JSON.stringify(mine))
  const pub = await page.evaluate(async () => (await (await fetch('/api/v1/public/products?page=1&page_size=100')).json()))
  log('public contains qa-product?', JSON.stringify(pub.data).includes(`qa-product-${TS}`))
  // selection might be cleared after op
  await page.waitForTimeout(800)
  log('batch bar visible after deactivate?', await page.getByRole('button', { name: 'Batch Activate' }).isVisible())
  if (!(await page.getByRole('button', { name: 'Batch Activate' }).isVisible())) for (const slug of [`qa-product-${TS}`, `qa-manual-${TS}`]) await rows.filter({ hasText: slug }).getByRole('checkbox').click()
  await page.getByRole('button', { name: 'Batch Activate' }).click()
  log('activate:', await notes(page))
  await page.waitForTimeout(800)
  log('batch bar visible after activate?', await page.getByRole('button', { name: 'Batch Activate' }).isVisible())
  if (!(await page.getByRole('button', { name: 'Batch Activate' }).isVisible())) for (const slug of [`qa-product-${TS}`, `qa-manual-${TS}`]) await rows.filter({ hasText: slug }).getByRole('checkbox').click()
  log('checked states', JSON.stringify(await rows.getByRole('checkbox').evaluateAll((els) => els.map((e) => e.getAttribute('aria-checked')))))
  const catSel = page.locator('select').filter({ has: page.locator('option', { hasText: 'Select category' }) }).first()
  log('batch cat options', (await catSel.locator('option').allTextContents()).join(' / '))
  log('disabled opts', JSON.stringify(await catSel.locator('option[disabled]').allTextContents())); await selectOption(catSel, /^Lab$/)
  await page.getByRole('button', { name: 'Move', exact: true }).click()
  log('move:', await notes(page))
  mine = (await api('GET', '/admin/products?search=qa-&page=1&page_size=50')).data.filter((p) => [13, 14].includes(p.id)).map((p) => [p.id, p.is_active, p.category_id, p.sort_order])
  log('after activate+move', JSON.stringify(mine))
  // inline sort
  const r13 = rows.filter({ hasText: `qa-product-${TS}` })
  await r13.locator('span[title="Click to edit"]').nth(1).click()
  await page.locator('#sort-input-13').fill('7')
  await page.locator('#sort-input-13').press('Enter')
  log('sort:', await notes(page))
  // inline category
  await r13.locator('span[title="Click to edit"]').first().click()
  await selectOption(r13.locator('select'), /qa-cat 0926$/)
  log('inline cat:', await notes(page))
  // inline status toggle
  await r13.locator('button[title]').first().click()
  log('toggle status:', await notes(page))
  await r13.locator('button[title]').first().click()
  await page.waitForTimeout(1000)
  mine = (await api('GET', '/admin/products?search=qa-&page=1&page_size=50')).data.filter((p) => [13, 14].includes(p.id)).map((p) => [p.id, p.is_active, p.category_id, p.sort_order])
  log('after inline', JSON.stringify(mine))
  await shot(page, 'c-prod-inline')
  // filters
  for (const [label, re] of [['status inactive', /Inactive/], ['stock low', /Low/]]) {
    await page.locator('#admin-products-search').fill('')
    const s = page.locator('select').filter({ has: page.locator('option', { hasText: re }) }).first()
    await selectOption(s, re); await page.waitForTimeout(1200)
    log('filter', label, await rows.count(), (await rows.allInnerTexts()).map((x) => x.replace(/\s+/g, ' ').slice(0, 40)).join(' || ').slice(0, 300))
    await page.getByRole('button', { name: 'Reset' }).click(); await page.waitForTimeout(800)
  }
  log('pager text', (await page.locator('main').innerText()).match(/(Total|共)[^\n]{0,60}/)?.[0])
}
