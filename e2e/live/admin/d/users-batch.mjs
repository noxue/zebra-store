import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, api, shot, log, admin, base }) => {
  const reqs = []
  page.on('request', (r) => { if (r.url().includes('/api/v1/admin/users')) reqs.push(r.method() + ' ' + r.url().replace(/^.*\/api\/v1/, '')) })
  await page.goto(admin + '/users'); await page.waitForLoadState('networkidle')
  const all = await api('GET', '/admin/users?page=1&page_size=50'); log('users total', all.pagination?.total, all.data.map(u => u.id + ':' + u.email).join(' '))
  // batch disable my user only
  await page.getByPlaceholder('用户ID').fill(String(U.id)); await page.waitForTimeout(1200)
  await page.locator('tbody tr [role=checkbox]').first().click()
  await page.waitForTimeout(300); await shot(page, 'd-users-batch-bar', false)
  await page.getByRole('button', { name: '批量禁用' }).click(); await page.waitForTimeout(800)
  await shot(page, 'd-users-batch-disable-confirm', false)
  const confirm = page.locator('[role=dialog] button', { hasText: /确认|确定/ }).last()
  if (await confirm.count()) await confirm.click()
  await page.waitForTimeout(1200)
  let u = await api('GET', '/admin/users/' + U.id); log('status after disable', u.data?.user?.status ?? u.data?.status)
  // user login should fail
  const lr = await page.evaluate(async (U) => (await (await fetch('/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: U.email, password: U.password }) })).json()), U)
  log('login while disabled', JSON.stringify(lr).slice(0, 200))
  await page.locator('tbody tr [role=checkbox]').first().click().catch(() => {})
  await page.getByRole('button', { name: '批量启用' }).click(); await page.waitForTimeout(800)
  const c2 = page.locator('[role=dialog] button', { hasText: /确认|确定/ }).last(); if (await c2.count()) await c2.click()
  await page.waitForTimeout(1200)
  u = await api('GET', '/admin/users/' + U.id); log('status after enable', u.data?.user?.status ?? u.data?.status)
}
