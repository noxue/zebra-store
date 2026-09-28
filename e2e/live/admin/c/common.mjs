export { apiResponse, dialog, field, fieldBox, fillLocalized, selectOption, setSwitch } from '../../../support/ui.ts'
export const TS = process.env.QA_TS || '0926'
export const out = []
export const rec = (id, verdict, note) => { out.push({ id, verdict, note }); console.log('RESULT', id, verdict, note) }
export async function confirmDialog(page, btn = /Confirm|OK|Delete|确定|确认/) {
  const d = page.locator('[role="dialog"], [role="alertdialog"]').last()
  await d.getByRole('button', { name: btn }).last().click()
}
export async function toast(page) {
  await page.waitForTimeout(600)
  return (await page.locator('[role=status], [role=alert], .zs-toast, [class*=toast]').allInnerTexts().catch(() => [])).join(' | ').slice(0, 300)
}
