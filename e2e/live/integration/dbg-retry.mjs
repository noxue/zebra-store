import { admin, browser, adminCtx, shot, watch, sleep } from './lib.mjs'
const [site, prefix] = process.argv.slice(2)
const a = await admin(site); const b = await browser(); const errs = []
const ctx = await adminCtx(b, site, a.token); const p = await ctx.newPage(); watch(p, errs)
p.on('request', (r) => { if (r.method() !== 'GET') console.log('REQ', r.method(), r.url()) })
await p.goto(`https://${site}.dot2.com/admin/procurement-orders`); await sleep(2500)
const row = p.locator(`xpath=//*[contains(text(),"${prefix}")]/ancestor::div[.//button][1]`).first()
const btn = row.getByRole('button', { name: /重试/ }); console.log('btn count', await btn.count())
await btn.first().click(); await sleep(1500)
console.log('visible btns', (await p.locator('button:visible').allInnerTexts()).slice(-8))
await shot(p, 'integration', 'dbg-retry', false)
const c = p.getByRole('button', { name: /确认|确定|重试/ }); console.log('cands', await c.allInnerTexts())
await c.last().click(); await sleep(4000)
console.log('ERRS', errs); await b.close()
