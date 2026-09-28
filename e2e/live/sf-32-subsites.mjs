// R-008/R-010/R-011 on SK/NE/MA: branding, member area entries, displayed vs charged price.
import { launch, open, go, shot, record, SITES, sleep, noAnnouncement, api, QAPW } from './sf-lib.mjs'
const b = await launch()
const body = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
for (const k of ['SK', 'NE', 'MA']) {
  const base = SITES[k]
  const cfg = (await api('public/config', { base })).data
  for (const mobile of [false, true]) {
    const m = mobile ? 'm' : 'd'
    const { ctx, page, diag } = await open(b, { mobile }); await noAnnouncement(ctx, base)
    await go(page, base + '/', diag); await sleep(1500)
    const logo = await page.evaluate(() => [...document.querySelectorAll('header img')].map((i) => [i.currentSrc.split('/').pop(), i.naturalWidth]))
    const s1 = await shot(page, `R008-${k}-home-${m}`)
    await go(page, base + '/products/lab-e2e-card', diag); await sleep(1500)
    const detailPrice = (await body(page)).match(/价格.{0,40}/)?.[0]
    const s2 = await shot(page, `R011-${k}-detail-${m}`)
    record({ flow: 'R-008', site: k, mobile, siteName: cfg.brand.site_name, seo: cfg.seo?.title?.['zh-CN'], annTitle: cfg.announcement?.title?.['zh-CN'], logo, detailPrice, diag, shots: [s1, s2] })
    await ctx.close()
  }
}
// R-011 purchase on Sakura as QA user d (desktop)
{
  const base = SITES.SK
  const tok = (await api('auth/login', { method: 'POST', body: { email: 'qa-sf-dmuh9ewag@lab.test', password: QAPW }, base })).data?.token
  const walletBefore = (await api('wallet', { token: tok, base })).data?.balance
  const { ctx, page, diag } = await open(b); await noAnnouncement(ctx, base)
  await ctx.addInitScript((t) => localStorage.setItem('user_token', t), tok)
  await go(page, base + '/me', diag); await sleep(1500)
  const menu = (await body(page)).match(/概览.{0,120}/)?.[0]
  const sMe = await shot(page, 'R008-SK-personal-center-d', true)
  await go(page, base + '/products/lab-e2e-card', diag); await sleep(1500)
  const detail = (await body(page)).match(/价格.{0,60}/)?.[0]
  await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  const t = await body(page)
  const hasCoupon = await page.locator('input[placeholder*="优惠券"]').isVisible().catch(() => false)
  const sum = t.slice(t.indexOf('商品数量'), t.indexOf('商品数量') + 250)
  const itemLine = t.match(/联调测试卡 10 元 数量.{0,120}/)?.[0]
  const s1 = await shot(page, 'R011-SK-checkout-d', true)
  await page.getByText('优先使用余额支付').click(); await sleep(1200)
  await page.getByRole('button', { name: /提交订单/ }).click(); await sleep(5000)
  const after = { url: page.url(), t: (await body(page)).match(/实付金额.{0,40}/)?.[0], shot: await shot(page, 'R011-SK-order-d', true) }
  const walletAfter = (await api('wallet', { token: tok, base })).data?.balance
  record({ flow: 'R-011', menu, detail, hasCoupon, sum, itemLine, walletBefore, walletAfter, after, diag, shots: [sMe, s1] })
  // same order visible on main site
  const orders = (await api('orders?page=1&page_size=3', { token: tok })).data?.map((o) => [o.order_no, o.total_amount, o.status])
  record({ flow: 'R-011-main', orders })
}
await b.close()
