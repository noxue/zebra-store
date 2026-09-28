import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const a = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', a.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('/reconciliation') && r.request().method() !== 'GET') net.push(`${r.request().method()} ${new URL(r.url()).pathname} ${r.status()} ${(await r.text().catch(() => '')).slice(0, 300)}`) })
await p.goto('https://store.dot2.com/admin/reconciliation'); await sleep(3000)
await p.getByRole('button', { name: /对账详情/ }).first().click(); await sleep(2500)
await shot(p, 'integration', 'I045-04-detail', false)
console.log((await p.locator('[role=dialog]').last().innerText()).slice(0, 1200))
const btn = p.getByRole('button', { name: /标记|处理|解决/ }); console.log('resolve btns', await btn.allInnerTexts())
if (await btn.count()) { await btn.first().click(); await sleep(1200); const c = p.locator('[role=dialog]').last(); const ta = c.locator('textarea, input[type=text]'); if (await ta.count()) await ta.first().fill('QA: ACG 人工发货已在上游完成'); await shot(p, 'integration', 'I045-05-resolve-dialog', false); const ok = p.getByRole('button', { name: /^(确认|确定|提交|标记已处理)$/ }); if (await ok.count()) await ok.last().click(); await sleep(2500) }
await shot(p, 'integration', 'I045-06-resolved', false)
note('I-045', net.join(' || ')); console.log('ERRS', errs); await b.close()
