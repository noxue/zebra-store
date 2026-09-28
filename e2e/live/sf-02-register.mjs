// F-020 register through the UI (desktop + mobile), check landing page and default member level.
import { launch, open, go, shot, record, adm, api, QAPW } from './sf-lib.mjs'
const b = await launch()
const run = Date.now().toString(36)
for (const mobile of [false, true]) {
  const email = `qa-sf-${mobile ? 'm' : 'd'}${run}@lab.test`
  const { ctx, page, diag } = await open(b, { mobile })
  await page.addInitScript(() => localStorage.setItem('announcement_dismiss', JSON.stringify({ mode: 'forever', version: '*' })))
  await go(page, 'https://store.dot2.com/auth/register', diag)
  await page.fill('input[type=email]', email)
  await page.fill('input[type=password]', QAPW)
  // submit without agreeing first
  const disabledBeforeAgree = await page.locator('button[type=submit]').isDisabled()
  const noAgree = await shot(page, `F020-register-noagree-${mobile ? 'm' : 'd'}`)
  const msgNoAgree = (await page.locator('body').innerText()).match(/.*(协议|同意|条款).*/g)?.slice(0, 3)
  await page.locator('label:has(input[type=checkbox])').first().click({ position: { x: 8, y: 8 } })
  await shot(page, `F020-register-filled-${mobile ? 'm' : 'd'}`)
  await page.click('button[type=submit]')
  await page.waitForTimeout(3000)
  const url = page.url()
  const s = await shot(page, `F020-register-after-${mobile ? 'm' : 'd'}`, true)
  const tok = await page.evaluate(() => localStorage.getItem('user_token'))
  const me = tok ? await api('me', { token: tok }) : null
  record({ flow: 'F-020', mobile, email, url, disabledBeforeAgree, me: me?.data, diag, shots: [noAgree, s] })
  await ctx.storageState({ path: `.state/sf-${mobile ? 'm' : 'd'}.json` })
  await ctx.close()
}
await b.close()
