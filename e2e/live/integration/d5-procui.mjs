// node d5-procui.mjs <tag> <site> <localOrderNoPrefix> <action:retry|cancel|none>
import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const [tag, site, prefix, action = 'none'] = process.argv.slice(2)
const a = await admin(site)
const b = await browser(); const errs = []
const ctx = await adminCtx(b, site, a.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('/procurement-orders/') && r.request().method() === 'POST') net.push(`${r.status()} ${new URL(r.url()).pathname} ${(await r.text().catch(() => '')).slice(0, 400)}`) })
await p.goto(`https://${site}.dot2.com/admin/procurement-orders`); await sleep(2500)
await shot(p, 'integration', `${tag}-proc-list`)
const row = p.locator(`xpath=//*[contains(text(),"${prefix}")]/ancestor::div[.//button][1]`).first()
console.log('row:', (await row.innerText()).replace(/\s+/g, ' '))
console.log('row btns:', (await row.locator('button').allInnerTexts()).join(' | '))
await row.locator('h3, .font-semibold, span').first().click().catch(() => {}); await sleep(2000)
await shot(p, 'integration', `${tag}-proc-detail`, false)
console.log('detail:', (await p.locator('[role=dialog]').last().innerText().catch(() => '')).replace(/\n+/g, ' | ').slice(0, 1500))
await p.keyboard.press('Escape'); await sleep(500)
if (action !== 'none') {
  const label = action === 'retry' ? /重试/ : /取消/
  await row.getByRole('button', { name: label }).click(); await sleep(1200)
  const conf = p.getByRole('button', { name: action === 'retry' ? /^(确认|确定|重试)$/ : /^(确认|确定|确认取消)$/ }); if (await conf.count()) { await shot(p, 'integration', `${tag}-proc-${action}-confirm`, false); await conf.last().click() }
  await sleep(5000); await p.reload(); await sleep(2500)
  await shot(p, 'integration', `${tag}-proc-after-${action}`)
  console.log('row after:', (await p.locator(`xpath=//*[contains(text(),"${prefix}")]/ancestor::div[.//button][1]`).first().innerText()).replace(/\s+/g, ' '))
}
console.log(net.join('\n')); console.log('ERRS', errs)
await b.close()
