import { TS } from './common.mjs'
export default async ({ page, shot, log, admin }) => {
  await page.goto(`${admin}/media`)
  await page.waitForLoadState('networkidle')
  const hdr = page.waitForRequest((r) => r.url().includes('/admin/upload'))
  await page.getByTestId('media-upload').setInputFiles({ name: 'evil.php', mimeType: 'application/x-php', buffer: Buffer.from('<?php echo 1;') })
  const req = await hdr
  log('upload req accept-language', req.headers()['accept-language'], 'x-locale', req.headers()['x-locale'])
  await page.waitForTimeout(1500)
  log('php ui msg', (await page.locator('[role=status]').allInnerTexts()).join(' | '))
  await shot(page, 'c-media-php')
  // rename to empty via UI
  const card = page.getByTitle('Click to edit name').filter({ hasText: `qa-media-${TS}-renamed` }).first()
  await card.click()
  const inp = page.locator('input:focus')
  await inp.fill('')
  await inp.press('Enter')
  await page.waitForTimeout(1500)
  log('rename empty ui msg', (await page.locator('[role=status]').allInnerTexts()).join(' | '))
  await shot(page, 'c-media-rename-empty')
}
