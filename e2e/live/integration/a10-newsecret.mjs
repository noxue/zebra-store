import { admin, browser, adminCtx, shot, watch, note, sleep, load } from './lib.mjs'
const st = load(); const z = await admin('zs2'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'zs2', z.token); const p = await ctx.newPage(); watch(p, errs)
const net = []
p.on('response', async (r) => { if (r.url().includes('/api/v1/admin/site-connections') && r.request().method() !== 'GET') net.push(`${r.request().method()} ${new URL(r.url()).pathname} ${r.status()} ${(await r.text().catch(() => '')).slice(0, 300)}`) })
await p.goto('https://zs2.dot2.com/admin/site-connections'); await sleep(2000)
await p.locator('tr', { hasText: 'QA 主站' }).getByRole('button', { name: 'Ping' }).click(); await sleep(3000)
await shot(p, 'integration', 'I071-02-ping-with-old-secret', false)
await p.locator('tr', { hasText: 'QA 主站' }).getByRole('button', { name: '编辑' }).click(); await sleep(1500)
const dlg = p.locator('[role=dialog]').last()
await dlg.locator('input[name="config.api_secret"]').fill(st.qaCred.secret)
await dlg.getByRole('button', { name: /^(保存|更新|确定)$/ }).click(); await sleep(4000)
await p.locator('tr', { hasText: 'QA 主站' }).getByRole('button', { name: 'Ping' }).click(); await sleep(3000)
await shot(p, 'integration', 'I071-03-updated-secret')
note('I-071', net.join(' || ').slice(0, 1500)); console.log('ERRS', errs); await b.close()
