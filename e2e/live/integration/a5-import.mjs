import { admin, browser, adminCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const z = await admin('zs2')
const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'zs2', z.token); const p = await ctx.newPage(); watch(p, errs)
await p.goto('https://zs2.dot2.com/admin/product-mappings'); await sleep(2500)
await shot(p, 'integration', 'I064-00-mappings')
console.log('btns', (await p.locator('button:visible').allInnerTexts()).join(' | '))
await p.getByRole('button', { name: /导入/ }).first().click(); await sleep(2000)
console.log('dialog', (await p.locator('[role=dialog]').last().innerText()).slice(0, 1500))
console.log('selects', await p.locator('[role=dialog] select').evaluateAll((els) => els.map((e) => [...e.options].map((o) => o.value + ':' + o.text).join(','))))
await shot(p, 'integration', 'I064-01-import-dialog', false)
console.log('ERRS', errs)
await b.close()
