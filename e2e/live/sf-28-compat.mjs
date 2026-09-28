// F-115/F-116 compat key generation + reset through UI (reuses the approved QA credential account).
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, QAPW } from './sf-lib.mjs'
const [email] = process.argv.slice(2)
const tok = (await api('auth/login', { method: 'POST', body: { email, password: QAPW } })).data.token
const b = await launch()
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
await ctx.addInitScript((t) => localStorage.setItem('user_token', t), tok)
await go(page, S + '/me/api', diag); await sleep(1800)
const R = {}
const sec = async () => { const t = (await page.locator('main').first().innerText()).replace(/\s+/g, ' '); const i = t.indexOf('异次元 / 萌次元 对接'); return t.slice(i, i + 700) }
await page.getByRole('button', { name: /生成对接密钥/ }).click(); await sleep(1500)
const d = page.getByRole('dialog').last()
if (await d.isVisible().catch(() => false)) { R.dlg = (await d.innerText()).replace(/\s+/g, ' ').slice(0, 300); await d.getByRole('button').last().click(); await sleep(2000) }
R.after = await sec(); R.key1 = (R.after.match(/\b[a-f0-9A-Z]{32}\b/) || [])[0]
R.shot1 = await shot(page, 'F115-compat-generated-d', true)
R.apiCompat = (await api('api-credential/compat', { token: tok })).data
await go(page, S + '/me/api', diag); await sleep(1800)
R.reload = await sec()
R.shot2 = await shot(page, 'F116-compat-controls-d', true)
R.buttons = await page.locator('main button').allInnerTexts()
record({ flow: 'F-115/F-116', R, diag })
await b.close()
