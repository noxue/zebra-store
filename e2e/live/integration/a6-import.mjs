import { admin, browser, adminCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const z = await admin('zs2')
const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'zs2', z.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('product-mappings/') && r.request().method() === 'POST') net.push(`${r.status()} ${new URL(r.url()).pathname} ${(await r.text().catch(() => '')).slice(0, 600)}`) })
await p.goto('https://zs2.dot2.com/admin/product-mappings'); await sleep(2500)
await p.getByRole('button', { name: /从上游导入/ }).first().click(); await sleep(1500)
await p.locator('[role=dialog] select').first().selectOption({ label: 'QA 主站 (zebra-store)' }); await sleep(3500)
const dlg = p.locator('[role=dialog]').last()
await dlg.getByText('联调测试').first().click(); await sleep(2500)
console.log('dialog', (await dlg.innerText()).slice(0, 3000))
await shot(p, 'integration', 'I064-02-upstream-list', false)
for (const t of ['QA 对接卡 A', 'QA 对接卡 B']) {
  const el = dlg.getByText(t).first()
  console.log(await el.evaluate((e) => { let x = e; for (let i = 0; i < 4; i++) x = x.parentElement; return x.outerHTML.slice(0, 900) }))
  await el.click(); await sleep(400)
}
await sleep(500)
console.log('selects', await dlg.locator('select').evaluateAll((els) => els.map((e) => [...e.options].map((o) => o.value + ':' + o.text).join(','))))
await shot(p, 'integration', 'I064-03-selected', false)
await dlg.getByRole('button', { name: /导入选中/ }).click(); await sleep(5000)
await shot(p, 'integration', 'I064-04-imported')
console.log(net.join('\n')); console.log('ERRS', errs)
await b.close()
