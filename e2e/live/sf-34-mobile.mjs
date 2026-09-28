// Mobile (390x844) layout sweep of member pages + horizontal overflow check.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const PAGES = ['/', '/products', '/products/google-account', '/cart', '/checkout', '/me', '/me/orders', '/me/wallet', '/me/affiliate', '/me/gift-cards', '/me/security', '/me/api', '/me/profile', '/orders/DJ20260926022209006712', '/guest/orders', '/blog/qa-sf-howto', '/about', '/pay?order_no=DJ20260926022209006712']
const b = await launch()
const out = {}
for (const dark of [false, true]) {
  const { ctx, page, diag } = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(ctx)
  if (dark) await ctx.addInitScript(() => localStorage.setItem('dujiao_theme', 'dark'))
  await ctx.addInitScript(() => localStorage.setItem('cart_items', JSON.stringify([{ productId: 7, skuId: 10, skuCode: 'new', slug: 'google-account', title: { 'zh-CN': 'Google 账号', 'zh-TW': 'Google 帳號', 'en-US': 'Google Account' }, priceAmount: '9.90', quantity: 1, minPurchaseQuantity: 1, maxPurchaseQuantity: 5, purchaseType: 'guest', fulfillmentType: 'auto' }])))
  for (const p of dark ? PAGES.slice(0, 8) : PAGES) {
    const ms = await go(page, S + p, diag); await sleep(1500)
    const overflow = await page.evaluate(() => { const w = document.documentElement.clientWidth; const off = [...document.querySelectorAll('body *')].filter((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.right > w + 2 && getComputedStyle(e).position !== 'fixed' }).map((e) => e.tagName + '.' + (e.className?.toString?.() || '').split(' ').slice(0, 2).join('.')); return { scrollW: document.documentElement.scrollWidth, w, off: off.slice(0, 5) } })
    out[(dark ? 'dark ' : '') + p] = { ms, overflow, shot: await shot(page, `M-${dark ? 'dark' : 'light'}${p.replace(/[/?=]/g, '_')}`, true) }
  }
  out[(dark ? 'dark' : 'light') + ' diag'] = diag
  await ctx.close()
}
record({ flow: 'MOBILE', out })
await b.close()
