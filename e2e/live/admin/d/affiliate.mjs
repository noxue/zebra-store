import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
export default async ({ page, shot, log, admin, api }) => {
  const call = (m, p, b, tok) => page.evaluate(async ({ m, p, b, tok }) => (await (await fetch('/api/v1' + p, { method: m, headers: { 'content-type': 'application/json', ...(tok ? { authorization: 'Bearer ' + tok } : {}) }, body: b ? JSON.stringify(b) : undefined })).json()), { m, p, b, tok })
  const orig = (await api('GET', '/admin/settings/affiliate')).data; fs.writeFileSync('live/admin/out/d-backup-affiliate.json', JSON.stringify(orig)); log('backup', JSON.stringify(orig))
  try {
    await page.goto(admin + '/affiliates/settings'); await page.waitForLoadState('networkidle')
    await shot(page, 'd-aff-settings-before')
    await page.locator('main [role=switch]').first().click()
    const nums = page.locator('main input'); log('inputs', await nums.count(), (await nums.evaluateAll(e => e.map(x => x.type + ':' + x.placeholder))).join(' | '))
    await nums.nth(0).fill('10'); await nums.nth(1).fill('0'); await nums.nth(2).fill('1')
    // withdraw channels: maybe a tag input
    const wc = page.locator('main').getByPlaceholder(/渠道|channel/i); if (await wc.count()) { await wc.first().fill('alipay'); await wc.first().press('Enter') }
    await shot(page, 'd-aff-settings-filled')
    const [r] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), page.getByRole('button', { name: /保存/ }).click()])
    await page.waitForTimeout(700); log('save', r?.request().postData(), (await r?.text())?.slice(0, 200))
    const pub = await page.evaluate(async () => JSON.stringify((await (await fetch('/api/v1/public/config')).json()).data.affiliate)); log('public affiliate', pub)
    // promoter = qa user
    const tokA = (await call('POST', '/auth/login', { email: U.email, password: U.password })).data.token
    const op = await call('POST', '/affiliate/open', null, tokA); log('open', JSON.stringify(op).slice(0, 250)); const code = op.data?.affiliate_code || op.data?.code || op.data?.profile?.affiliate_code
    // buyer2
    const e2 = `qa-user-d2-${Date.now()}@lab.test`; const reg = await call('POST', '/auth/register', { email: e2, password: 'QaUser12345!', code: '', agreement_accepted: true }); const tokB = reg.data.token; const idB = reg.data.user.id
    fs.writeFileSync('live/admin/d/qa-user2.json', JSON.stringify({ id: idB, email: e2 }))
    log('topup buyer2', (await api('POST', `/admin/users/${idB}/wallet/adjust`, { operation: 'add', amount: '20', remark: 'qa affiliate buyer' })).status_code)
    const o = await call('POST', '/orders/create-and-pay', { items: [{ product_id: 8, sku_id: 12, quantity: 1 }], use_balance: true, affiliate_code: code }, tokB); log('aff order', JSON.stringify(o).slice(0, 160))
    await page.waitForTimeout(2000)
    const dash = await call('GET', '/affiliate/dashboard', null, tokA); log('promoter dashboard', JSON.stringify(dash.data).slice(0, 400))
    // admin pages
    await page.goto(admin + '/affiliates/users'); await page.waitForLoadState('networkidle'); await shot(page, 'd-aff-users'); log('aff users', (await page.locator('main tbody').innerText()).replace(/\s+/g, ' ').slice(0, 300))
    await page.goto(admin + '/affiliates/commissions'); await page.waitForLoadState('networkidle'); await shot(page, 'd-aff-commissions'); log('commissions', (await page.locator('main tbody').innerText()).replace(/\s+/g, ' ').slice(0, 300))
    const w = await call('POST', '/affiliate/withdraws', { amount: '0.5', channel: 'alipay', account: 'qa@alipay' }, tokA); log('withdraw apply', JSON.stringify(w).slice(0, 200))
    const w2 = await call('POST', '/affiliate/withdraws', { amount: '0.4', channel: 'alipay', account: 'qa@alipay' }, tokA); log('withdraw 2', JSON.stringify(w2).slice(0, 200))
    await page.goto(admin + '/affiliates/withdraws'); await page.waitForLoadState('networkidle'); await shot(page, 'd-aff-withdraws'); log('withdraws', (await page.locator('main tbody').innerText()).replace(/\s+/g, ' ').slice(0, 400))
    fs.writeFileSync('live/admin/d/aff.json', JSON.stringify({ code, idB }))
  } finally {
    const rr = await api('PUT', '/admin/settings/affiliate', orig); log('restore', rr.status_code, JSON.stringify((await api('GET', '/admin/settings/affiliate')).data))
  }
}
