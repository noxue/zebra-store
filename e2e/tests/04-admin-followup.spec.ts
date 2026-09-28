import type { Page } from '@playwright/test'
import { test, expect, shot } from '../support/fixtures'
import { ADMIN_URL } from '../support/env'
import { adminSession } from '../support/admin'
import { need } from '../support/state'
import { Api } from '../support/api'
import { apiResponse, dialog } from '../support/ui'

test.describe.configure({ mode: 'serial' })

/** Numeric value of a dashboard KPI card. */
async function kpi(page: Page, label: string): Promise<number> {
  const card = page.locator('div.rounded-zs-lg', { has: page.locator('p', { hasText: new RegExp(`^${label}$`) }) }).first()
  const text = (await card.locator('p.zs-num').first().textContent()) ?? ''
  return Number(text.replace(/[^\d.-]/g, ''))
}

test('order list and detail show the storefront orders', async ({ page }) => {
  await adminSession(page)
  await page.goto(`${ADMIN_URL}/orders`)
  const guestOrder = need('guestOrderNo')
  const memberOrder = need('memberOrderNos')[0]!
  await expect(page.getByText(guestOrder).first()).toBeVisible()
  await expect(page.getByText(memberOrder).first()).toBeVisible()
  await shot(page, 'admin-20-orders')

  const row = page.locator('tr', { hasText: guestOrder })
  await row.getByRole('button', { name: 'View' }).click()
  const dlg = dialog(page, 'Order Detail')
  await expect(dlg.getByText(guestOrder).first()).toBeVisible()
  await expect(dlg.getByText(need('guestEmail')).first()).toBeVisible()
  await expect(dlg.getByText(/STD-CARD-/).first()).toBeVisible()
  await shot(page, 'admin-21-order-detail-guest')
})

test('refund the member order to the wallet', async ({ page }) => {
  await adminSession(page)
  const memberOrder = need('memberOrderNos')[0]!
  const api = await Api.admin()
  const before = await memberBalance(api)

  await page.goto(`${ADMIN_URL}/orders`)
  await page.locator('tr', { hasText: memberOrder }).getByRole('button', { name: 'View' }).click()
  const dlg = dialog(page, 'Order Detail')
  await expect(dlg.getByText('Refund Processing')).toBeVisible()
  await dlg.getByRole('tab', { name: 'Refund to Wallet' }).click()
  await dlg.getByPlaceholder('Enter refund amount').fill('5.00')
  await apiResponse(page, 'POST', /\/admin\/orders\/\d+\/refund-to-wallet$/, () => dlg.getByRole('button', { name: 'Confirm Refund' }).click())
  await expect(dlg.getByText('Refund completed and returned to wallet')).toBeVisible()
  await shot(page, 'admin-22-order-refund')

  expect(await memberBalance(api)).toBeCloseTo(before + 5, 2)
  await page.goto(`${ADMIN_URL}/order-refunds`)
  const refundRow = page.locator('tr', { hasText: need('memberEmail') })
  await expect(refundRow.getByText('Wallet Refund').first()).toBeVisible()
  await expect(refundRow.getByText('5.00 CNY').first()).toBeVisible()
  await shot(page, 'admin-23-order-refunds')
  await api.dispose()
})

test('dashboard numbers reflect the activity', async ({ page }) => {
  await adminSession(page)
  await page.goto(`${ADMIN_URL}/`)
  await expect(page.getByText('Total Orders').first()).toBeVisible()
  await expect.poll(() => kpi(page, 'Total Orders')).toBeGreaterThanOrEqual(2)
  expect(await kpi(page, 'Paid GMV')).toBeGreaterThan(0)
  expect(await kpi(page, 'Successful Payments')).toBeGreaterThan(0)
  expect(await kpi(page, 'New Users')).toBeGreaterThan(0)
  expect(await kpi(page, 'Total User Balance')).toBeGreaterThan(0)
  await shot(page, 'admin-24-dashboard')
})

async function memberBalance(api: Api): Promise<number> {
  const users = (await api.get<Array<{ id: number; email: string }>>('admin/users', { keyword: need('memberEmail'), page: 1, page_size: 20 })).data
  const user = users.find((u) => u.email === need('memberEmail'))
  expect(user, 'member user').toBeTruthy()
  const wallet = (await api.get<{ account?: { balance: string }; balance?: string }>(`admin/users/${user!.id}/wallet`)).data
  return Number(wallet.account?.balance ?? wallet.balance ?? 'NaN')
}
