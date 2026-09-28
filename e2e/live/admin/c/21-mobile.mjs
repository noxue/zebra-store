import { adminContext, shot, log, SITES, watch } from '../lib.mjs'
import { browser } from '../lib.mjs'
const b = await browser()
for (const [loc, mobile] of [['zh-TW', false], ['en-US', true]]) {
  const ctx = await adminContext(b, { locale: loc, mobile, viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 900 } })
  const page = await ctx.newPage()
  const probs = watch(page, [])
  const tag = `${loc}${mobile ? '-m' : ''}`
  for (const route of ['products', 'coupons', 'gift-cards', 'card-secrets', 'media']) {
    await page.goto(`${SITES.sa.admin}/${route}`)
    await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => log('slow', route))
    await page.waitForTimeout(600)
    const hs = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)
    log(tag, route, 'hscroll', hs)
    if (route === 'products') {
      await shot(page, `c-${tag}-products`, false)
      await page.locator('tbody tr').filter({ hasText: 'qa-product-0926' }).getByRole('button').filter({ hasText: /编辑|編輯|Edit/ }).first().click()
      await page.waitForTimeout(1500)
      const dlg = page.locator('[role=dialog]').last()
      const box = await dlg.boundingBox()
      log(tag, 'edit dialog box', JSON.stringify(box))
      const txt = await dlg.innerText()
      const odd = [...new Set((txt.match(/[A-Za-z][a-z]+(?: [A-Za-z][a-z]+){1,4}/g) || []))].filter((s) => !/qa|Standard|Pro|SKU|CNY|Slug|SEO|CSV|TXT|Telegram/i.test(s)).slice(0, 15)
      if (loc !== 'en-US') log(tag, 'english phrases in dialog', JSON.stringify(odd))
      await shot(page, `c-${tag}-product-edit`, false)
      await dlg.evaluate((e) => e.scrollTo(0, e.scrollHeight)).catch(() => {})
      await page.keyboard.press('Escape')
    } else if (mobile) await shot(page, `c-${tag}-${route}`, false)
  }
  log(tag, 'problems', JSON.stringify(probs).slice(0, 500))
  await ctx.close()
}
await b.close()
