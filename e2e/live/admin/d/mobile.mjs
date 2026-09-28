import { adminContext, browser, shot, log, watch, SITES } from '../lib.mjs'
const b = await browser()
for (const locale of ['zh-CN', 'en-US', 'zh-TW']) {
  const ctx = await adminContext(b, { locale, mobile: locale === 'zh-CN', viewport: locale === 'zh-CN' ? { width: 390, height: 844 } : { width: 1440, height: 900 } })
  const page = await ctx.newPage(); const sink = watch(page, [])
  const tag = locale === 'zh-CN' ? 'm' : locale
  for (const r of ['orders', 'users/8', 'order-refunds', 'payment-channels', 'affiliates/users', 'member-levels']) {
    await page.goto(SITES.sa.admin + '/' + r); await page.waitForLoadState('networkidle'); await page.waitForTimeout(500)
    const info = await page.evaluate(() => ({ hs: document.documentElement.scrollWidth > innerWidth + 2, sw: document.documentElement.scrollWidth, cjk: [...new Set([...document.querySelectorAll('main button,main label,main th,main h1,main h2,main h3,main [role=tab],main option,main p,main span')].filter(e => e.children.length === 0).map(e => e.textContent.trim()).filter(s => /[一-鿿]/.test(s) && s.length < 50))].slice(0, 15), keys: [...new Set((document.querySelector('main')?.innerText || '').match(/\b(admin|common|error)\.[a-zA-Z0-9_.]+/g) || [])] }))
    await shot(page, `d-${tag}-${r.replace(/\//g, '_')}`, tag === 'm')
    log(tag, r, 'hscroll', info.hs, info.sw, 'keys', info.keys.join(','), tag === 'en-US' ? 'CJK: ' + info.cjk.join(' | ') : '')
  }
  if (tag === 'm') {
    await page.goto(SITES.sa.admin + '/orders'); await page.waitForLoadState('networkidle'); await page.getByPlaceholder('订单号').fill('DJ20260926022050288592'); await page.waitForTimeout(1500)
    await page.getByRole('button', { name: '查看详情' }).first().click(); await page.waitForTimeout(1500); await shot(page, 'd-m-order-detail', false)
    log('m dialog width', await page.locator('[role=dialog]').last().evaluate(e => e.getBoundingClientRect().width))
  }
  log(tag, 'problems', JSON.stringify(sink).slice(0, 400)); await ctx.close()
}
await b.close()
