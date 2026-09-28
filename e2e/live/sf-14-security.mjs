// F-026/F-027 2FA, F-029 change password, F-030 login history, F-031 profile, F-033 disabled user.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, QAPW, totp } from './sf-lib.mjs'
const b = await launch()
const run = Date.now().toString(36)
const email = `qa-sf-2fa${run}@lab.test`
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
async function uiLogin(page, diag, pw = QAPW) {
  await go(page, S + '/auth/login', diag); await sleep(600)
  await page.locator('input[type=email]').fill(email)
  await page.locator('input[type=password]').fill(pw)
  await page.locator('button[type=submit]').click(); await sleep(2500)
}
const reg = await api('auth/register', { method: 'POST', body: { email, password: QAPW, agreement_accepted: true } })
const me0 = await api('me', { token: reg.data?.token })
const uid = me0.data?.id
const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
await uiLogin(page, diag)
const R = { uid, loginUrl: page.url() }
// F-031 profile
await go(page, S + '/me/profile', diag); await sleep(1200)
await page.locator('input[placeholder*="昵称"]').fill('QA 樱花测试')
await page.locator('main select').selectOption({ label: 'English' })
await page.getByRole('button', { name: /保存资料/ }).click(); await sleep(2000)
R.profile = { toast: (await toastsOf(page)).join('/'), header: (await page.locator('main').innerText()).slice(0, 120).replace(/\s+/g, ' '), locale: await page.evaluate(() => localStorage.getItem('locale')), me: (await api('me', { token: await page.evaluate(() => localStorage.getItem('user_token')) })).data?.locale, shot: await shot(page, 'F031-profile-saved-d') }
// switch UI back to zh-CN for the rest
await page.evaluate(() => localStorage.setItem('locale', 'zh-CN'))
// F-030 login history
await go(page, S + '/me/security', diag); await sleep(1500)
const t = await txt(page); const i = t.indexOf('最近登录记录')
R.history = { t: t.slice(i, i + 400), shot: await shot(page, 'F030-security-page-d', true) }
// F-026 enable 2FA
await page.getByRole('button', { name: /启用两步验证/ }).click(); await sleep(2000)
const setupText = await txt(page)
const secret = (setupText.match(/密钥 ([A-Z2-7]{16,64})/) || [])[1] || await page.evaluate(() => { const m = document.body.innerHTML.match(/secret=([A-Z2-7]+)/); return m ? m[1] : '' })
R.setup = { secret: !!secret, qr: await page.evaluate(() => [...document.querySelectorAll('img,canvas,svg')].filter((e) => e.offsetParent && (e.width > 100 || e.getBoundingClientRect().width > 100)).length), shot: await shot(page, 'F026-2fa-setup-d', true) }
const codeInput = page.locator('input[placeholder*="6"], input[maxlength="6"], input[inputmode=numeric]').last()
await codeInput.fill(totp(secret))
await page.getByRole('button', { name: /完成绑定/ }).click(); await sleep(2500)
const afterEnable = await txt(page)
const recovery = [...new Set(afterEnable.match(/\b[a-zA-Z0-9]{4,6}-[a-zA-Z0-9]{4,6}\b|\b[A-Z0-9]{8,12}\b/g) || [])]
R.enabled = { recoveryCount: recovery.length, sample: recovery.slice(0, 3), t: afterEnable.slice(afterEnable.indexOf('两步验证'), afterEnable.indexOf('两步验证') + 500), shot: await shot(page, 'F026-2fa-enabled-d', true) }
// close any dialog
await page.keyboard.press('Escape'); await sleep(500)
const st = await api('me/2fa/status', { token: await page.evaluate(() => localStorage.getItem('user_token')) })
R.status = st.data
// F-027 login with TOTP in a new context
{
  const c2 = await open(b); await noAnnouncement(c2.ctx)
  await go(c2.page, S + '/auth/login', c2.diag); await sleep(600)
  await c2.page.locator('input[type=email]').fill(email); await c2.page.locator('input[type=password]').fill(QAPW)
  await c2.page.locator('button[type=submit]').click(); await sleep(2500)
  const chal = await shot(c2.page, 'F027-totp-challenge-d')
  const ct = (await txt(c2.page)).slice(0, 600)
  await c2.page.locator('input:visible').last().fill(totp(secret)); await c2.page.keyboard.press('Enter'); await sleep(3000)
  R.totpLogin = { ct, url: c2.page.url(), shots: [chal, await shot(c2.page, 'F027-totp-login-ok-d')] }
  await c2.ctx.close()
}
// recovery code login (UI), then reuse → rejected
if (recovery[0]) {
  for (const attempt of [1, 2]) {
    const c3 = await open(b); await noAnnouncement(c3.ctx)
    await go(c3.page, S + '/auth/login', c3.diag); await sleep(600)
    await c3.page.locator('input[type=email]').fill(email); await c3.page.locator('input[type=password]').fill(QAPW)
    await c3.page.locator('button[type=submit]').click(); await sleep(2500)
    const sw = c3.page.getByText(/恢复码/).first()
    if (await sw.isVisible().catch(() => false)) { await sw.click(); await sleep(500) }
    const sBefore = await shot(c3.page, `F027-recovery-${attempt}-form-d`)
    await c3.page.locator('input:visible').last().fill(recovery[0]); await c3.page.keyboard.press('Enter'); await sleep(3000)
    R['recovery' + attempt] = { url: c3.page.url(), toast: (await toastsOf(c3.page)).join('/'), shots: [sBefore, await shot(c3.page, `F027-recovery-${attempt}-result-d`)] }
    await c3.ctx.close()
  }
}
// F-029 change password: other session must die
const otherTok = (await api('auth/login', { method: 'POST', body: { email, password: QAPW } })).data
R.otherLogin = otherTok?.requires_totp ? 'challenge' : 'token'
const verified = otherTok?.challenge_token ? (await api('auth/login/verify-2fa', { method: 'POST', body: { challenge_token: otherTok.challenge_token, code: totp(secret) } })).data?.token : otherTok?.token
await go(page, S + '/me/security', diag); await sleep(1500)
const pw = page.locator('main input[type=password]')
const NEWPW = QAPW + 'X'
await pw.nth(0).fill(QAPW); await pw.nth(1).fill(NEWPW); await pw.nth(2).fill(NEWPW)
await page.getByRole('button', { name: /确认修改密码/ }).click(); await sleep(2500)
R.pwChange = { url: page.url(), toast: (await toastsOf(page)).join('/'), shot: await shot(page, 'F029-password-changed-d') }
const otherMe = await api('me', { token: verified })
R.otherSessionAfterPw = [otherMe.http, otherMe.status_code, otherMe.msg]
const oldPw = await api('auth/login', { method: 'POST', body: { email, password: QAPW } })
R.oldPwLogin = [oldPw.status_code, oldPw.msg]
// F-033 disable user → token invalid + login rejected (own QA account, stays disabled)
const newLogin = (await api('auth/login', { method: 'POST', body: { email, password: NEWPW } })).data
const tok2 = newLogin?.challenge_token ? (await api('auth/login/verify-2fa', { method: 'POST', body: { challenge_token: newLogin.challenge_token, code: totp(secret, 1) } })).data?.token : newLogin?.token
const dis = await adm(`users/${uid}`, { method: 'PUT', body: { status: 'disabled' } })
const bs = dis.status_code === 0 ? dis : await adm('users/batch-status', { method: 'POST', body: { user_ids: [uid], status: 'disabled' } })
R.disable = [dis.status_code, dis.msg, bs.status_code, bs.msg]
const meDisabled = await api('me', { token: tok2 })
R.tokenAfterDisable = [meDisabled.http, meDisabled.status_code, meDisabled.msg]
{
  const c4 = await open(b); await noAnnouncement(c4.ctx)
  await go(c4.page, S + '/auth/login', c4.diag); await sleep(600)
  await c4.page.locator('input[type=email]').fill(email); await c4.page.locator('input[type=password]').fill(NEWPW + 'wrong-to-avoid-limit-check')
  await c4.page.locator('button[type=submit]').click(); await sleep(2500)
  R.disabledLogin = { url: c4.page.url(), toast: (await toastsOf(c4.page)).join('/'), shot: await shot(c4.page, 'F033-disabled-login-d') }
  await c4.ctx.close()
}
// the original UI session (browser) — reload a member page
await go(page, S + '/me/orders', diag); await sleep(2000)
R.browserAfterDisable = { url: page.url(), shot: await shot(page, 'F033-browser-session-after-disable-d') }
record({ flow: 'F-026..F-033', email, R, diag })
await b.close()
