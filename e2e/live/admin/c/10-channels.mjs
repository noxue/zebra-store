import { dialog, apiResponse, TS } from './common.mjs'
export default async ({ page, api, shot, log, admin }) => {
  const mk = (n) => ({ name: `qa-c-chan-${n}-${TS}`, provider_type: 'epay', channel_type: 'alipay', interaction_mode: 'redirect', fee_rate: '0', is_active: false, sort_order: 0,
    config_json: { gateway_url: 'https://epay.qa.invalid', merchant_id: '1000', merchant_key: 'qa-key-123456', epay_version: 'v1', notify_url: 'https://store.dot2.com/api/v1/payments/callback', return_url: 'https://store.dot2.com/pay' } })
  const ids = []
  for (const n of ['A', 'B']) {
    const r = await api('POST', '/admin/payment-channels', mk(n))
    log('create chan', n, r.status_code, r.msg, r.data?.id, r.data?.is_active)
    if (r.data?.id) ids.push(r.data.id)
  }
  await page.goto(`${admin}/products`)
  await page.waitForLoadState('networkidle')
  const row = page.locator('tbody tr', { hasText: `qa-product-${TS}` })
  await row.getByRole('button', { name: 'Edit' }).click()
  const dlg = dialog(page, 'Edit product')
  await page.waitForTimeout(1200)
  const box = dlg.locator('button[aria-pressed]')
  log('channel buttons in editor', await box.count(), (await box.allInnerTexts()).join(' / '))
  if (await box.count()) {
    await dlg.locator('button[aria-pressed]', { hasText: `qa-c-chan-A-${TS}` }).click()
    await shot(page, 'c-prod-channels')
    const r = await apiResponse(page, 'PUT', /\/admin\/products\/\d+$/, () => dlg.getByRole('button', { name: 'Save changes' }).click())
    log('saved payment_channel_ids', JSON.stringify(r.payment_channel_ids))
  }
  // API: set to both incl. a bogus id
  const cur = (await api('GET', '/admin/products/13')).data
  log('current payment_channel_ids', JSON.stringify(cur.payment_channel_ids))
  const pubCh = await page.evaluate(async () => (await (await fetch('/api/v1/public/config')).json()).data?.payment_channels)
  log('public channels', JSON.stringify(pubCh).slice(0, 300))
  log('channel ids', JSON.stringify(ids))
}
