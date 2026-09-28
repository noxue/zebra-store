import { selectOption } from './common.mjs'
export default async ({ page, shot, log, admin }) => {
  await page.goto(`${admin}/products`)
  await page.waitForLoadState('networkidle')
  const rows = page.locator('tbody tr')
  const names = async () => (await rows.allInnerTexts()).map((x) => x.replace(/\s+/g, ' ').match(/#\d+/)?.[0]).join(',')
  log('all', await rows.count(), await names())
  for (const re of [/^Low stock$/, /^In stock$/, /^Unlimited stock$/, /^Unlisted$/, /^With wholesale pricing$/, /^qa-cat 0926$/]) {
    const s = page.locator('select').filter({ has: page.locator('option', { hasText: re }) }).first()
    const reqP = page.waitForRequest((r) => r.url().includes('/admin/products?'), { timeout: 5000 }).catch(() => null)
    await selectOption(s, re)
    const req = await reqP
    await page.waitForTimeout(1200)
    log('filter', String(re), 'req', req ? new URL(req.url()).search : 'none', 'rows', await rows.count(), await names())
    await page.getByRole('button', { name: 'Reset' }).click(); await page.waitForTimeout(1000)
  }
  log('footer', (await page.locator('main').innerText()).split('\n').slice(-6).join(' | '))
  await shot(page, 'c-prod-list-full')
}
