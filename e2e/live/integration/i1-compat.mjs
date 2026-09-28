import { user, admin, browser, userCtx, shot, watch, load, save, note, sleep, setWallet } from './lib.mjs'
const st = load(); const s = await admin('store')
const u = await user('store', 'qa-acgdown@lab.test'); const me = await u.get('/me'); st.acgDownUid = me.id
let c = await u.get('/api-credential', [0, 404])
if (!c || !c.status || c.status === 'none') { await u.post('/api-credential/apply'); c = await u.get('/api-credential') }
if (c.status === 'pending_review') { const [rows] = await s.page(`/admin/api-credentials?status=pending_review&user_id=${me.id}`); for (const r of rows) await s.post(`/admin/api-credentials/${r.id}/approve`) }
await setWallet(s, me.id, 50)
const b = await browser(); const errs = []
const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage(); watch(p, errs)
let issued
p.on('response', async (r) => { if (r.url().includes('/api-credential/compat')) { try { const j = await r.json(); if (j.data?.app_key) issued = j.data } catch {} } })
await p.goto('https://store.dot2.com/me/api'); await sleep(3000)
await p.getByRole('button', { name: /生成对接密钥/ }).click(); await sleep(1500)
const cf = p.getByRole('button', { name: /^(确认|确定|生成)$/ }); if (await cf.count()) { await cf.last().click(); await sleep(2000) }
await shot(p, 'integration', 'I020-01-compat-key', true)
note('I-020', `compat issued: app_id=${issued?.app_id} key_len=${issued?.app_key?.length} fields=${Object.keys(issued || {}).join(',')}`)
st.compat = issued; save(st); console.log('ERRS', errs); await b.close()
