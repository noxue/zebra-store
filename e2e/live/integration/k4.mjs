import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const a = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', a.token); const p = await ctx.newPage(); watch(p, errs)
p.on('request', (r) => { if (r.method() !== 'GET') console.log('REQ', r.method(), r.url()) })
await p.goto('https://store.dot2.com/admin/reconciliation'); await sleep(3000)
await p.getByRole('button', { name: /对账详情/ }).first().click(); await sleep(2500)
await p.getByRole('button', { name: '标记处理' }).click(); await sleep(2000)
console.log('visible btns', (await p.locator('button:visible').allInnerTexts()).slice(-6))
console.log('dialogs', await p.locator('[role=dialog]').count(), (await p.locator('[role=dialog]').last().innerText()).slice(0, 300))
await shot(p, 'integration', 'I045-05-after-click', false)
const ta = p.locator('[role=dialog] textarea:visible'); if (await ta.count()) { await ta.first().fill('QA: 上游已人工发货'); const ok = p.locator('[role=dialog] button:visible', { hasText: /确认|确定|提交|标记处理/ }); console.log('ok', await ok.allInnerTexts()); await ok.last().click(); await sleep(2500) }
await shot(p, 'integration', 'I045-06-resolved', false)
const j = await a.get('/admin/reconciliation/jobs/1'); note('I-045', 'item resolved=' + j.items[0].resolved + ' ' + JSON.stringify(j.items[0]).slice(0, 200))
console.log('ERRS', errs); await b.close()
