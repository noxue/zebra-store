// F-100..F-104 affiliate: open, click tracking, attributed order, refund claw-back, withdraw. Settings restored.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, QAPW } from './sf-lib.mjs'
import fs from 'node:fs'
const fx = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
const before = (await adm('settings/affiliate')).data
const b = await launch()
const run = Date.now().toString(36)
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
const R = {}
try {
  R.enable = (await adm('settings/affiliate', { method: 'PUT', body: { enabled: true, commission_rate: 10, confirm_days: 0, min_withdraw_amount: 0.5, withdraw_channels: ['支付宝', 'USDT'] } })).status_code
  // F-100 promoter = user m (mobile)
  const P = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(P.ctx)
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(1500)
  R.before = { t: (await txt(P.page)).slice(0, 600), shot: await shot(P.page, 'F100-affiliate-closed-m', true) }
  const openBtn = P.page.getByRole('button', { name: /开通/ }).first()
  if (await openBtn.isVisible().catch(() => false)) { await openBtn.click(); await sleep(2500) }
  const tp = await txt(P.page)
  const code = (tp.match(/aff=([A-Za-z0-9]+)/) || [])[1] || (tp.match(/推广码\s*([A-Za-z0-9]{4,})/) || [])[1]
  R.opened = { code, t: tp.slice(tp.indexOf('推广返利', 300), tp.indexOf('推广返利', 300) + 900), shot: await shot(P.page, 'F100-affiliate-opened-m', true) }
  // F-101 visitor
  const V = await open(b); await noAnnouncement(V.ctx)
  const clicks = []
  V.page.on('request', (r) => { if (r.url().includes('/affiliate/click')) clicks.push(r.postData()) })
  await go(V.page, `${S}/?aff=${code}`, V.diag); await sleep(2000)
  await go(V.page, `${S}/?aff=${code}`, V.diag); await sleep(2000)
  R.visitor = { clicks: clicks.length, ls: await V.page.evaluate(() => [localStorage.getItem('dj_affiliate_attribution'), localStorage.getItem('dj_affiliate_visitor_key')]) }
  // visitor registers and buys the affiliate-enabled QA manual product
  const vEmail = `qa-sf-aff${run}@lab.test`
  await go(V.page, S + '/auth/register', V.diag); await sleep(800)
  await V.page.fill('input[type=email]', vEmail); await V.page.fill('input[type=password]', QAPW)
  await V.page.locator('label:has(input[type=checkbox])').first().click({ position: { x: 8, y: 8 } })
  await V.page.click('button[type=submit]'); await sleep(3000)
  const vTok = await V.page.evaluate(() => localStorage.getItem('user_token'))
  const vid = (await api('me', { token: vTok })).data?.id
  R.topup = (await adm(`users/${vid}/wallet/adjust`, { method: 'POST', body: { amount: '20.00', operation: 'add', remark: 'QA-SF affiliate buyer' } })).status_code
  await go(V.page, `${S}/products/qa-sf-manual`, V.diag); await sleep(800)
  await V.page.getByRole('button', { name: /立即购买/ }).first().click(); await V.page.waitForURL(/checkout/); await sleep(2500)
  await V.page.locator('main input[type=email]').first().fill('aff@lab.test')
  await V.page.locator('main select').first().selectOption({ label: '亚服' })
  await V.page.getByText('优先使用余额支付').click(); await sleep(1200)
  await V.page.getByRole('button', { name: /提交订单/ }).click(); await sleep(5000)
  const orderNo = (V.page.url().match(/orders\/(DJ\w+)/) || [])[1]
  R.affOrder = { orderNo, url: V.page.url() }
  await sleep(3000)
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  const t2 = await txt(P.page)
  R.dashAfterOrder = { t: t2.slice(t2.indexOf('推广返利', 300), t2.indexOf('推广返利', 300) + 1500), shot: await shot(P.page, 'F102-affiliate-after-order-m', true), api: (await api('affiliate/dashboard', { token: await P.page.evaluate(() => localStorage.getItem('user_token')) })).data }
  // F-103 refund half (paid 6.00, promoter gets 10%)
  if (orderNo) {
    const o = (await adm(`orders?page=1&page_size=5&order_no=${orderNo}`)).data?.[0]
    R.refund = (await adm(`orders/${o.id}/refund-to-wallet`, { method: 'POST', body: { amount: '3.00', remark: 'QA-SF aff clawback' } })).status_code
    await sleep(2000)
    R.dashAfterRefund = (await api('affiliate/dashboard', { token: await P.page.evaluate(() => localStorage.getItem('user_token')) })).data
    R.commissions = (await api('affiliate/commissions?page=1&page_size=10', { token: await P.page.evaluate(() => localStorage.getItem('user_token')) })).data
  }
  // F-104 withdraw through UI
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  R.beforeWithdraw = { shot: await shot(P.page, 'F104-affiliate-before-withdraw-m', true), inputs: await P.page.evaluate(() => [...document.querySelectorAll('main input, main select')].filter((e) => e.offsetParent).map((e) => `${e.tagName}:${e.type}:${e.placeholder}`)) }
  const amt = P.page.locator('main input[placeholder*="金额"], main input[type=number]').first()
  if (await amt.isVisible().catch(() => false)) {
    await amt.fill('0.30')
    const sel = P.page.locator('main select').first(); if (await sel.isVisible().catch(() => false)) await sel.selectOption({ index: 1 }).catch(() => {})
    const acc = P.page.locator('main input[placeholder*="账号"]').first(); if (await acc.isVisible().catch(() => false)) await acc.fill('qa@alipay')
    await P.page.getByRole('button', { name: /提现/ }).last().click(); await sleep(2500)
    R.withdraw = { toast: (await toastsOf(P.page)).join('/'), t: (await txt(P.page)).match(/可提现.{0,200}/)?.[0], shot: await shot(P.page, 'F104-withdraw-submitted-m', true) }
    const ws = (await adm('affiliates/withdraws?page=1&page_size=5')).data || []
    const mine = ws.find((w) => (w.user?.email || w.email || '').includes('qa-sf-m')) || ws[0]
    if (mine) { R.reject = (await adm(`affiliates/withdraws/${mine.id}/reject`, { method: 'POST', body: { reason: 'QA-SF reject test' } })).status_code }
    await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
    R.afterReject = { t: (await txt(P.page)).match(/可提现.{0,300}/)?.[0], shot: await shot(P.page, 'F104-after-reject-m', true) }
  }
  record({ flow: 'F-100..F-104', R, diagP: P.diag, diagV: V.diag })
} finally {
  const r = await adm('settings/affiliate', { method: 'PUT', body: before })
  console.log('restored affiliate', r.status_code, JSON.stringify((await adm('settings/affiliate')).data))
}
await b.close()
