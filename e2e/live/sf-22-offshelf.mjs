// F-008 list view of sold-out product; off-shelf (my QA product) → list hidden + direct URL.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, adm } from './sf-lib.mjs'
import fs from 'node:fs'
const fx = JSON.parse(fs.readFileSync('.state/sf-fixtures.json', 'utf8'))
const b = await launch()
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
const body = async () => (await page.locator('body').innerText()).replace(/\s+/g, ' ')
await go(page, `${S}/categories/lab`, diag); await sleep(1500)
const listSold = (await body()).match(/QA-SF 最后一张卡.{0,80}/)?.[0]
const s1 = await shot(page, 'F008-soldout-list-d', true)
const off = await adm(`products/${fx.auto}`, { method: 'PATCH', body: { is_active: false } })
await go(page, `${S}/categories/lab`, diag); await sleep(1500)
const listAfter = /QA-SF 最后一张卡/.test(await body())
await go(page, `${S}/products/qa-sf-lastcard`, diag); await sleep(1500)
const direct = (await body()).slice(0, 400)
const s2 = await shot(page, 'F008-offshelf-direct-d', true)
record({ flow: 'F-008', listSold, off: [off.status_code, off.msg], listAfterOff: listAfter, direct, diag, shots: [s1, s2] })
await b.close()
