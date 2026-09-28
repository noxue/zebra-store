import { test, expect, shot } from '../support/fixtures'
import { EPAY, STOREFRONT_URL, RUN_ID } from '../support/env'
import { need, saveState } from '../support/state'
import { mockEpayGateway, parseEpayPayUrl, sendEpayCallback } from '../support/api'

test.describe.configure({ mode: 'serial' })

const GUEST_EMAIL = `guest-${RUN_ID}@e2e.test`
const GUEST_PASSWORD = 'guest-pass-123'

interface CreateAndPayData {
  order_no?: string
  order?: { order_no: string }
  payment?: { pay_url?: string }
}

test('home shows the configured site name and theme', async ({ page }) => {
  await page.goto(`${STOREFRONT_URL}/`)
  const siteName = need('siteName')
  await expect(page.getByText(siteName).first()).toBeVisible()
  const primary = await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--zs-primary').trim())
  expect(primary.toLowerCase()).toBe(need('primaryColor').toLowerCase())
  await expect(page).toHaveTitle(new RegExp(siteName))
  await expect(page.getByText(need('productTitle')).first()).toBeVisible()
  await shot(page, 'store-01-home')
})

test('guest: product detail → buy now → checkout → epay → delivered card secret → order lookup', async ({ page, context }) => {
  const gateway = await mockEpayGateway(context)
  const title = need('productTitle')

  await page.goto(`${STOREFRONT_URL}/`)
  await page.getByText(title).first().click()
  await expect(page).toHaveURL(new RegExp(`/products/${need('productSlug')}`))
  await expect(page.getByRole('heading', { name: title })).toBeVisible()
  await page.getByRole('button', { name: /Standard/ }).first().click()
  await shot(page, 'store-02-product-detail')
  await page.getByRole('button', { name: 'Buy now' }).first().click()

  await expect(page).toHaveURL(/\/checkout/)
  await page.getByRole('button', { name: 'Guest checkout' }).click()
  await page.getByPlaceholder('Email (for order lookup)').fill(GUEST_EMAIL)
  await page.getByPlaceholder('Order password').fill(GUEST_PASSWORD)
  await page.getByText('E2E Alipay (epay)').click()
  await shot(page, 'store-03-checkout-guest')

  const [res] = await Promise.all([
    page.waitForResponse((r) => r.request().method() === 'POST' && /\/guest\/orders\/create-and-pay$/.test(new URL(r.url()).pathname)),
    page.getByRole('button', { name: 'Place Order & Pay' }).click(),
  ])
  const created = ((await res.json()) as { status_code: number; msg: string; data: CreateAndPayData })
  expect(created.status_code, created.msg).toBe(0)
  const orderNo = created.data.order_no || created.data.order?.order_no || ''
  expect(orderNo).not.toBe('')

  // Redirect-mode channels send the browser straight to the gateway (stubbed by mockEpayGateway).
  await expect(page).toHaveURL(new RegExp(`^${EPAY.gatewayUrl.replace(/[.]/g, '\\.')}/submit\\.php`), { timeout: 20_000 })
  const payUrl = created.data.payment?.pay_url || gateway.opened[0] || page.url()
  const { outTradeNo, money, type, returnUrl } = parseEpayPayUrl(payUrl)
  expect(Number(money)).toBe(10)
  await sendEpayCallback(outTradeNo, money, type)

  // The gateway sends the customer back to return_url (the storefront /pay page).
  await page.goto(returnUrl)
  // The payment page polls; after success it lands on the guest order detail with the delivered secret.
  await expect(page).toHaveURL(new RegExp(`/guest/orders/${orderNo}`), { timeout: 30_000 })
  await expect(page.getByText(new RegExp(`STD-CARD-${need('runId')}-\\d`)).first()).toBeVisible({ timeout: 30_000 })
  await shot(page, 'store-05-guest-order-delivered')

  saveState({ guestEmail: GUEST_EMAIL, guestPassword: GUEST_PASSWORD, guestOrderNo: orderNo })
})

test('guest order lookup finds the order with its card secret', async ({ page }) => {
  await page.goto(`${STOREFRONT_URL}/guest/orders`)
  await page.getByPlaceholder('Email', { exact: true }).fill(need('guestEmail'))
  await page.getByPlaceholder('Order password', { exact: true }).fill(need('guestPassword'))
  await page.getByRole('button', { name: 'Search orders' }).click()
  const orderNo = need('guestOrderNo')
  await expect(page.getByText(orderNo).first()).toBeVisible()
  await shot(page, 'store-06-guest-lookup')
  await page.getByRole('link', { name: 'View details' }).or(page.getByRole('button', { name: 'View details' })).first().click()
  await expect(page).toHaveURL(new RegExp(`/guest/orders/${orderNo}`))
  await expect(page.getByText(/STD-CARD-/).first()).toBeVisible()
})
