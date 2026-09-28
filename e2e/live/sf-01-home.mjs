// F-001 / F-015 / R-008: home page on every site, desktop + mobile.
import { launch, open, go, shot, SITES, record } from './sf-lib.mjs'
const b = await launch()
for (const [k, base] of Object.entries(SITES)) {
  for (const mobile of [false, true]) {
    const { ctx, page, diag } = await open(b, { mobile })
    const ms = await go(page, base + '/', diag)
    await page.waitForTimeout(1500)
    const title = await page.title()
    const fav = await page.evaluate(() => [...document.querySelectorAll('link[rel*=icon]')].map((l) => l.href))
    const text = (await page.locator('body').innerText()).slice(0, 600).replace(/\s+/g, ' ')
    const s1 = await shot(page, `F001-home-${k}-${mobile ? 'm' : 'd'}`)
    const s2 = await shot(page, `F001-home-${k}-${mobile ? 'm' : 'd'}-full`, true)
    record({ flow: 'F-001', site: k, mobile, ms, title, fav, text, diag, shots: [s1, s2] })
    await ctx.close()
  }
}
await b.close()
