import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { test, expect, shot } from '../support/fixtures'
import { ADMIN_URL, API_URL, EPAY, RUN_ID, STOREFRONT_URL } from '../support/env'
import { saveState } from '../support/state'
import { apiResponse, dialog, field, fieldBox, fillLocalized, selectOption, setSwitch } from '../support/ui'
import { adminSession, loginAdmin } from '../support/admin'
import { Api } from '../support/api'

interface PublicConfig {
  brand: { site_name: string; site_logo: string }
  theme: { primary_color: string }
  email_verification_enabled: boolean
}

const FIXTURES = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'fixtures')
const SITE_NAME = `Zebra E2E ${RUN_ID}`
const PRIMARY = '#e0457b'

test.describe.configure({ mode: 'serial' })

test('admin login and compliance acknowledgement', async ({ page, watch }) => {
  saveState({ runId: RUN_ID })
  await loginAdmin(page)
  await shot(page, 'admin-01-dashboard-empty')

  // Payment pages are gated: before the acknowledgement the list request is refused (silently).
  watch.allow(/compliance_required .*\/admin\/(payment-channels|settings)/)
  await page.goto(`${ADMIN_URL}/payment-channels`)
  const dlg = dialog(page, 'Compliance Acknowledgement')
  const shown = await dlg.waitFor({ state: 'visible', timeout: 8_000 }).then(() => true, () => false)
  test.skip(!shown && !process.env.E2E_REQUIRE_FRESH_DB, 'compliance already acknowledged (re-run on an existing DB)')
  const boxes = dlg.locator('input')
  const phrases = ['我已阅读并理解上述合规声明提醒', '知悉相关法律风险', '并确认自行承担部署', '运营和收费行为产生的法律责任']
  for (const [i, phrase] of phrases.entries()) await boxes.nth(i).pressSequentially(phrase)
  await dlg.getByRole('button', { name: 'I have read and confirm' }).click()
  await expect(dlg).toBeHidden()
  await shot(page, 'admin-02-payment-channels')
})

test('site settings: name, logo, theme colors, open registration', async ({ page }) => {
  await adminSession(page)
  await page.goto(`${ADMIN_URL}/settings`)
  await expect(field(page, 'Site name')).toBeVisible()

  // Registration without email verification (needed by the member spec)
  const verifyRow = page.locator('div.rounded-zs', { has: page.getByText('Email Verification', { exact: true }) })
  await setSwitch(verifyRow.getByRole('switch'), false)

  await field(page, 'Site name').fill(SITE_NAME)

  // Logo: upload through the media picker dialog
  await page.getByTestId('site-logo').getByRole('button', { name: 'Select Logo' }).click()
  const picker = dialog(page, 'Upload New')
  await picker.getByRole('tab', { name: 'Upload New' }).click()
  await apiResponse(page, 'POST', /\/admin\/upload/, () => picker.locator('input[type="file"]').setInputFiles(path.join(FIXTURES, 'logo.png')))
  await picker.getByRole('button', { name: 'Confirm' }).click()
  await expect(page.getByTestId('site-logo').locator('img')).toBeVisible()

  // Theme colors live on the Template tab
  await page.getByRole('tab', { name: 'Template', exact: true }).click()
  await field(page, 'Primary (sakura pink)').fill(PRIMARY)

  await apiResponse(page, 'PUT', /\/admin\/settings/, () => page.getByTestId('settings-save').click())
  await shot(page, 'admin-03-settings-template')

  const pub = await Api.anonymous()
  const cfg = (await pub.get<PublicConfig>('public/config')).data
  expect(cfg.brand.site_name).toBe(SITE_NAME)
  expect(cfg.brand.site_logo).toMatch(/^\/uploads\//)
  expect(cfg.theme.primary_color).toBe(PRIMARY)
  expect(cfg.email_verification_enabled).toBe(false)
  await pub.dispose()
  saveState({ siteName: SITE_NAME, primaryColor: PRIMARY })
})

const CATEGORY_SLUG = `e2e-cat-${RUN_ID}`
const PRODUCT_SLUG = `e2e-card-${RUN_ID}`
const PRODUCT_TITLE = `E2E Game Card ${RUN_ID}`
const SKUS = [
  { code: `STD-${RUN_ID}`, spec: 'Standard', price: '10.00' },
  { code: `PRO-${RUN_ID}`, spec: 'Pro', price: '25.00' },
]

test('catalog: category, auto-delivery product with SKUs, card secrets', async ({ page }) => {
  await adminSession(page)

  // Category
  await page.goto(`${ADMIN_URL}/categories`)
  await page.getByRole('button', { name: 'Add category' }).click()
  let dlg = dialog(page, 'Add category')
  await fillLocalized(fieldBox(dlg, 'Name'), `E2E Cards ${RUN_ID}`)
  await field(dlg, 'Slug').fill(CATEGORY_SLUG)
  await apiResponse(page, 'POST', /\/admin\/categories$/, () => dlg.getByRole('button', { name: 'Save', exact: true }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(CATEGORY_SLUG)).toBeVisible()
  await shot(page, 'admin-04-categories')

  // Product with two SKUs, auto delivery, guest purchase allowed
  await page.goto(`${ADMIN_URL}/products`)
  await page.getByRole('button', { name: 'Add product' }).click()
  dlg = dialog(page, 'Add product')
  for (const [tab, label] of [
    ['Simplified Chinese', 'Product title (Simplified Chinese)'],
    ['Traditional Chinese', 'Product title (Traditional Chinese)'],
    ['English', 'Product title (English)'],
  ] as const) {
    await dlg.getByRole('tab', { name: tab, exact: true }).click()
    await field(dlg, label).fill(PRODUCT_TITLE)
  }
  await field(dlg, 'Slug (URL identifier)').fill(PRODUCT_SLUG)
  await selectOption(field(dlg, 'Category', 'select'), new RegExp(`E2E Cards ${RUN_ID}`))
  await selectOption(field(dlg, 'Purchase type', 'select'), 'Guest purchase')
  await selectOption(field(dlg, 'Fulfillment type', 'select'), 'Auto')
  for (const [i, sku] of SKUS.entries()) {
    await dlg.getByRole('button', { name: 'Add SKU' }).click()
    const item = dlg.locator('div.space-y-3.rounded-zs-sm', { hasText: 'SKU code' }).nth(i)
    await field(item, 'SKU code').fill(sku.code)
    await field(item, 'Spec label (English)').fill(sku.spec)
    await field(item, 'SKU price').fill(sku.price)
  }
  await apiResponse(page, 'POST', /\/admin\/products$/, () => dlg.getByRole('button', { name: 'Create now' }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(PRODUCT_TITLE)).toBeVisible()
  await shot(page, 'admin-05-products')

  // Card secrets: CSV import for the Standard SKU, pasted batch for the Pro SKU
  await page.goto(`${ADMIN_URL}/card-secret-imports`)
  const productSelect = page.locator('select').filter({ has: page.locator('option', { hasText: PRODUCT_TITLE }) })
  await selectOption(productSelect, new RegExp(PRODUCT_TITLE))
  const skuSelect = page.locator('select').filter({ has: page.locator('option', { hasText: SKUS[0].code }) })
  await selectOption(skuSelect, new RegExp(SKUS[0].code))
  const csv = ['secret', ...Array.from({ length: 5 }, (_, i) => `STD-CARD-${RUN_ID}-${i + 1}`)].join('\n')
  await page.locator('input[type="file"][accept=".csv"]').setInputFiles({ name: 'cards.csv', mimeType: 'text/csv', buffer: Buffer.from(csv) })
  await apiResponse(page, 'POST', /\/admin\/card-secrets\/import$/, () => page.getByRole('button', { name: 'Start import' }).click())

  await selectOption(skuSelect, new RegExp(SKUS[1].code))
  const secrets = Array.from({ length: 5 }, (_, i) => `PRO-CARD-${RUN_ID}-${i + 1}`).join('\n')
  await field(page, 'Secrets', 'textarea').fill(secrets)
  await apiResponse(page, 'POST', /\/admin\/card-secrets\/batch$/, () => page.getByRole('button', { name: 'Submit batch' }).click())
  await shot(page, 'admin-06-card-secret-import')

  saveState({ categorySlug: CATEGORY_SLUG, productSlug: PRODUCT_SLUG, productTitle: PRODUCT_TITLE, skuCode: SKUS[0].code })
})

test('payment channel: epay (v1, redirect)', async ({ page }) => {
  await adminSession(page)
  await page.goto(`${ADMIN_URL}/payment-channels`)
  await page.getByRole('button', { name: 'New Channel' }).click()
  const dlg = dialog(page, 'Create Channel')
  await field(dlg, 'Name').fill('E2E Alipay (epay)')
  await selectOption(field(dlg, 'Provider Type', 'select'), 'Epay')
  await selectOption(field(dlg, 'Channel Type', 'select'), 'Alipay')
  await selectOption(field(dlg, 'Interaction Mode', 'select'), 'Redirect')
  await selectOption(field(dlg, 'Version', 'select'), 'v1')
  await field(dlg, 'Gateway URL').fill(EPAY.gatewayUrl)
  await field(dlg, 'Merchant ID').fill(EPAY.merchantId)
  await field(dlg, 'Merchant Key').fill(EPAY.merchantKey)
  await field(dlg, 'Notify URL').fill(`${API_URL}/api/v1/payments/callback`)
  await field(dlg, 'Return URL').fill(`${STOREFRONT_URL}/pay`)
  await apiResponse(page, 'POST', /\/admin\/payment-channels$/, () => dlg.getByRole('button', { name: 'Save', exact: true }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText('E2E Alipay (epay)')).toBeVisible()
  await shot(page, 'admin-07-payment-channels')
})

test('content & marketing: banner, post, coupon, member level, gift cards', async ({ page }) => {
  await adminSession(page)
  const logo = path.join(FIXTURES, 'logo.png')

  // Banner on the home hero
  await page.goto(`${ADMIN_URL}/banners`)
  await page.getByRole('button', { name: 'Add banner' }).click()
  let dlg = dialog(page, 'Add banner')
  await field(dlg, 'Admin name').fill(`E2E Banner ${RUN_ID}`)
  await selectOption(field(dlg, 'Position', 'select'), 'Home hero')
  await fillLocalized(fieldBox(dlg, 'Title'), `Welcome to ${SITE_NAME}`)
  await apiResponse(page, 'POST', /\/admin\/upload/, () => fieldBox(dlg, 'Banner image').locator('input[type="file"]').setInputFiles(logo))
  await apiResponse(page, 'POST', /\/admin\/banners$/, () => dlg.getByRole('button', { name: 'Create now' }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(`E2E Banner ${RUN_ID}`).first()).toBeVisible()
  await shot(page, 'admin-08-banners')

  // Blog post
  await page.goto(`${ADMIN_URL}/posts/blog`)
  await page.getByRole('button', { name: 'New post' }).click()
  dlg = dialog(page, 'New post')
  await fillLocalized(fieldBox(dlg, 'Title'), `E2E Post ${RUN_ID}`)
  await field(dlg, 'Slug (URL identifier)').fill(`e2e-post-${RUN_ID}`)
  const content = fieldBox(dlg, 'Content')
  for (const code of ['zh-CN', 'zh-TW', 'en-US']) {
    await content.locator(`button[title="${code}"]`).click()
    await content.locator('.ProseMirror').click()
    await page.keyboard.type(`Hello from the E2E suite (${code}).`)
  }
  await apiResponse(page, 'POST', /\/admin\/posts$/, () => dlg.getByRole('button', { name: 'Publish now' }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(`E2E Post ${RUN_ID}`)).toBeVisible()

  // Coupon: 10% off the E2E product
  const couponCode = `E2E10${RUN_ID}`.toUpperCase()
  await page.goto(`${ADMIN_URL}/coupons`)
  await page.getByRole('button', { name: 'New coupon' }).click()
  dlg = dialog(page, 'New coupon')
  await field(dlg, 'Code').fill(couponCode)
  await selectOption(field(dlg, 'Type', 'select'), 'Percent')
  await field(dlg, 'Value').fill('10')
  const scope = fieldBox(dlg, 'Applicable products')
  await scope.locator('input').first().fill(RUN_ID)
  await scope.getByRole('button', { name: 'Search products' }).click()
  await scope.getByText(new RegExp(`E2E Game Card ${RUN_ID}`)).click()
  await expect(scope.getByText('1 product(s) selected')).toBeVisible()
  await apiResponse(page, 'POST', /\/admin\/coupons$/, () => dlg.getByRole('button', { name: 'Save', exact: true }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(couponCode)).toBeVisible()
  await shot(page, 'admin-09-coupons')

  // Member level
  await page.goto(`${ADMIN_URL}/member-levels`)
  await page.getByRole('button', { name: 'Create Level' }).click()
  dlg = dialog(page, 'Create Member Level')
  await fillLocalized(fieldBox(dlg, 'Level Name'), `E2E VIP ${RUN_ID}`)
  await field(dlg, 'Level Slug').fill(`e2e-vip-${RUN_ID}`)
  await field(dlg, 'Discount Rate').fill('95')
  await field(dlg, 'Recharge Threshold').fill('100000')
  await apiResponse(page, 'POST', /\/admin\/member-levels$/, () => dlg.getByRole('button', { name: 'Save', exact: true }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(`E2E VIP ${RUN_ID}`)).toBeVisible()

  // Gift cards
  await page.goto(`${ADMIN_URL}/gift-cards`)
  await page.getByRole('button', { name: 'Generate Gift Cards' }).click()
  dlg = dialog(page, 'Generate Gift Cards')
  await field(dlg, 'Name').fill(`E2E Gift ${RUN_ID}`)
  await field(dlg, 'Quantity').fill('2')
  await field(dlg, 'Amount').fill('20.00')
  await apiResponse(page, 'POST', /\/admin\/gift-cards\/generate$/, () => dlg.getByRole('button', { name: 'Confirm', exact: true }).click())
  await expect(dlg).toBeHidden()
  await expect(page.getByText(`E2E Gift ${RUN_ID}`).first()).toBeVisible()
  await shot(page, 'admin-10-gift-cards')

  const api = await Api.admin()
  const cards = (await api.get<Array<{ code: string; name: string; status: string }>>('admin/gift-cards', { page: 1, page_size: 20 })).data
  const card = cards.find((c) => c.name === `E2E Gift ${RUN_ID}` && c.status === 'active')
  expect(card, 'generated gift card').toBeTruthy()
  await api.dispose()
  saveState({ couponCode, giftCardCode: card?.code })
})
