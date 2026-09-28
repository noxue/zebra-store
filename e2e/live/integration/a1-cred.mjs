import { user, admin, browser, userCtx, adminCtx, shot, watch, load, save, note, sleep } from './lib.mjs'
const st = load()
const u = await user('store', 'qa-z2@lab.test'); const me = await u.get('/me'); st.qaZ2Uid = me.id; save(st)
const b = await browser(); const errs = []
const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage(); watch(p, errs)
await p.goto('https://store.dot2.com/me/api'); await sleep(2500)
await shot(p, 'integration', 'F110-01-api-page-before')
console.log((await p.locator('button').allInnerTexts()).join(' | '))
const apply = p.getByRole('button', { name: /申请/ })
if (await apply.count()) { await apply.first().click(); await sleep(1500) }
// possible confirm dialog
const conf = p.getByRole('button', { name: /确认|确定|提交/ }); if (await conf.count()) { await conf.last().click(); await sleep(1500) }
await shot(p, 'integration', 'F110-02-applied-pending')
note('F-110', 'user cred after apply: ' + JSON.stringify(await u.get('/api-credential')))
// admin approve via UI
const a = await admin('store')
const actx = await adminCtx(b, 'store', a.token); const ap = await actx.newPage(); watch(ap, errs)
await ap.goto('https://store.dot2.com/admin/api-credentials'); await sleep(2500)
await shot(ap, 'integration', 'I080-01-admin-credentials-list')
const row = ap.locator('tr', { hasText: 'qa-z2@lab.test' })
console.log('row', await row.count(), await row.first().innerText().catch(() => ''))
console.log((await row.first().locator('button').allInnerTexts()).join(' | '))
await row.first().getByRole('button', { name: /通过|审核/ }).first().click(); await sleep(1200)
console.log('dialog btns', (await ap.locator('button:visible').allInnerTexts()).join(' | '))
await shot(ap, 'integration', 'I080-02-approve-dialog')
const ok = ap.getByRole('button', { name: /^(确认|确定|通过|确认通过)$/ }); if (await ok.count()) { await ok.last().click(); await sleep(1500) }
await shot(ap, 'integration', 'I080-03-approved')
await p.reload(); await sleep(2500); await shot(p, 'integration', 'F110-03-user-approved')
note('F-110', 'after approve: ' + JSON.stringify(await u.get('/api-credential')))
console.log('ERRS', errs)
await b.close()
