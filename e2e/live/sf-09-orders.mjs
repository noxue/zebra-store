// F-051 multi-item wallet order, F-045 per-user limit, F-044 insufficient balance, F-073 copy.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const summary = async (p) => { const t = await txt(p); const i = t.indexOf('商品数量'); return t.slice(i, i + 330) }
{
  const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
  await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: S })
  await go(page, S + '/cart', diag); await sleep(1000)
  await page.getByText('去结算').first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  await page.locator('input[placeholder*="优惠券"]').fill('QASF10'); await page.keyboard.press('Enter'); await sleep(2500)
  const perUser = { sum: await summary(page), shot: await shot(page, 'F045-per-user-limit-d', true) }
  await page.locator('input[placeholder*="优惠券"]').fill(''); await page.keyboard.press('Enter'); await sleep(2000)
  await page.getByText('优先使用余额支付').click(); await sleep(1500)
  const before = await summary(page)
  await page.getByRole('button', { name: /提交订单/ }).click(); await sleep(6000)
  const url = page.url()
  const t = await txt(page)
  const s = await shot(page, 'F051-multi-order-detail-d', true)
  // copy
  const copyBtns = page.getByRole('button', { name: /复制内容/ })
  const nCopy = await copyBtns.count()
  let clip = ''
  if (nCopy) { await copyBtns.first().click(); await sleep(600); clip = await page.evaluate(() => navigator.clipboard.readText()).catch((e) => 'ERR ' + e.message) }
  const toast = (await page.locator('[role=status], [class*=toast]').allInnerTexts()).join('/')
  const cartAfter = await page.evaluate(() => localStorage.getItem('cart_items'))
  record({ flow: 'F-051/F-045/F-073', perUser, before, url, t: t.slice(0, 2500), nCopy, clip: clip.slice(0, 300), toast, cartAfter, diag, shots: [perUser.shot, s] })
  await ctx.storageState({ path: '.state/sf-d.json' })
  await ctx.close()
}
{
  const { ctx, page, diag } = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(ctx)
  await go(page, `${S}/products/claude-max`, diag); await sleep(800)
  await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  const s0 = await shot(page, 'F044-checkout-insufficient-m', true)
  await page.getByText('优先使用余额支付').click(); await sleep(1500)
  const sum = await summary(page)
  const s1 = await shot(page, 'F044-checkout-insufficient-balance-m', true)
  const btn = page.getByRole('button', { name: /提交订单/ })
  const disabled = await btn.isDisabled()
  let after = ''
  if (!disabled) { await btn.click(); await sleep(5000); after = page.url() + ' ' + (await txt(page)).slice(0, 800) }
  record({ flow: 'F-044', sum, disabled, after, diag, shots: [s0, s1, await shot(page, 'F044-after-submit-m', true)] })
  await ctx.storageState({ path: '.state/sf-m.json' })
  await ctx.close()
}
await b.close()
