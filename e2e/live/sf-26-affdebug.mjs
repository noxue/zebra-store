// F-104 debug: capture the withdraw request the UI sends. Settings restored.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm } from './sf-lib.mjs'
const before = (await adm('settings/affiliate')).data
const b = await launch(); const R = {}
try {
  await adm('settings/affiliate', { method: 'PUT', body: { enabled: true, commission_rate: 10, confirm_days: 0, min_withdraw_amount: 0.2, withdraw_channels: ['支付宝', 'USDT'] } })
  const P = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(P.ctx)
  const reqs = []
  P.page.on('response', async (r) => { if (r.url().includes('/affiliate/withdraws') && r.request().method() === 'POST') reqs.push({ body: r.request().postData(), res: (await r.text()).slice(0, 300) }) })
  for (let i = 0; i < 20 && !(await api('public/config')).data.affiliate.enabled; i++) await sleep(1500)
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2500)
  R.cfg = (await api('public/config')).data.affiliate
  R.controls = await P.page.evaluate(() => [...document.querySelectorAll('main input, main select, main datalist, main [role=combobox]')].map((e) => `${e.tagName}:${e.type}:${e.placeholder}:${e.getAttribute('list')}`))
  await P.page.locator('main input[placeholder*="提现金额"]').fill('0.30')
  const sel = P.page.locator('main select').first()
  R.options = await sel.locator('option').allInnerTexts()
  await P.page.locator('main input[placeholder*="提现金额"]').fill('0.10')
  await sel.selectOption({ label: '支付宝' })
  await P.page.locator('main input[placeholder*="提现账号"]').fill('qa@alipay.test')
  await P.page.getByRole('button', { name: /提交申请/ }).click(); await sleep(2500)
  R.belowMinToast = (await P.page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/')
  await P.page.locator('main input[placeholder*="提现金额"]').fill('9.00')
  await P.page.getByRole('button', { name: /提交申请/ }).click(); await sleep(2500)
  R.tooMuchToast = (await P.page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/')
  await P.page.locator('main input[placeholder*="提现金额"]').fill('0.30')
  await P.page.locator('main input[placeholder*="提现账号"]').fill('qa@alipay.test')
  await P.page.getByRole('button', { name: /提交申请/ }).click(); await sleep(2500)
  R.reqs = reqs; R.toast = (await P.page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/')
  const tok = await P.page.evaluate(() => localStorage.getItem('user_token'))
  R.dashAfterSubmit = (await api('affiliate/dashboard', { token: tok })).data
  const ws = (await adm('affiliates/withdraws?page=1&page_size=10')).data || []
  const mine = ws.find((w) => JSON.stringify(w).includes('qa-sf-mmuh9ewag'))
  R.mine = mine && [mine.id, mine.status, mine.amount]
  if (mine) R.reject = (await adm(`affiliates/withdraws/${mine.id}/reject`, { method: 'POST', body: { reason: 'QA-SF reject test' } })).msg
  R.dashAfterReject = (await api('affiliate/dashboard', { token: tok })).data
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  R.rejectShot = await shot(P.page, 'F104-after-reject-m', true)
  await P.page.locator('main input[placeholder*="提现金额"]').fill('0.30')
  await P.page.locator('main select').first().selectOption({ label: 'USDT' })
  await P.page.locator('main input[placeholder*="提现账号"]').fill('TQA-SF-wallet')
  await P.page.getByRole('button', { name: /提交申请/ }).click(); await sleep(2500)
  const ws2 = (await adm('affiliates/withdraws?page=1&page_size=10')).data || []
  const m2 = ws2.find((w) => JSON.stringify(w).includes('qa-sf-mmuh9ewag') && w.id !== (R.mine || [])[0])
  if (m2) R.pay = (await adm(`affiliates/withdraws/${m2.id}/pay`, { method: 'POST', body: {} })).msg
  R.dash = (await api('affiliate/dashboard', { token: tok })).data
  await go(P.page, S + '/me/affiliate', P.diag); await sleep(2000)
  R.shot = await shot(P.page, 'F104-after-direct-m', true)
} catch (e) { R.error = e.message.slice(0, 300) } finally {
  R.restored = (await adm('settings/affiliate', { method: 'PUT', body: before })).status_code
  record({ flow: 'F-104-debug', R })
}
await b.close()
