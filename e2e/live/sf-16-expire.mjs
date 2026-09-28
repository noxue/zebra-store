// F-052: pending order after expiry time (UI).
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const [no] = process.argv.slice(2)
const b = await launch()
const { ctx, page, diag } = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(ctx)
await go(page, `${S}/orders/${no}`, diag); await sleep(1500)
const t = (await page.locator('main').innerText()).replace(/\s+/g, ' ')
record({ flow: 'F-052', now: new Date().toISOString(), t: t.slice(0, 500), diag, shots: [await shot(page, 'F052-expired-order-m', true)] })
await b.close()
