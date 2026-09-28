import { admin, browser, adminCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const st = load()
const z = await admin('zs2')
const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'zs2', z.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('site-connections')) { try { net.push(`${r.request().method()} ${new URL(r.url()).pathname} ${r.status()} ${(await r.text()).slice(0, 1200)}`) } catch {} } })
await p.goto('https://zs2.dot2.com/admin/site-connections'); await sleep(2500)
await shot(p, 'integration', 'I061-00-z2-connections')
console.log('btns', (await p.locator('button:visible').allInnerTexts()).join(' | '))
await p.getByRole('button', { name: /新建|添加|创建/ }).first().click(); await sleep(1500)
await shot(p, 'integration', 'I061-01-new-dialog', false)
const dlg = p.locator('[role=dialog]').last()
console.log('dialog', (await dlg.innerText().catch(() => p.locator('body').innerText())).slice(0, 1500))
console.log('inputs', await p.locator('input:visible, textarea:visible, select:visible').evaluateAll((els) => els.map((e) => `${e.tagName}:${e.getAttribute('placeholder')}:${e.getAttribute('name')}`)))
console.log('ERRS', errs); console.log(net.join('\n'))
await ctx.storageState({ path: '.state/z2ui.json' })
await b.close()
