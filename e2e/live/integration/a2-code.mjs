import { user, browser, userCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const st = load()
const u = await user('store', 'qa-z2@lab.test'); const me = await u.get('/me')
const b = await browser(); const errs = []
const ctx = await userCtx(b, u.token, me); await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: 'https://store.dot2.com' })
const p = await ctx.newPage(); watch(p, errs)
let code
p.on('response', async (r) => { if (r.url().includes('/api-credential/connection-code')) { try { code = (await r.json()).data } catch {} } })
await p.goto('https://store.dot2.com/me/api'); await sleep(2500)
await p.getByRole('button', { name: /生成连接码/ }).click(); await sleep(1500)
console.log('btns', (await p.locator('button:visible').allInnerTexts()).join(' | '))
const c = p.getByRole('button', { name: /^(确认|确定|生成|继续)/ }); if (await c.count()) { await shot(p, 'integration', 'I060-01-confirm'); await c.last().click(); await sleep(2000) }
await shot(p, 'integration', 'I060-02-code-shown', false)
console.log('dialog text', (await p.locator('[role=dialog], .modal, .dialog').allInnerTexts()).join('\n').slice(0, 800))
note('I-060', 'connection-code response: ' + JSON.stringify(code).slice(0, 120) + '...')
st.qaCode = code; save(st)
console.log('ERRS', errs)
await b.close()
