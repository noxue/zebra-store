// F-021..F-025, F-028, F-032: registration/login settings and limits (settings restored afterwards).
import { launch, open, go, shot, record, S, sleep, noAnnouncement, api, adm, QAPW } from './sf-lib.mjs'
const b = await launch()
const run = Date.now().toString(36)
const txt = async (p) => (await p.locator('body').innerText()).replace(/\s+/g, ' ')
const regBefore = (await adm('settings?key=registration_config')).data
const capBefore = (await adm('settings/captcha')).data
const toastsOf = (p) => p.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()
try {
  // ---- F-032 + forgot page + login page look
  {
    const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
    await go(page, S + '/auth/login', diag); await sleep(800)
    const t = await txt(page)
    const s1 = await shot(page, 'F032-login-page-d')
    await go(page, S + '/auth/forgot', diag); await sleep(800)
    const t2 = await txt(page)
    const s2 = await shot(page, 'F028-forgot-page-d')
    const sendRes = ''
    const s3 = await shot(page, 'F028-forgot-send-d')
    record({ flow: 'F-032/F-028', loginHasTelegram: /Telegram/.test(t), loginHasGoogle: /Google/.test(t), login: t.slice(0, 500), forgot: t2.slice(0, 500), sendRes, diag, shots: [s1, s2, s3] })
    await ctx.close()
  }
  // ---- F-024 rate limit
  {
    const email = `qa-sf-rl${run}@lab.test`
    const reg = await api('auth/register', { method: 'POST', body: { email, password: QAPW, agreement_accepted: true } })
    const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
    await go(page, S + '/auth/login', diag); await sleep(800)
    const msgs = []
    for (let i = 1; i <= 6; i++) {
      await page.locator('input[type=email]').fill(email)
      await page.locator('input[type=password]').fill('WrongPass#' + i)
      await page.locator('button[type=submit]').click(); await sleep(1500)
      msgs.push(`${i}: ` + (await toastsOf(page)).join(' / ').slice(0, 120) + ' || ' + ((await txt(page)).match(/.{0,10}(错误|过多|锁|重试|不正确).{0,30}/)?.[0] || ''))
      if (i === 5 || i === 6) await shot(page, `F024-attempt-${i}-d`)
    }
    await page.locator('input[type=password]').fill(QAPW)
    await page.locator('button[type=submit]').click(); await sleep(1500)
    const correctAfter = page.url() + ' ' + (await toastsOf(page)).join('/')
    const s = await shot(page, 'F024-correct-after-lock-d')
    const logs = await adm(`user-login-logs?page=1&page_size=10&email=${encodeURIComponent(email)}`)
    record({ flow: 'F-024', reg: reg.status_code, msgs, correctAfter, logs: logs.data?.slice?.(0, 8).map((l) => [l.status, l.fail_reason, l.ip]), diag, shots: [s] })
    await ctx.close()
  }
  // ---- F-022 allowlist
  {
    await adm('settings', { method: 'PUT', body: { key: 'registration_config', value: { ...regBefore, email_domain_allowlist_enabled: true, allowed_email_domains: ['lab.test'] } } })
    const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
    await go(page, S + '/auth/register', diag); await sleep(1200)
    const s1 = await shot(page, 'F022-register-allowlist-d')
    const t = await txt(page)
    const selects = await page.locator('select:visible, [role=combobox]:visible').count()
    // attempt gmail through UI
    const em = page.locator('input[type=email], input[placeholder*="邮箱"]').first()
    await em.fill(`qa-sf-gm${run}@gmail.com`).catch(() => {})
    await page.locator('input[type=password]').fill(QAPW)
    await page.locator('label:has(input[type=checkbox])').first().click({ position: { x: 8, y: 8 } })
    await page.locator('button[type=submit]').click(); await sleep(2000)
    const res = (await toastsOf(page)).join('/') + ' | ' + page.url()
    const s2 = await shot(page, 'F022-register-gmail-rejected-d')
    const apiRes = await api('auth/register', { method: 'POST', body: { email: `qa-sf-gm${run}@gmail.com`, password: QAPW, agreement_accepted: true } })
    record({ flow: 'F-022', t: t.slice(0, 600), selects, res, api: [apiRes.status_code, apiRes.msg], diag, shots: [s1, s2] })
    await ctx.close()
  }
  // ---- F-023 registration closed
  {
    await adm('settings', { method: 'PUT', body: { key: 'registration_config', value: { ...regBefore, registration_enabled: false } } })
    const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
    await go(page, S + '/auth/register', diag); await sleep(1200)
    const t = await txt(page)
    const s1 = await shot(page, 'F023-register-closed-d')
    await go(page, S + '/auth/login', diag); await sleep(800)
    const loginHasRegLink = /注册/.test(await txt(page))
    const apiRes = await api('auth/register', { method: 'POST', body: { email: `qa-sf-closed${run}@lab.test`, password: QAPW, agreement_accepted: true } })
    record({ flow: 'F-023', t: t.slice(0, 500), loginHasRegLink, api: [apiRes.status_code, apiRes.msg], diag, shots: [s1, await shot(page, 'F023-login-when-closed-d')] })
    await ctx.close()
  }
} finally {
  const r = await adm('settings', { method: 'PUT', body: { key: 'registration_config', value: regBefore } })
  console.log('restored registration_config', r.status_code)
}
// ---- F-025 image captcha
try {
  await adm('settings/captcha', { method: 'PUT', body: { provider: 'image', scenes: { ...capBefore.scenes, login: true } } })
  const { ctx, page, diag } = await open(b); await noAnnouncement(ctx)
  await go(page, S + '/auth/login', diag); await sleep(1500)
  const imgs = await page.evaluate(() => [...document.images].filter((i) => /captcha|data:image/.test(i.src)).map((i) => [i.src.slice(0, 60), i.naturalWidth]))
  const s1 = await shot(page, 'F025-login-captcha-d')
  await page.locator('input[type=email]').fill('qa-sf-dmuh9ewag@lab.test')
  await page.locator('input[type=password]').fill(QAPW)
  const capInput = page.locator('input[placeholder*="验证码"]').first()
  const hasCapInput = await capInput.isVisible().catch(() => false)
  if (hasCapInput) await capInput.fill('00000')
  await page.locator('button[type=submit]').click(); await sleep(2000)
  const res = (await toastsOf(page)).join('/') + ' | ' + page.url()
  const s2 = await shot(page, 'F025-login-captcha-wrong-d')
  record({ flow: 'F-025', imgs, hasCapInput, res, diag, shots: [s1, s2] })
  await ctx.close()
} finally {
  const r = await adm('settings/captcha', { method: 'PUT', body: { provider: capBefore.provider, scenes: capBefore.scenes } })
  console.log('restored captcha', r.status_code, JSON.stringify((await adm('settings/captcha')).data.scenes), (await adm('settings?key=registration_config')).data)
}
await b.close()
