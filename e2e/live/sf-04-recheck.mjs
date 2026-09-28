// Rechecks: F-004 clear filters (desktop), F-015 sticky buy bar, F-009 reload in en-US.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const cards = (p) => p.evaluate(() => [...document.querySelectorAll('article')].filter((a) => a.offsetParent !== null).length)
{
  const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
  await go(page, S + '/products', diag); await sleep(800)
  const n0 = await cards(page)
  const input = page.locator('input[placeholder*="搜索"]:visible').first()
  await input.fill('zzzqqq-none'); await sleep(1500)
  await page.getByRole('button', { name: /清除筛选/ }).click(); await sleep(2000)
  const n1 = await cards(page); const val = await input.inputValue()
  const s = await shot(page, 'F004-after-clear-d')
  // category + clear
  await page.getByRole('button', { name: /AI 账号/ }).first().click(); await sleep(1200)
  await input.fill('zzz'); await sleep(1500)
  await page.getByRole('button', { name: /清除筛选/ }).click(); await sleep(2000)
  const n2 = await cards(page)
  record({ flow: 'F-004', recheck: true, n0, n1, val, n2, url: page.url(), diag, shots: [s, await shot(page, 'F004-after-clear-cat-d')] })
  await ctx.close()
}
{
  const { ctx, page, diag } = await open(b, { mobile: true }); await noAnnouncement(ctx)
  await go(page, S + '/products/aws-account', diag); await sleep(800)
  const out = []
  for (const y of [300, 700, 1200, 2000]) {
    await page.evaluate((yy) => window.scrollTo(0, yy), y); await sleep(700)
    const bar = await page.evaluate(() => [...document.querySelectorAll('div,nav,footer')].filter((e) => { const cs = getComputedStyle(e); return (cs.position === 'fixed' || cs.position === 'sticky') && e.offsetParent !== null && /立即购买|加入购物车/.test(e.innerText) }).length)
    out.push({ y, bar, shot: await shot(page, `F015-scroll-${y}-m`) })
  }
  // home + cart mobile
  await go(page, S + '/', diag); await sleep(800); const h = await shot(page, 'F015-home-m')
  await go(page, S + '/cart', diag); await sleep(800); const c = await shot(page, 'F015-cart-m')
  // hamburger menu
  const burger = page.locator('header button').last(); await burger.click(); await sleep(700); const menu = await shot(page, 'F015-menu-m')
  record({ flow: 'F-015', out, diag, shots: [h, c, menu] })
  await ctx.close()
}
{
  const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
  await ctx.addInitScript(() => localStorage.setItem('locale', 'en-US'))
  await go(page, S + '/products/aws-account', diag); await sleep(3000)
  const t = (await page.locator('main, #app').first().innerText()).replace(/\s+/g, ' ')
  const s = await shot(page, 'F009-en-detail-reload')
  record({ flow: 'F-009', recheck: true, t: t.slice(0, 500), diag, shots: [s] })
  await ctx.close()
}
await b.close()
