// F-054 guest pending limit, F-055 IP blacklist, F-066 wallet-only checkout. All settings restored.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm } from './sf-lib.mjs'
const riskBefore = (await adm('settings?key=order_risk_control_config')).data
const walletBefore = (await adm('settings?key=wallet_config')).data
const R = {}
const guest = (i) => api('guest/orders', { method: 'POST', body: { items: [{ product_id: 7, sku_id: 10, quantity: 1 }], email: `qa-sf-risk${Date.now()}${i}@lab.test`, order_password: 'riskpw123' } })
try {
  R.set1 = (await adm('settings', { method: 'PUT', body: { key: 'order_risk_control_config', value: { version: 2, enabled: true, common: { ip_blacklist: [] }, guest: { enabled: true, max_pending_orders_per_ip: 2, max_quantity_per_product_per_order: 0, max_pending_quantity_per_ip_product: 0, payment_expire_minutes: 0, rate_limit: { enabled: false, window_seconds: 60, max_requests: 3, block_seconds: 120 } }, member: { enabled: false, max_pending_orders_per_user: 0, max_pending_orders_per_ip: 0, max_quantity_per_product_per_order: 0, rate_limit: { enabled: false, window_seconds: 60, max_requests: 3, block_seconds: 120 } } } } })).msg
  await sleep(2000)
  const res = []
  for (let i = 0; i < 4; i++) { const r = await guest(i); res.push([r.status_code, r.msg, r.data?.order_no]) }
  R.guestPending = res
  R.set2 = (await adm('settings', { method: 'PUT', body: { key: 'order_risk_control_config', value: { version: 2, enabled: true, common: { ip_blacklist: ['143.246.59.65'] }, guest: { enabled: false, max_pending_orders_per_ip: 0, max_quantity_per_product_per_order: 0, max_pending_quantity_per_ip_product: 0, payment_expire_minutes: 0, rate_limit: { enabled: false, window_seconds: 60, max_requests: 3, block_seconds: 120 } }, member: { enabled: false, max_pending_orders_per_user: 0, max_pending_orders_per_ip: 0, max_quantity_per_product_per_order: 0, rate_limit: { enabled: false, window_seconds: 60, max_requests: 3, block_seconds: 120 } } } } })).msg
  await sleep(2000)
  const bl = await guest(9); R.blacklisted = [bl.status_code, bl.msg]
  // member through UI while blacklisted
  const b = await launch()
  const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
  await go(page, `${S}/products/google-account`, diag); await sleep(800)
  await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  await page.getByText('优先使用余额支付').click(); await sleep(1200)
  await page.getByRole('button', { name: /提交订单/ }).click(); await sleep(3000)
  R.memberBlacklistedUI = { url: page.url(), toast: (await page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/'), shot: await shot(page, 'F055-blacklisted-checkout-d', true) }
  await adm('settings', { method: 'PUT', body: { key: 'order_risk_control_config', value: riskBefore && Object.keys(riskBefore).length ? riskBefore : { version: 2, enabled: false, common: { ip_blacklist: [] }, guest: { enabled: true, max_pending_orders_per_ip: 2, max_quantity_per_product_per_order: 1, max_pending_quantity_per_ip_product: 2, payment_expire_minutes: 10, rate_limit: { enabled: true, window_seconds: 60, max_requests: 3, block_seconds: 120 } }, member: { enabled: true, max_pending_orders_per_user: 5, max_pending_orders_per_ip: 0, max_quantity_per_product_per_order: 0, rate_limit: { enabled: true, window_seconds: 60, max_requests: 10, block_seconds: 60 } } } } })
  // F-066 wallet only
  R.w1 = (await adm('settings', { method: 'PUT', body: { key: 'wallet_config', value: { ...walletBefore, wallet_only_payment: true } } })).msg
  for (let i = 0; i < 15 && !(await api('public/config')).data?.wallet_only_payment; i++) await sleep(1500)
  R.cfgWalletOnly = (await api('public/config')).data?.wallet_only_payment
  await go(page, `${S}/products/google-account`, diag); await sleep(800)
  await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
  const cb = page.locator('main input[type=checkbox]').last()
  R.walletOnlyUI = { checked: await cb.isChecked().catch(() => 'n/a'), disabled: await cb.isDisabled().catch(() => 'n/a'), t: (await page.locator('main').first().innerText()).replace(/\s+/g, ' ').match(/支付方式.{0,200}/)?.[0], shot: await shot(page, 'F066-wallet-only-checkout-d', true) }
  const g2 = await open(b, { mobile: true }); await noAnnouncement(g2.ctx)
  await go(g2.page, `${S}/products/google-account`, g2.diag); await sleep(800)
  await g2.page.getByRole('button', { name: /立即购买/ }).first().click(); await g2.page.waitForURL(/checkout/); await sleep(2500)
  R.walletOnlyGuest = { t: (await g2.page.locator('main').first().innerText()).replace(/\s+/g, ' ').match(/支付方式.{0,200}/)?.[0], shot: await shot(g2.page, 'F066-wallet-only-guest-m', true) }
  R.diag = diag
  await b.close()
} catch (e) { R.error = e.message.slice(0, 400) } finally {
  R.restoreRisk = (await adm('settings?key=order_risk_control_config')).data?.common
  if (JSON.stringify(R.restoreRisk || {}).includes('143.246')) await adm('settings', { method: 'PUT', body: { key: 'order_risk_control_config', value: { ...(await adm('settings?key=order_risk_control_config')).data, enabled: false, common: { ip_blacklist: [] } } } })
  R.restoreWallet = (await adm('settings', { method: 'PUT', body: { key: 'wallet_config', value: { ...walletBefore, wallet_only_payment: false } } })).msg
  R.final = [(await adm('settings?key=order_risk_control_config')).data?.enabled, JSON.stringify((await adm('settings?key=order_risk_control_config')).data?.common), JSON.stringify((await adm('settings?key=wallet_config')).data)]
  record({ flow: 'F-054/F-055/F-066', riskBefore, walletBefore, R })
}
