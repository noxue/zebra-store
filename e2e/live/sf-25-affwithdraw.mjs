// F-104 affiliate withdraw via UI (reject → back to available, then pay). Settings restored.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm } from './sf-lib.mjs'
const before = (await adm('settings/affiliate')).data
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
const R = {}
try {
  await adm('settings/affiliate', { method: 'PUT', body: { enabled: true, commission_rate: 10, confirm_days: 0, min_withdraw_amount: 0.5, withdraw_channels: ['支付宝', 'USDT'] } })
  const P = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(P.ctx)
  const submit = async (amount, tag) => {
    await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
    await P.page.locator('main input[placeholder*="提现金额"]').fill(amount)
    await P.page.locator('main input[placeholder*="提现渠道"], main [placeholder*="渠道"]').first().click().catch(() => {})
    await sleep(500)
    const opt = P.page.getByText('支付宝', { exact: true }).last()
    if (await opt.isVisible().catch(() => false)) await opt.click(); else await P.page.locator('main input[placeholder*="提现渠道"]').fill('支付宝').catch(() => {})
    await P.page.locator('main input[placeholder*="提现账号"]').fill('qa@alipay.test')
    await P.page.getByRole('button', { name: /提交申请/ }).click(); await sleep(2500)
    return { toast: (await toastsOf(P.page)).join('/'), shot: await shot(P.page, `F104-withdraw-${tag}-m`, true) }
  }
  R.belowMin = await submit('0.10', 'below-min')
  R.tooMuch = await submit('5.00', 'too-much')
  R.ok = await submit('0.30', 'ok')
  const tok = await P.page.evaluate(() => localStorage.getItem('user_token'))
  R.dashAfterSubmit = (await api('affiliate/dashboard', { token: tok })).data
  const ws = (await adm('affiliates/withdraws?page=1&page_size=10')).data || []
  const mine = ws.find((w) => JSON.stringify(w).includes('qa-sf-mmuh9ewag'))
  R.mine = mine && [mine.id, mine.status, mine.amount]
  if (mine) R.reject = (await adm(`affiliates/withdraws/${mine.id}/reject`, { method: 'POST', body: { reason: 'QA-SF reject test' } })).msg
  R.dashAfterReject = (await api('affiliate/dashboard', { token: tok })).data
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  const t = await txt(P.page)
  R.afterRejectUI = { t: t.slice(t.indexOf('提现记录'), t.indexOf('提现记录') + 300), shot: await shot(P.page, 'F104-after-reject-m', true) }
  R.ok2 = await submit('0.30', 'ok2')
  const ws2 = (await adm('affiliates/withdraws?page=1&page_size=10')).data || []
  const mine2 = ws2.find((w) => JSON.stringify(w).includes('qa-sf-mmuh9ewag') && w.status === 'pending_review' || JSON.stringify(w).includes('qa-sf-mmuh9ewag') && w.status === 'pending')
  if (mine2) R.pay = (await adm(`affiliates/withdraws/${mine2.id}/pay`, { method: 'POST', body: {} })).msg
  R.dashAfterPay = (await api('affiliate/dashboard', { token: tok })).data
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  R.finalShot = await shot(P.page, 'F104-after-pay-m', true)
  R.diag = P.diag
} catch (e) { R.error = e.message.slice(0, 300) } finally {
  const r = await adm('settings/affiliate', { method: 'PUT', body: before })
  R.restored = r.status_code
  record({ flow: 'F-104', R })
}
await b.close()
