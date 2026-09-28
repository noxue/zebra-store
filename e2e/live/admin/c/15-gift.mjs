import fs from 'node:fs'
import { dialog, field, selectOption, apiResponse, TS } from './common.mjs'
export default async ({ page, api, shot, log, admin }) => {
  const name = `qa-gift ${TS}`
  await page.goto(`${admin}/gift-cards`)
  await page.waitForLoadState('networkidle')
  await page.getByRole('button', { name: 'Generate Gift Cards' }).click()
  const dlg = dialog(page, 'Generate Gift Cards')
  await field(dlg, 'Name').fill(name)
  await field(dlg, 'Quantity').fill('20000')
  await field(dlg, 'Amount').fill('50')
  await dlg.getByRole('button', { name: 'Confirm', exact: true }).click()
  await page.waitForTimeout(800)
  log('qty 20000 msg:', (await page.locator('[role=status]').allInnerTexts()).join(' | '), (await dlg.locator('.text-danger-text').allInnerTexts()).join('/'))
  await field(dlg, 'Quantity').fill('10')
  await dlg.locator('input[type="datetime-local"]').fill('2027-01-01T00:00')
  const r = await apiResponse(page, 'POST', /\/admin\/gift-cards\/generate$/, () => dlg.getByRole('button', { name: 'Confirm', exact: true }).click())
  log('generated', JSON.stringify(r).slice(0, 300))
  await page.waitForTimeout(1000)
  const cards = (await api('GET', '/admin/gift-cards?page=1&page_size=50')).data.filter((c) => c.name === name)
  log('mine', cards.length, JSON.stringify(cards.slice(0, 2)).slice(0, 400))
  // filter by batch? use code filter on first card
  await page.getByPlaceholder('Card code').fill(cards[0].code)
  await page.getByRole('button', { name: 'Search', exact: true }).click()
  await page.waitForTimeout(1200)
  log('rows after code filter', await page.locator('tbody tr').count())
  await page.getByPlaceholder('Card code').fill('')
  await page.getByRole('button', { name: 'Search', exact: true }).click()
  await page.waitForTimeout(1200)
  // select my cards (top rows)
  const rows = page.locator('tbody tr', { hasText: name })
  const n = await rows.count()
  log('my rows visible', n)
  for (let i = 0; i < Math.min(n, 3); i++) await rows.nth(i).getByRole('checkbox').click()
  await shot(page, 'c-gift-selected')
  const st = page.locator('select').filter({ has: page.locator('option', { hasText: 'Disabled' }) }).last()
  await selectOption(st, 'Disabled')
  await page.getByRole('button', { name: 'Apply status' }).click()
  await page.waitForTimeout(600)
  await page.locator('[role=dialog]').last().getByRole('button').last().click()
  await page.waitForTimeout(1500)
  log('disable msg', (await page.locator('[role=status]').allInnerTexts()).join(' | '))
  for (const fmt of ['TXT', 'CSV']) {
    for (let i = 0; i < Math.min(n, 3); i++) if ((await rows.nth(i).getByRole('checkbox').getAttribute('aria-checked')) !== 'true') await rows.nth(i).getByRole('checkbox').click()
    const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 15000 }), page.getByRole('button', { name: `Export ${fmt}` }).click()])
    log('export', fmt, dl.suggestedFilename(), JSON.stringify(fs.readFileSync(await dl.path(), 'utf8').slice(0, 400)))
  }
  const after = (await api('GET', '/admin/gift-cards?page=1&page_size=50')).data.filter((c) => c.name === name)
  const disabled = after.filter((c) => c.status === 'disabled')
  log('statuses', JSON.stringify(after.map((c) => c.status)))
  // redeem disabled card as buyer
  const tok = await page.evaluate(async () => {
    const r = await fetch('/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'buyer@lab.test', password: 'KKYrp7h0Mspnfmp1Zs7k' }) })
    return r.json()
  })
  const ut = tok.data?.token
  log('buyer login', tok.status_code, !!ut)
  if (ut && disabled[0]) {
    const red = await page.evaluate(async ({ ut, code }) => (await (await fetch('/api/v1/gift-cards/redeem', { method: 'POST', headers: { 'content-type': 'application/json', authorization: `Bearer ${ut}` }, body: JSON.stringify({ code }) })).json()), { ut, code: disabled[0].code })
    log('redeem disabled', JSON.stringify(red).slice(0, 300))
  }
  await shot(page, 'c-gift-list')
  fs.writeFileSync('live/admin/out/c-gift-ids.json', JSON.stringify(after.map((c) => c.id)))
}
