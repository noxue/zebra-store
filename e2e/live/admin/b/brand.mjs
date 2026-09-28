import path from 'node:path'
const D = path.resolve('live/admin/b')
export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  await page.goto(`${admin}/settings?tab=basic`); await page.waitForLoadState('networkidle')
  const nameInput = page.locator('xpath=//label[contains(.,"站点名称")]/following-sibling::div[1]//input').first()
  await nameInput.fill('QA品牌测试站 QA-Brand')
  // SEO title zh-CN
  await page.getByPlaceholder('网站首页标题').fill('QA SEO 标题')
  // favicon
  await page.getByRole('button', { name: '选择/上传' }).click()
  await page.waitForTimeout(800); await shot(page, 'b-020-mediapicker')
  const dlg = page.locator('[role=dialog]').last()
  await dlg.getByText('上传', { exact: false }).first().click().catch(() => {})
  await page.waitForTimeout(500)
  await dlg.locator('input[type=file]').first().setInputFiles(path.join(D, 'qa-favicon.png'))
  await page.waitForTimeout(2500); await shot(page, 'b-020-after-favicon-upload')
  if (await page.locator('[role=dialog]').count()) { log('dialog still open after favicon upload'); await page.keyboard.press('Escape') }
  await page.getByRole('button', { name: '选择 Logo' }).click(); await page.waitForTimeout(800)
  const dlg2 = page.locator('[role=dialog]').last()
  await dlg2.getByText('上传', { exact: false }).first().click().catch(() => {})
  await dlg2.locator('input[type=file]').first().setInputFiles(path.join(D, 'qa-logo.png'))
  await page.waitForTimeout(2500)
  if (await page.locator('[role=dialog]').count()) { log('dialog still open after logo upload'); await page.keyboard.press('Escape') }
  // footer link
  await page.getByRole('button', { name: '新增链接' }).click(); await page.waitForTimeout(300)
  await shot(page, 'b-020-before-save')
  const n0 = problems.length
  await page.getByTestId('settings-save').click(); await page.waitForTimeout(2000)
  await shot(page, 'b-020-after-save', false)
  log('save problems', JSON.stringify(problems.slice(n0)))
  const sc = await api('GET', '/admin/settings?key=site_config'); log('site_config', JSON.stringify(sc.data).slice(0, 1500))
  // storefront
  const sp = await page.context().newPage()
  await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1000)
  const sf = await sp.evaluate(() => ({ title: document.title, icon: [...document.querySelectorAll('link[rel*=icon]')].map(l => l.href), nav: document.querySelector('header')?.innerText.slice(0, 200), imgs: [...document.querySelectorAll('header img')].map(i => i.src), footer: document.querySelector('footer')?.innerText.slice(0, 300), theme: getComputedStyle(document.documentElement).getPropertyValue('--color-primary') }))
  log('storefront', JSON.stringify(sf))
  await shot(sp, 'b-020-storefront', false)
  const lp = await b.newPage(); await lp.goto(admin + '/login'); await lp.waitForLoadState('networkidle'); await lp.waitForTimeout(800)
  log('admin login page', await lp.evaluate(() => ({ title: document.title, text: document.body.innerText.slice(0, 200), imgs: [...document.querySelectorAll('img')].map(i => i.src), icon: [...document.querySelectorAll('link[rel*=icon]')].map(l => l.href) })))
  await shot(lp, 'b-020-admin-login', false)
}
