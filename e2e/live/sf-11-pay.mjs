// F-060/F-064: pay page for a pending order when the site has no online channel.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const [orderNo] = process.argv.slice(2)
const b = await launch()
const { ctx, page, diag } = await open(b, { mobile: true, storage: '.state/sf-m.json' }); await noAnnouncement(ctx)
await go(page, `${S}/orders/${orderNo}`, diag); await sleep(1500)
await page.getByText('去支付', { exact: true }).first().click(); await sleep(3000)
const t = (await page.locator('body').innerText()).replace(/\s+/g, ' ')
record({ flow: 'F-060', url: page.url(), t: t.slice(0, 1500), diag, shots: [await shot(page, 'F060-pay-page-m', true)] })
await b.close()
