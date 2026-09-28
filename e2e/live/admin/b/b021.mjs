export default async ({ page, admin, base, shot, log, problems, api }) => {
  await page.goto(`${admin}/settings?tab=basic`); await page.waitForLoadState('networkidle')
  // currency
  const cur = page.locator('xpath=//label[contains(.,"全站币种")]/following-sibling::div[1]//select').first()
  log('currency options', await cur.locator('option').allTextContents().catch(() => 'n/a'))
  if (await cur.count()) await cur.selectOption('USD').catch(e => log('sel fail', e.message.slice(0, 100)))
  else { await page.locator('xpath=//label[contains(.,"全站币种")]/following-sibling::div[1]//button').first().click(); await page.waitForTimeout(300); await shot(page, 'b-021-currency-dd', false); await page.getByRole('option', { name: /USD/ }).first().click().catch(e => log('opt fail', e.message.slice(0, 100))) }
  await page.getByPlaceholder('https://telegram.me/...').fill('https://t.me/qa_test')
  await page.getByPlaceholder('seo, 关键词, 多个用逗号分隔').fill('qa,测试')
  await page.getByPlaceholder('简短的网站介绍，利于搜索引擎收录').fill('QA 描述')
  await page.getByRole('button', { name: '新增脚本' }).click(); await page.waitForTimeout(300)
  await page.getByPlaceholder(/脚本名称|名称/).last().fill('qa-script').catch(e => log('name', e.message.slice(0, 80)))
  await page.locator('textarea.font-mono, textarea').last().fill('window.__QA_SCRIPT = "ok"; document.documentElement.setAttribute("data-qa","1")')
  await shot(page, 'b-021-before-save')
  const n0 = problems.length
  await page.getByTestId('settings-save').click(); await page.waitForTimeout(2000)
  log('save problems', JSON.stringify(problems.slice(n0)))
  const sc = await api('GET', '/admin/settings?key=site_config'); log('saved', sc.data.currency, JSON.stringify(sc.data.scripts), JSON.stringify(sc.data.contact), JSON.stringify(sc.data.seo))
  const sp = await page.context().newPage()
  await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
  const sf = await sp.evaluate(() => ({ qa: window.__QA_SCRIPT, attr: document.documentElement.getAttribute('data-qa'), kw: document.querySelector('meta[name=keywords]')?.content, desc: document.querySelector('meta[name=description]')?.content, prices: document.body.innerText.match(/(\$|¥|USD|CNY)\s?[\d.]+|[\d.]+\s?(USD|CNY)/g)?.slice(0, 5), tg: [...document.querySelectorAll('a')].filter(a => a.href.includes('t.me')).map(a => a.href) }))
  log('storefront', JSON.stringify(sf))
  await shot(sp, 'b-021-storefront', false)
}
