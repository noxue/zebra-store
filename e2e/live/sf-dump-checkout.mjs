import { launch, open, go, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
await go(page, `${S}/products/qa-sf-manual`, diag); await sleep(800)
await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
console.log(await page.evaluate(() => [...document.querySelectorAll('main input, main select, main textarea')].map((e) => `${e.tagName}|${e.type}|${e.name}|${e.placeholder}|vis=${!!e.offsetParent}`).join('\n')))
await b.close()
