// F-074 manual delivery (SA API) → buyer UI; F-075 instructions; F-080/F-081/F-082 refunds → buyer UI.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, userToken } from './sf-lib.mjs'
const MANUAL = 'DJ20260926024658654693', MULTI = 'DJ20260926022209006712', SINGLE = 'DJ20260926021352561095'
const find = async (no) => (await adm(`orders?page=1&page_size=5&order_no=${no}`)).data?.[0]
const det = async (no) => (await adm(`orders/${(await find(no)).id}`)).data
const tok = await userToken('qa-sf-dmuh9ewag@lab.test')
const bal = async () => (await api('wallet', { token: tok })).data?.balance
const R = { bal0: await bal() }
// F-074
const m = await det(MANUAL)
const child = (m.children || [])[0]
R.fulfill = await adm('fulfillments', { method: 'POST', body: { order_id: child?.id ?? m.id, payload: 'QA-SF 已为 player@lab.test 充值完成\n凭证号 QA-0001' } }).then((r) => [r.status_code, r.msg])
// F-080 refund 5.00 to wallet on MULTI
const mu = await find(MULTI)
R.refundWallet = await adm(`orders/${mu.id}/refund-to-wallet`, { method: 'POST', body: { amount: '5.00', remark: 'QA-SF partial refund' } }).then((r) => [r.status_code, r.msg, r.data?.order?.status])
R.bal1 = await bal()
// F-081 manual refund 1.00 on SINGLE
const si = await find(SINGLE)
R.manualRefund = await adm(`orders/${si.id}/manual-refund`, { method: 'POST', body: { amount: '1.00', remark: 'QA-SF offline refund', payment_fee_refunded: true } }).then((r) => [r.status_code, r.msg, r.data?.order?.status])
R.bal2 = await bal()
// F-082 concurrent full refunds on SINGLE remaining (20.52-1.00=19.52)
const par = await Promise.all([1, 2, 3].map(() => adm(`orders/${si.id}/refund-to-wallet`, { method: 'POST', body: { amount: '19.52', remark: 'QA-SF concurrent' } })))
R.concurrent = par.map((r) => [r.status_code, r.msg])
R.bal3 = await bal()
// over-refund
R.overRefund = await adm(`orders/${si.id}/refund-to-wallet`, { method: 'POST', body: { amount: '0.01', remark: 'QA-SF over' } }).then((r) => [r.status_code, r.msg])
record({ flow: 'F-074/F-080/F-081/F-082-api', R })
const b = await launch()
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
const txt = async (p) => (await p.locator('main').innerText()).replace(/\s+/g, ' ')
const out = {}
for (const [k, no] of [['manual', MANUAL], ['multi', MULTI], ['single', SINGLE]]) {
  await go(page, `${S}/orders/${no}`, diag); await sleep(1800)
  out[k] = { t: (await txt(page)).slice(0, 2600), shot: await shot(page, `F07x-08x-order-${k}-d`, true) }
}
await go(page, `${S}/me/wallet`, diag); await sleep(1500)
out.wallet = { t: (await txt(page)).match(/钱包明细.{0,900}/)?.[0], shot: await shot(page, 'F080-wallet-after-refunds-d', true) }
record({ flow: 'F-074/F-075/F-080..082-ui', out, diag })
await b.close()
