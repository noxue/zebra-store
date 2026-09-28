// node g-import.mjs <site> <tag> <connName> <categoryText> <title1> [title2...]
import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const [site, tag, connName, catText, ...titles] = process.argv.slice(2)
const a = await admin(site); const b = await browser(); const errs = []
const ctx = await adminCtx(b, site, a.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('product-mappings/') && r.request().method() === 'POST') net.push(`${r.status()} ${new URL(r.url()).pathname} ${(await r.text().catch(() => '')).slice(0, 600)}`) })
await p.goto(`https://${site}.dot2.com/admin/product-mappings`); await sleep(2500)
await p.getByRole('button', { name: /从上游导入/ }).first().click(); await sleep(1500)
const dlg = p.locator('[role=dialog]').last()
await dlg.locator('select').first().selectOption({ label: connName }); await sleep(4000)
await dlg.locator('span, div', { hasText: new RegExp('^' + catText) }).locator('visible=true').last().click(); await sleep(3000)
const cat = dlg.locator('select').nth(1); const opts = await cat.evaluate((e) => [...e.options].map((o) => o.text)); console.log('local cats', opts)
const pick = opts.find((o) => /联调|lab|Lab 游戏|独角/.test(o)); if (pick) await cat.selectOption({ label: pick })
await shot(p, 'integration', `${tag}-import-list`, false)
console.log((await dlg.innerText()).slice(0, 1800))
for (const t of titles) { await dlg.getByText(t).first().click(); await sleep(400) }
await shot(p, 'integration', `${tag}-import-selected`, false)
await dlg.getByRole('button', { name: /导入选中/ }).click(); await sleep(6000)
await shot(p, 'integration', `${tag}-import-done`)
note(tag, net.join(' || ')); console.log('ERRS', errs); await b.close()
