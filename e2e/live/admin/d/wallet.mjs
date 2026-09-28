import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, api, shot, log, admin }) => {
  await page.goto(admin + '/users/' + U.id); await page.waitForLoadState('networkidle')
  await page.getByRole('button', { name: '钱包明细' }).or(page.getByRole('tab', { name: '钱包明细' })).first().click()
  await page.waitForTimeout(1000)
  const form = page.locator('form').filter({ hasText: '调整' }).last()
  const msg = async () => (await page.locator('main p.text-sm').allInnerTexts()).filter(Boolean).join(' | ')
  const adj = async (op, amount, remark) => {
    await form.locator('select').selectOption({ label: op })
    await form.locator('input').nth(0).fill(amount)
    await form.locator('input').nth(1).fill(remark)
    const [res] = await Promise.all([page.waitForResponse((r) => r.request().method() !== 'GET' && r.url().includes('/admin/users'), { timeout: 8000 }).catch(() => null), form.locator('button[type=submit]').click()])
    await page.waitForTimeout(1200)
    const body = res ? await res.json().catch(() => null) : null
    log('adjust', op, amount, remark, res ? new URL(res.url()).pathname : 'NO REQUEST', JSON.stringify(body)?.slice(0, 160), 'UI:', await msg())
  }
  await form.locator('button[type=submit]').click(); await page.waitForTimeout(800)
  log('empty submit UI:', await msg()); await shot(page, 'd-wallet-empty-submit')
  await adj('增加余额', '100', 'qa +100')
  await adj('扣减余额', '50', 'qa -50')
  await adj('扣减余额', '999', 'qa over-deduct'); await shot(page, 'd-wallet-overdeduct')
  await adj('增加余额', '-5', 'qa negative')
  await adj('增加余额', '1.234', 'qa 3 decimals')
  await adj('增加余额', 'abc', 'qa letters')
  await adj('增加余额', '10', '')
  const w = await api('GET', `/admin/users/${U.id}/wallet`); log('wallet api', JSON.stringify(w).slice(0, 300))
  await page.waitForTimeout(500); await shot(page, 'd-user-wallet-tab')
  log('tx rows', (await page.locator('main table tbody').last().innerText()).slice(0, 700).replace(/\t/g, ' '))
}
