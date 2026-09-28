import { test, expect, shot } from '../support/fixtures'
import { EPAY, RUN_ID, STOREFRONT_URL } from '../support/env'
import { loadState, need, saveState } from '../support/state'
import { mockEpayGateway, parseEpayPayUrl, sendEpayCallback } from '../support/api'
import { sfField } from '../support/ui'
import type { Page } from '@playwright/test'

test.describe.configure({ mode: 'serial' })

const EMAIL = `member-${RUN_ID}@e2e.test`
const PASSWORD = 'Member12345'
const gatewayRe = new RegExp(`^${EPAY.gatewayUrl.replace(/[.]/g, '\\.')}/submit\\.php`)

async function login(page: Page, email: string, password: string) {
  await page.goto(`${STOREFRONT_URL}/auth/login`)
  await sfField(page, 'Email').fill(email)
  await sfField(page, 'Password').fill(password)
  await page.getByRole('button', { name: 'Login', exact: true }).click()
  await expect(page).not.toHaveURL(/\/auth\/login/)
}

test('register (no email verification) and log in', async ({ page }) => {
  await page.goto(`${STOREFRONT_URL}/auth/register`)
  await page.getByPlaceholder('Enter email').fill(EMAIL)
  await page.getByPlaceholder('Set login password').fill(PASSWORD)
  await expect(page.getByPlaceholder('Enter email code')).toHaveCount(0)
  await page.getByText('I have read and agree to').click()
  await shot(page, 'store-10-register')
  await page.getByRole('button', { name: 'Create account' }).click()
  await expect(page).not.toHaveURL(/\/auth\/register/)
  // new member → start a fresh order list (state.json persists across runs)
  saveState({ memberEmail: EMAIL, memberPassword: PASSWORD, memberOrderNos: [] })

  // fresh session: sign in through the login form
  await page.evaluate(() => localStorage.clear())
  await login(page, EMAIL, PASSWORD)
  await page.goto(`${STOREFRONT_URL}/me`)
  await expect(page.getByText(EMAIL).first()).toBeVisible()
  await shot(page, 'store-11-personal-center')
})

test('cart → checkout with coupon → wallet recharge → pay the order with balance', async ({ page, context }) => {
  const gateway = await mockEpayGateway(context)
  await login(page, need('memberEmail'), need('memberPassword'))

  // Add the Pro SKU to the cart
  await page.goto(`${STOREFRONT_URL}/products/${need('productSlug')}`)
  await page.getByRole('button', { name: /Pro/ }).first().click()
  await page.getByRole('button', { name: 'Add to cart' }).first().click()
  await page.goto(`${STOREFRONT_URL}/cart`)
  await expect(page.getByText(need('productTitle')).first()).toBeVisible()
  await shot(page, 'store-12-cart')
  await page.getByRole('link', { name: 'Checkout' }).or(page.getByRole('button', { name: 'Checkout' })).first().click()

  // Checkout with the 10% coupon: 25.00 → 22.50
  await expect(page).toHaveURL(/\/checkout/)
  await page.getByPlaceholder('Enter coupon code (optional)').fill(need('couponCode'))
  await expect(page.getByText('22.50').first()).toBeVisible()
  await page.getByText('E2E Alipay (epay)').click()
  await shot(page, 'store-13-checkout-coupon')
  const [res] = await Promise.all([
    page.waitForResponse((r) => r.request().method() === 'POST' && /\/orders\/create-and-pay$/.test(new URL(r.url()).pathname)),
    page.getByRole('button', { name: 'Place Order & Pay' }).click(),
  ])
  const created = (await res.json()) as { status_code: number; msg: string; data: { order_no?: string; order?: { order_no: string } } }
  expect(created.status_code, created.msg).toBe(0)
  const orderNo = created.data.order_no || created.data.order?.order_no || ''
  expect(orderNo).not.toBe('')
  // The customer reaches the gateway but abandons the online payment.
  await expect(page).toHaveURL(gatewayRe, { timeout: 20_000 })

  // Wallet recharge 50.00 through epay
  await page.goto(`${STOREFRONT_URL}/me/wallet`)
  await sfField(page, 'Top-up Amount').fill('50')
  await expect(sfField(page, 'Payment Channel', 'select').locator('option', { hasText: 'E2E Alipay (epay)' })).toHaveCount(1)
  await sfField(page, 'Payment Channel', 'select').selectOption({ label: 'E2E Alipay (epay)' })
  await shot(page, 'store-14-wallet-recharge')
  await page.getByRole('button', { name: 'Confirm Top-up' }).click()
  await expect(page).toHaveURL(/\/recharge-orders\//)
  const rechargeUrl = page.url()
  // open the pay link (redirect channel) and simulate the gateway notification
  await expect(page).toHaveURL(gatewayRe, { timeout: 20_000 })
  const recharge = parseEpayPayUrl(gateway.opened[gateway.opened.length - 1] ?? page.url())
  expect(Number(recharge.money)).toBe(50)
  await sendEpayCallback(recharge.outTradeNo, recharge.money, recharge.type)
  await page.goto(rechargeUrl)
  await expect(page.getByText('Credited').first()).toBeVisible({ timeout: 30_000 })
  await shot(page, 'store-15-recharge-credited')

  // Pay the pending order with the wallet balance
  gateway.stayOnPage = true
  await page.goto(`${STOREFRONT_URL}/pay?order_no=${orderNo}`)
  await expect(page.getByText(orderNo).first()).toBeVisible()
  await page.getByRole('button', { name: 'Change payment method' }).click()
  const useBalance = page.getByText('Use wallet balance first')
  await useBalance.click()
  await expect(page.getByText('Full amount will be paid from wallet balance')).toBeVisible()
  await expect(page.getByText('22.50').first()).toBeVisible()
  await shot(page, 'store-16-pay-with-balance')
  await page.getByRole('button', { name: 'Confirm payment' }).click()
  await expect(page).toHaveURL(new RegExp(`/orders/${orderNo}`), { timeout: 30_000 })
  await expect(page.getByText(new RegExp(`PRO-CARD-${need('runId')}-\\d`)).first()).toBeVisible({ timeout: 30_000 })
  await shot(page, 'store-17-member-order-delivered')
  saveState({ memberOrderNos: [...(loadState().memberOrderNos ?? []), orderNo] })
})

test('personal center pages load: orders, wallet, gift card redeem, security', async ({ page }) => {
  await login(page, need('memberEmail'), need('memberPassword'))

  await page.goto(`${STOREFRONT_URL}/me/orders`)
  for (const no of need('memberOrderNos')) await expect(page.getByText(no).first()).toBeVisible()
  await shot(page, 'store-18-me-orders')

  await page.goto(`${STOREFRONT_URL}/me/wallet`)
  await expect(page.getByText('27.50 CNY', { exact: true }).first()).toBeVisible()
  await shot(page, 'store-19-me-wallet')

  await page.goto(`${STOREFRONT_URL}/me/gift-cards`)
  await page.getByPlaceholder('Enter gift card code').fill(need('giftCardCode'))
  await page.getByRole('button', { name: 'Redeem now' }).click()
  await expect(page.getByText('Redeem successful').first()).toBeVisible()
  await shot(page, 'store-20-me-gift-card')

  await page.goto(`${STOREFRONT_URL}/me/wallet`)
  await expect(page.getByText('47.50 CNY', { exact: true }).first()).toBeVisible()

  await page.goto(`${STOREFRONT_URL}/me/security`)
  await expect(page.getByRole('heading').first()).toBeVisible()
  await shot(page, 'store-21-me-security')

  await page.goto(`${STOREFRONT_URL}/me/profile`)
  await shot(page, 'store-22-me-profile')
})
