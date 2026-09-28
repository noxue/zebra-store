// F-090..F-093 wallet page, recharge (no channel), transactions, gift card redeem + reuse + concurrency.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, userToken } from './sf-lib.mjs'
const [c1, c2, c3] = process.argv.slice(2)
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
for (const mobile of [false, true]) {
  const m = mobile ? 'm' : 'd'
  const { ctx, page, diag } = await open(b, { mobile, storage: `.state/sf-${m}.json` }); await noAnnouncement(ctx)
  await go(page, S + '/me/wallet', diag); await sleep(1500)
  const t0 = await txt(page)
  const s0 = await shot(page, `F092-wallet-${m}`, true)
  // try recharge
  const amt = page.locator('main input[type=number], main input[placeholder*="金额"]').first()
  let rech = ''
  if (await amt.isVisible().catch(() => false)) {
    await amt.fill('10'); await sleep(1500)
    rech = (await txt(page)).match(/.{0,40}(支付方式|渠道|充值).{0,80}/g)?.slice(0, 4).join(' || ')
  }
  const s1 = await shot(page, `F090-recharge-${m}`, true)
  // gift card redeem
  await go(page, S + '/me/gift-cards', diag); await sleep(1200)
  const code = mobile ? c2 : c1
  const inp = page.locator('main input:visible').first()
  await inp.fill(code.toLowerCase())
  await page.getByRole('button', { name: /兑换/ }).first().click(); await sleep(2500)
  const r1 = { toast: (await toastsOf(page)).join('/'), t: (await txt(page)).slice(0, 900), shot: await shot(page, `F093-giftcard-redeemed-${m}`, true) }
  await inp.fill(code)
  await page.getByRole('button', { name: /兑换/ }).first().click(); await sleep(2500)
  const r2 = { toast: (await toastsOf(page)).join('/'), shot: await shot(page, `F093-giftcard-reuse-${m}`) }
  await go(page, S + '/me/wallet', diag); await sleep(1500)
  const t2 = await txt(page)
  record({ flow: 'F-090/F-092/F-093', mobile, wallet: t0.slice(t0.indexOf('我的钱包', 200), t0.indexOf('我的钱包', 200) + 1500), rech, r1, r2, walletAfter: t2.slice(t2.indexOf('我的钱包', 200), t2.indexOf('我的钱包', 200) + 900), diag, shots: [s0, s1, await shot(page, `F092-wallet-after-gift-${m}`, true)] })
  await ctx.close()
}
// concurrency: 2 users race on c3
const [ta, tb] = [await userToken('qa-sf-dmuh9ewag@lab.test'), await userToken('qa-sf-mmuh9ewag@lab.test')]
const res = await Promise.all([ta, tb, ta, tb].map((t) => api('gift-cards/redeem', { method: 'POST', body: { code: c3 }, token: t })))
record({ flow: 'F-093-concurrency', res: res.map((r) => [r.status_code, r.msg, r.data?.wallet_delta]) })
await b.close()
