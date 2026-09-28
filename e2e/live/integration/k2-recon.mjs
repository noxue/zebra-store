import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const a = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', a.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('/reconciliation') && r.request().method() !== 'GET') net.push(`${r.request().method()} ${new URL(r.url()).pathname} ${r.status()} ${(await r.text().catch(() => '')).slice(0, 400)}`) })
await p.goto('https://store.dot2.com/admin/reconciliation'); await sleep(3000)
await shot(p, 'integration', 'I045-01-recon-page')
console.log((await p.locator('main').innerText()).slice(0, 800))
console.log('btns', (await p.locator('main button:visible').allInnerTexts()).join(' | '))
await p.getByRole('button', { name: /新建|创建|运行|发起/ }).first().click(); await sleep(1500)
const dlg = p.locator('[role=dialog]').last()
console.log('dialog', (await dlg.innerText()).slice(0, 800))
console.log('selects', await dlg.locator('select').evaluateAll((els) => els.map((e) => [...e.options].map((o) => o.text).join(','))))
const sel = dlg.locator('select'); const n = await sel.count()
for (let i = 0; i < n; i++) { const o = await sel.nth(i).evaluate((e) => [...e.options].map((x) => x.text)); const pick = o.find((x) => /QA 异次元/.test(x)) || o.find((x) => /全量|全部|full/i.test(x)); if (pick) await sel.nth(i).selectOption({ label: pick }) }
const dts = dlg.locator('input[type=datetime-local], input[type=date]'); console.log('dt inputs', await dts.count())
await dts.nth(0).fill('2026-09-26T00:00'); await dts.nth(1).fill('2026-09-26T23:59')
await shot(p, 'integration', 'I045-02-new-job', false)
await dlg.getByRole('button', { name: /^(创建|确定|确认|运行|开始)/ }).last().click(); await sleep(8000)
await p.reload(); await sleep(3000)
await shot(p, 'integration', 'I045-03-jobs')
console.log((await p.locator('main').innerText()).slice(0, 1500))
note('I-045', net.join(' || ')); console.log('ERRS', errs); await b.close()
