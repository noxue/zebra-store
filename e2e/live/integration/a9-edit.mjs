import { admin, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const z = await admin('zs2')
const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'zs2', z.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('/api/v1/admin/site-connections') && r.request().method() !== 'GET') net.push(`${r.request().method()} ${new URL(r.url()).pathname} ${r.status()} ${(await r.text().catch(() => '')).slice(0, 900)}`) })
await p.goto('https://zs2.dot2.com/admin/site-connections'); await sleep(2000)
await p.locator('tr', { hasText: 'QA 主站' }).getByRole('button', { name: '编辑' }).click(); await sleep(1500)
const dlg = p.locator('[role=dialog]').last()
await shot(p, 'integration', 'I063-01-edit-dialog', false)
console.log((await dlg.innerText()).slice(0, 400))
await dlg.locator('input[placeholder="https://your-site.com/callback"]').fill('https://zs2.dot2.com/api/v1/zs/events')
const sels = dlg.locator('select'); const n = await sels.count()
for (let i = 0; i < n; i++) { const opts = await sels.nth(i).evaluate((e) => [...e.options].map((o) => o.text)); if (opts.includes('是')) await sels.nth(i).selectOption({ label: '是' }) }
const secret = dlg.locator('input[name="config.api_secret"]'); console.log('secret field value length', (await secret.inputValue()).length, await secret.getAttribute('placeholder'))
await dlg.getByRole('button', { name: /^(保存|更新|确定)$/ }).click(); await sleep(4000)
await shot(p, 'integration', 'I063-02-after-save')
console.log(net.join('\n')); console.log('ERRS', errs)
await b.close()
