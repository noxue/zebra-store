/** Deployment URLs (see playwright.config.ts). */
export const ADMIN_URL = (process.env.E2E_ADMIN_URL || 'http://localhost:5186').replace(/\/$/, '')
export const STOREFRONT_URL = (process.env.E2E_STOREFRONT_URL || 'http://localhost:5185').replace(/\/$/, '')
export const API_URL = (process.env.E2E_API_URL || 'http://localhost:8082').replace(/\/$/, '')

export const ADMIN_USERNAME = process.env.E2E_ADMIN_USERNAME || 'admin'
export const ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD || 'Admin12345'

/** Unique per run so the suite can be re-run against the same database. */
export const RUN_ID = process.env.E2E_RUN_ID || Date.now().toString(36)

/** epay (v1 / MD5) test merchant configured by the admin spec; callbacks are signed with it. */
export const EPAY = {
  gatewayUrl: 'https://epay.e2e.invalid',
  merchantId: '10086',
  merchantKey: 'e2e-epay-merchant-key-0123456789',
}
