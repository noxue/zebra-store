// F-050 manual form (front + back validation), manual order paid by wallet.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, userToken } from './sf-lib.mjs'
const b = await launch()
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
const { ctx, page, diag } = await open(b, { storage: '.state/sf-d.json' }); await noAnnouncement(ctx)
await go(page, `${S}/products/qa-sf-manual`, diag); await sleep(1000)
const sDetail = await shot(page, 'F050-manual-detail-d', true)
await page.getByRole('button', { name: /立即购买/ }).first().click(); await page.waitForURL(/checkout/); await sleep(2500)
const s0 = await shot(page, 'F050-manual-checkout-form-d', true)
const formText = (await txt(page)).match(/.{0,40}充值账号.{0,200}/)?.[0]
await page.getByText('优先使用余额支付').click(); await sleep(1200)
const btn = page.getByRole('button', { name: /提交订单/ })
const steps = {}
await btn.click().catch(() => {}); await sleep(1500)
steps.empty = { toast: (await toastsOf(page)).join('/'), url: page.url(), inline: (await txt(page)).match(/.{0,15}(必填|请填写|不能为空|请输入).{0,20}/g)?.slice(0, 5), shot: await shot(page, 'F050-empty-submit-d', true) }
const acc = page.locator('main input[placeholder*="邮箱账号"]').first()
await acc.fill('not-an-email')
await page.locator('main select').last().selectOption({ label: '美服' }).catch(async () => { await page.getByText('美服').click().catch(() => {}) })
await page.locator('main input[type=text]').nth(0).fill('abc')
await btn.click().catch(() => {}); await sleep(1500)
steps.badEmail = { toast: (await toastsOf(page)).join('/'), url: page.url(), inline: (await txt(page)).match(/.{0,15}(格式|有效|不正确|邮箱).{0,20}/g)?.slice(0, 6), shot: await shot(page, 'F050-bad-email-d', true) }
await acc.fill('player@lab.test')
await page.locator('main input[type=text]').nth(0).fill('abc')
await btn.click().catch(() => {}); await sleep(1500)
steps.badRegex = { toast: (await toastsOf(page)).join('/'), url: page.url(), shot: await shot(page, 'F050-bad-regex-d', true) }
await page.locator('main input[type=text]').nth(0).fill('12345')
await sleep(500)
await btn.click().catch(() => {}); await sleep(5000)
steps.ok = { url: page.url(), t: (await txt(page)).slice(0, 2200), shot: await shot(page, 'F050-manual-order-detail-d', true) }
record({ flow: 'F-050', formText, steps, diag, shots: [sDetail, s0] })
if (0) {
// backend validation bypass
const tok = await userToken('qa-sf-mmuh9ewag@lab.test')
const fx = JSON.parse((await import('node:fs')).readFileSync('.state/sf-fixtures.json', 'utf8'))
const noForm = await api('orders/preview', { method: 'POST', token: tok, body: { items: [{ product_id: fx.manual, sku_id: fx.manualSku, quantity: 1 }] } })
const bad = await api('orders/create-and-pay', { method: 'POST', token: tok, body: { items: [{ product_id: fx.manual, sku_id: fx.manualSku, quantity: 1 }], manual_form_data: { [fx.manual]: { account: 'x', region: '火星' } }, use_balance: true } })
const bad2 = await api('orders/create-and-pay', { method: 'POST', token: tok, body: { items: [{ product_id: fx.manual, sku_id: fx.manualSku, quantity: 1 }], use_balance: true } })
record({ flow: 'F-050-api', preview: [noForm.status_code, noForm.msg], badForm: [bad.status_code, bad.msg, bad.data?.order_no], missing: [bad2.status_code, bad2.msg, bad2.data?.order_no] })
}
await b.close()
