import { user, admin, browser, userCtx, adminCtx, shot, watch, load, save, note, sleep, setWallet } from './lib.mjs'
const st = load()
const u = await user('dujiao', 'qa-dj@lab.test'); const me = await u.get('/me'); st.djUid = me.id
const b = await browser(); const errs = []
const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage(); watch(p, errs)
await p.goto('https://dujiao.dot2.com/me/api'); await sleep(3000)
console.log('url', p.url(), (await p.locator('button:visible').allInnerTexts()).join(' | '))
await shot(p, 'integration', 'I040-01-dj-api-page')
const ap = p.getByRole('button', { name: /申请/ }); if (await ap.count()) { await ap.first().click(); await sleep(1500); const c = p.getByRole('button', { name: /确认|确定|提交/ }); if (await c.count()) { await c.last().click(); await sleep(1500) } }
await shot(p, 'integration', 'I040-02-dj-applied')
note('I-040', 'DJ cred after apply: ' + JSON.stringify(await u.get('/api-credential', [0, 404])).slice(0, 200))
const a = await admin('dujiao')
const [rows] = await a.page(`/admin/api-credentials?status=pending_review&user_id=${me.id}`)
for (const r of rows || []) await a.post(`/admin/api-credentials/${r.id}/approve`)
const sec = await u.post('/api-credential/regenerate')
const cred = await u.get('/api-credential')
st.djCred = { key: cred.api_key, secret: sec.api_secret }
await setWallet(a, me.id, 100)
note('I-040', `DJ approved (admin API), key=${cred.api_key.slice(0, 8)}… wallet 100`)
await p.reload(); await sleep(2500); await shot(p, 'integration', 'I040-03-dj-approved')
save(st); console.log('ERRS', errs); await b.close()
