// node h2-syncui.mjs <tag> <title> [site]
import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const [tag, title, site = 'store'] = process.argv.slice(2)
const a = await admin(site); const b = await browser(); const errs = []
const ctx = await adminCtx(b, site, a.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('product-mappings') && r.request().method() === 'POST') net.push(`${r.status()} ${new URL(r.url()).pathname} ${(await r.text().catch(() => '')).slice(0, 400)}`) })
await p.goto(`https://${site}.dot2.com/admin/product-mappings`); await sleep(3000)
const row = p.locator(`xpath=//*[normalize-space(text())="${title}"]/ancestor::div[.//button[normalize-space()="同步"]][1]`).first()
console.log('before:', (await row.innerText()).replace(/\s+/g, ' '))
await row.getByRole('button', { name: /^同步$/ }).click(); await sleep(1000)
const c = p.getByRole('button', { name: /^(确认|确定)$/ }); if (await c.count()) await c.last().click()
await sleep(5000); await p.reload(); await sleep(3000)
console.log('after:', (await p.locator(`xpath=//*[normalize-space(text())="${title}"]/ancestor::div[.//button[normalize-space()="同步"]][1]`).first().innerText()).replace(/\s+/g, ' '))
await shot(p, 'integration', `${tag}-mapping-after-sync`)
note(tag, net.join(' || ')); console.log('ERRS', errs); await b.close()
