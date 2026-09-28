export default async ({ page, admin, base, shot, log, problems, api }) => {
  const orig = (await api('GET', '/admin/settings?key=callback_routes_config')).data
  log('orig', JSON.stringify(orig))
  const inp = () => page.locator('main input').first()
  const save = async (v, name) => {
    await page.goto(`${admin}/callback-routes`); await page.waitForLoadState('networkidle')
    await inp().fill(v)
    const n0 = problems.length
    await page.getByRole('button', { name: /保存/ }).last().click(); await page.waitForTimeout(1500)
    await shot(page, 'b-032-' + name, false)
    log(name, v, JSON.stringify(problems.slice(n0)), JSON.stringify((await api('GET', '/admin/settings?key=callback_routes_config')).data))
  }
  try {
    await save('/pay/notify-x', 'bad-prefix')
    await save('/api/v1/admin/orders', 'conflict-admin')
    await save('/api/v1/payments/webhook/paypal', 'dup-other-key')
    await save('/api/v1/../admin/x', 'traversal')
    await save('/api/pay/notify-x', 'valid')
    const probe = async (p) => page.evaluate(async (p) => { const r = await fetch(p, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: 'out_trade_no=QA-NONEXIST&trade_status=TRADE_SUCCESS&sign=x' }); return r.status + ' ' + (await r.text()).slice(0, 80) }, p)
    log('POST new path', await probe('/api/pay/notify-x'))
    log('POST old path', await probe('/api/v1/payments/callback'))
    log('POST random', await probe('/api/pay/notify-nope'))
  } finally {
    const r = await api('PUT', '/admin/settings', { key: 'callback_routes_config', value: orig && Object.keys(orig).length ? orig : {} })
    log('restored', r.status_code, JSON.stringify((await api('GET', '/admin/settings?key=callback_routes_config')).data))
  }
}
