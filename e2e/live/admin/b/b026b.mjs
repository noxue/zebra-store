export default async ({ page, admin, shot, log, problems }) => {
  await page.goto(`${admin}/settings?tab=smtp`); await page.waitForLoadState('networkidle')
  const tn = problems.length
  await page.getByRole('button', { name: /测试/ }).last().click(); await page.waitForTimeout(1500)
  log('empty email', JSON.stringify(problems.slice(tn)), await page.locator('body').innerText().then(t => t.match(/请输入[^\n]{0,20}|邮箱[^\n]{0,20}无效/g)))
  await shot(page, 'b-026-test-empty', false)
  await page.getByPlaceholder('输入收件邮箱').fill('qa-test@lab.test')
  const t0 = Date.now()
  await page.getByRole('button', { name: /测试/ }).last().click(); await page.waitForTimeout(6000)
  log('send', Date.now() - t0, JSON.stringify(problems.slice(tn)))
  await shot(page, 'b-026-test-send', false)
}
