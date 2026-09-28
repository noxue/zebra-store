// F-110..F-117 API credential page (user side) with admin approve/reject via API.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, QAPW } from './sf-lib.mjs'
const b = await launch()
const run = Date.now().toString(36)
const email = `qa-sf-api${run}@lab.test`
const reg = await api('auth/register', { method: 'POST', body: { email, password: QAPW, agreement_accepted: true } })
const tok = reg.data.token
const txt = async (p) => (await p.locator('main').first().innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
await ctx.addInitScript((t) => localStorage.setItem('user_token', t), tok)
await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: S })
const R = {}
const snap = async (k, name) => { await go(page, S + '/me/api', diag); await sleep(1800); R[k] = { t: (await txt(page)).slice(0, 1400), shot: await shot(page, name, true) } }
await snap('none', 'F110-api-none-d')
const click = async (re) => { const bt = page.getByRole('button', { name: re }).first(); if (await bt.isVisible().catch(() => false)) { await bt.click(); await sleep(1500); return true } return false }
R.applyClicked = await click(/申请/)
const confirmBtn = page.getByRole('dialog').getByRole('button', { name: /确认|提交|申请/ }).last()
if (await confirmBtn.isVisible().catch(() => false)) { await confirmBtn.click(); await sleep(1500) }
await snap('pending', 'F110-api-pending-d')
const cred = (await adm('api-credentials?page=1&page_size=20')).data.find((c) => JSON.stringify(c).includes(email))
R.cred = cred && [cred.id, cred.status]
// F-111 reject → reapply
R.reject = (await adm(`api-credentials/${cred.id}/reject`, { method: 'POST', body: { reason: 'QA-SF 请补充用途说明' } })).msg
await snap('rejected', 'F111-api-rejected-d')
R.reapplyClicked = await click(/重新申请|申请/)
if (await confirmBtn.isVisible().catch(() => false)) { await confirmBtn.click(); await sleep(1500) }
await snap('reapplied', 'F111-api-reapplied-d')
R.approve = (await adm(`api-credentials/${cred.id}/approve`, { method: 'POST', body: {} })).msg
await snap('approved', 'F110-api-approved-d')
const dlgInfo = async () => { const d = page.getByRole('dialog').last(); if (!(await d.isVisible().catch(() => false))) return null; return { t: (await d.innerText()).replace(/\s+/g, ' ').slice(0, 600), btns: await d.getByRole('button').allInnerTexts() } }
const closeDlg = async () => { for (let i = 0; i < 3; i++) { const d = page.getByRole('dialog').last(); if (!(await d.isVisible().catch(() => false))) return; const c = d.getByRole('button', { name: /关闭|我已|完成|取消|知道/ }).last(); if (await c.isVisible().catch(() => false)) await c.click(); else await page.keyboard.press('Escape'); await sleep(600) } }
const step = async (key, openRe, confirmRe, name) => {
  await go(page, S + '/me/api', diag); await sleep(1800)
  const o = {}
  o.opened = await click(openRe)
  o.d1 = await dlgInfo()
  if (o.d1 && confirmRe) { const c = page.getByRole('dialog').last().getByRole('button', { name: confirmRe }).last(); if (await c.isVisible().catch(() => false)) { await c.click(); await sleep(2500) } }
  o.d2 = await dlgInfo()
  o.page = (await txt(page)).slice(0, 900)
  o.shot = await shot(page, name, true)
  await closeDlg()
  R[key] = o
}
R.btnsApproved = await page.locator('main button').allInnerTexts()
await step('regen', /重新生成/, /确认|确定|重新生成/, 'F112-api-regenerated-d')
await step('conn', /连接码/, /生成|确认/, 'F113-api-connection-code-d')
await step('compat', /异次元|萌次元|兼容/, /生成|确认|重置/, 'F115-api-compat-d')
await go(page, S + '/me/api', diag); await sleep(1800)
const sw = page.locator('main [role=switch], main input[type=checkbox]').first()
R.hasSwitch = await sw.isVisible().catch(() => false)
if (R.hasSwitch) { await sw.click({ force: true }); await sleep(2000) }
R.afterDisable = { api: (await api('api-credential', { token: tok })).data, toast: (await toastsOf(page)).join('/'), shot: await shot(page, 'F114-api-disabled-d', true) }
if (R.hasSwitch) { await sw.click({ force: true }); await sleep(1500); R.reEnabled = (await api('api-credential', { token: tok })).data?.is_active }
record({ flow: 'F-110..F-117', email, R, diag })
await b.close()
