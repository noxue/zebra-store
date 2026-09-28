import { expect, type Locator, type Page } from '@playwright/test'

type Scope = Page | Locator

const xpathLiteral = (s: string) => (s.includes('"') ? `concat("${s.split('"').join('", \'"\', "')}")` : `"${s}"`)

/**
 * The form control of a `<FormField label="…">` (label + sibling wrapper div).
 * Matches the label text exactly, ignoring the required `*` marker.
 */
export function field(scope: Scope, label: string, control = 'input'): Locator {
  return fieldBox(scope, label).locator(control).first()
}

/** The wrapper div holding the control(s) of a `<FormField label="…">`. */
export function fieldBox(scope: Scope, label: string): Locator {
  const lit = xpathLiteral(label)
  return scope.locator(`xpath=.//label[normalize-space(translate(., "*", ""))=${lit}]/following-sibling::div[1]`).first()
}

/** Fills a `<LocalizedInput>` (zh-CN / zh-TW / en-US tabs) with the same text in every locale. */
export async function fillLocalized(box: Locator, text: string) {
  for (const code of ['zh-CN', 'zh-TW', 'en-US']) {
    await box.locator(`button[title="${code}"]`).click()
    await box.locator('input, textarea').first().fill(text)
  }
}

/** The currently open modal dialog (the last one when stacked). */
export function dialog(page: Page, title?: string): Locator {
  const all = page.locator('[role="dialog"]')
  return (title ? all.filter({ hasText: title }) : all).last()
}

/** Picks an option of a native <select> by its visible label. */
export async function selectOption(select: Locator, label: string | RegExp) {
  const matches = (o: string) => (typeof label === 'string' ? o.trim() === label : label.test(o))
  await expect
    .poll(async () => (await select.locator('option').allTextContents()).some(matches), { message: `option ${String(label)}` })
    .toBe(true)
  const options = await select.locator('option').allTextContents()
  const idx = options.findIndex(matches)
  if (idx < 0) throw new Error(`option ${String(label)} not found in [${options.join(', ')}]`)
  const value = await select.locator('option').nth(idx).getAttribute('value')
  await select.selectOption(value ?? String(idx))
}

/** Sets a switch (role=switch) to the wanted state. */
export async function setSwitch(sw: Locator, on: boolean) {
  const cur = (await sw.getAttribute('aria-checked')) === 'true'
  if (cur !== on) await sw.click()
  await expect(sw).toHaveAttribute('aria-checked', String(on))
}

/** Waits for the next API response matching method + path regex and returns its JSON envelope. */
export async function apiResponse<T = unknown>(page: Page, method: string, path: RegExp, action: () => Promise<unknown>) {
  const [res] = await Promise.all([
    page.waitForResponse((r) => r.request().method() === method && path.test(new URL(r.url()).pathname)),
    action(),
  ])
  const body = (await res.json()) as { status_code: number; msg: string; data: T }
  expect(body.status_code, `${method} ${path} → ${body.msg}`).toBe(0)
  return body.data
}

/** Storefront `<Field label="…">`: label (text in its first span) and control are siblings. */
export function sfField(scope: Scope, label: string, control = 'input'): Locator {
  const lit = xpathLiteral(label)
  return scope.locator(`xpath=.//label[normalize-space(./span)=${lit}]/parent::div`).locator(control).first()
}
