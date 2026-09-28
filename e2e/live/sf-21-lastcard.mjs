// F-056 last card race via two browsers; F-008 sold-out view; F-075 instructions on auto order.
import { launch, open, go, shot, record, S, sleep, noAnnouncement } from './sf-lib.mjs'
const b = await launch()
const prep = async (m, mobile) => {
  const o = await open(b, { mobile, storage: `.state/sf-${m}.json` }); await noAnnouncement(o.ctx)
  await go(o.page, `${S}/products/qa-sf-lastcard`, o.diag); await sleep(800)
  await o.page.getByRole('button', { name: /立即购买/ }).first().click(); await o.page.waitForURL(/checkout/); await sleep(2500)
  await o.page.getByText('优先使用余额支付').click(); await sleep(1500)
  return o
}
const A = await prep('d', false), B = await prep('m', true)
await Promise.all([A.page.getByRole('button', { name: /提交订单/ }).click(), B.page.getByRole('button', { name: /提交订单/ }).click()])
await sleep(6000)
const res = {}
for (const [k, o] of [['A', A], ['B', B]]) {
  res[k] = { url: o.page.url(), toast: (await o.page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/'), t: (await o.page.locator('main').innerText()).replace(/\s+/g, ' ').slice(0, 900), shot: await shot(o.page, `F056-race-${k}`, true) }
}
record({ flow: 'F-056', res, diagA: A.diag, diagB: B.diag })
// sold-out view
await go(A.page, `${S}/products/qa-sf-lastcard`, A.diag); await sleep(1500)
const t = (await A.page.locator('main').innerText()).replace(/\s+/g, ' ')
const buyDisabled = await A.page.getByRole('button', { name: /立即购买|售罄|缺货/ }).first().isDisabled().catch(() => 'n/a')
record({ flow: 'F-008', t: t.slice(0, 600), buyDisabled, shots: [await shot(A.page, 'F008-soldout-detail-d', true)] })
await go(A.page, `${S}/categories/lab`, A.diag); await sleep(1500)
record({ flow: 'F-008', list: (await A.page.locator('main').innerText()).replace(/\s+/g, ' ').match(/QA-SF 最后一张卡.{0,80}/)?.[0], shots: [await shot(A.page, 'F008-soldout-list-d', true)] })
await b.close()
