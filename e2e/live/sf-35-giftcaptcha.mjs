// F-094 gift card redeem with image captcha scene + repeated wrong codes. Captcha settings restored.
import { launch, open, go, shot, record, S, sleep, noAnnouncement, adm } from './sf-lib.mjs'
const capBefore = (await adm('settings/captcha')).data
const b = await launch(); const R = {}
try {
  await adm('settings/captcha', { method: 'PUT', body: { provider: 'image', scenes: { ...capBefore.scenes, gift_card_redeem: true } } })
  await sleep(3000)
  const { ctx, page, diag } = await open(b, { storage: '.state/sf-m.json' }); await noAnnouncement(ctx)
  await go(page, S + '/me/gift-cards', diag); await sleep(2000)
  R.captchaImg = await page.evaluate(() => [...document.images].filter((i) => /^data:image/.test(i.src)).length)
  R.s1 = await shot(page, 'F094-giftcard-captcha-m', true)
  const toasts = []
  for (let i = 0; i < 7; i++) {
    const ins = page.locator('main input:visible')
    await ins.nth(0).fill('GC-WRONG-' + i)
    if ((await ins.count()) > 1) await ins.nth(1).fill('00000')
    await page.getByRole('button', { name: /立即兑换/ }).click(); await sleep(1500)
    toasts.push((await page.locator('[role=status], [role=alert], [class*=toast]').allInnerTexts()).slice(-1)[0])
  }
  R.toasts = toasts
  R.s2 = await shot(page, 'F094-giftcard-after-wrong-m', true)
  R.diag = diag
} catch (e) { R.error = e.message.slice(0, 300) } finally {
  await adm('settings/captcha', { method: 'PUT', body: { provider: capBefore.provider, scenes: capBefore.scenes } })
  R.restored = JSON.stringify((await adm('settings/captcha')).data.scenes) + ' ' + (await adm('settings/captcha')).data.provider
  record({ flow: 'F-094', R })
}
await b.close()
