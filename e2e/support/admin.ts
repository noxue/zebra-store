import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, type Page } from '@playwright/test'
import { ADMIN_PASSWORD, ADMIN_URL, ADMIN_USERNAME } from './env'
import { field } from './ui'

const SESSION_FILE = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '.state', 'admin-session.json')
const SESSION_KEYS = ['admin_token', 'admin_is_super', 'admin_roles', 'admin_permissions']

/** Signs in through the admin login form and remembers the session for later tests. */
export async function loginAdmin(page: Page) {
  await page.goto(`${ADMIN_URL}/login`)
  await field(page, 'Username').fill(ADMIN_USERNAME)
  await field(page, 'Password').fill(ADMIN_PASSWORD)
  await page.getByRole('button', { name: 'Sign In' }).click()
  await expect(page).toHaveURL(`${ADMIN_URL}/`)
  const session = await page.evaluate((keys) => Object.fromEntries(keys.map((k) => [k, localStorage.getItem(k)])), SESSION_KEYS)
  fs.mkdirSync(path.dirname(SESSION_FILE), { recursive: true })
  fs.writeFileSync(SESSION_FILE, JSON.stringify(session))
}

/** Reuses the session of an earlier UI login (falls back to logging in). */
export async function adminSession(page: Page) {
  if (!fs.existsSync(SESSION_FILE)) return loginAdmin(page)
  const session = JSON.parse(fs.readFileSync(SESSION_FILE, 'utf8')) as Record<string, string | null>
  await page.context().addInitScript((s) => {
    for (const [k, v] of Object.entries(s)) if (v !== null && !localStorage.getItem(k)) localStorage.setItem(k, v)
  }, session)
}
