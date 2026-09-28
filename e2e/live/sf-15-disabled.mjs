// F-033 disabled user cannot log in (fresh account, single attempt).
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, QAPW } from './sf-lib.mjs'
const email = `qa-sf-dis${Date.now().toString(36)}@lab.test`
const reg = await api('auth/register', { method: 'POST', body: { email, password: QAPW, agreement_accepted: true } })
const uid = (await api('me', { token: reg.data.token })).data.id
const d = await adm(`users/${uid}`, { method: 'PUT', body: { status: 'disabled' } })
const u = (await adm(`users/${uid}`)).data
const b = await launch()
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
await go(page, S + '/auth/login', diag); await sleep(600)
await page.locator('input[type=email]').fill(email); await page.locator('input[type=password]').fill(QAPW)
await page.locator('button[type=submit]').click(); await sleep(2500)
const toast = (await page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).join('/')
const oldTok = await api('me', { token: reg.data.token })
record({ flow: 'F-033', uid, disable: [d.status_code, d.msg], status: u?.status ?? u?.user?.status, url: page.url(), toast, oldToken: [oldTok.status_code, oldTok.msg], diag, shots: [await shot(page, 'F033-disabled-login-d')] })
await b.close()
