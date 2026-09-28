import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const a = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', a.token); const p = await ctx.newPage(); watch(p, errs)
await p.goto('https://store.dot2.com/admin/products'); await sleep(3000)
const si = p.locator('input[placeholder*="搜索"]:visible').last(); await si.fill('QA ACG 自动卡'); await si.press('Enter'); await sleep(3000)
console.log((await p.locator('main').innerText()).slice(0, 600))
const row = p.locator('tr', { hasText: 'QA ACG 自动卡' }).first()
console.log(await row.locator('button').allInnerTexts())
await row.locator('button').filter({ hasText: /编辑/ }).first().click(); await sleep(2500)
await shot(p, 'integration', 'I078-01-edit-mapped-product', true)
const t = await p.locator('[role=dialog]').last().innerText().catch(() => p.locator('main').innerText())
note('I-078', 'edit dialog excerpt: ' + t.replace(/\s+/g, ' ').match(/.{0,80}(发货|交付|上游).{0,120}/g)?.slice(0, 4).join(' ... '))
const dis = await p.locator('[role=dialog] select:disabled, [role=dialog] input:disabled').count(); note('I-078', 'disabled controls in dialog: ' + dis)
console.log('ERRS', errs); await b.close()
