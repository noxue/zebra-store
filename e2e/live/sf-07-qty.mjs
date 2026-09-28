// F-040 typed quantity beyond max purchase quantity.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
await go(page, S + '/cart', diag); await sleep(1000)
const q = page.locator('input').filter({ hasNot: page.locator('[type=checkbox]') }).nth(0)
const inputs = page.locator('main input:visible')
await inputs.first().click(); await inputs.first().fill('99'); await page.keyboard.press('Tab'); await sleep(1200)
const v = await inputs.first().inputValue()
const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('cart_items')).map((i) => i.quantity))
const total = (await page.locator('body').innerText()).match(/合计\s*([\d.]+)/)?.[1]
const s = await shot(page, 'F040-qty99-d')
console.log((await page.locator('body').innerText()).slice(0,800)); await page.getByText('去结算').first().click(); await sleep(2500)
const s2 = await shot(page, 'F040-qty99-checkout-d', true)
const t = (await page.locator('body').innerText()).replace(/\s+/g, ' ').slice(0, 1200)
record({ flow: 'F-040', typed99: v, stored, total, url: page.url(), checkoutText: t, diag, shots: [s, s2] })
await b.close()
