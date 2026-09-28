import { user, browser, shot, note, sleep } from './lib.mjs'
const u = await user('store', 'qa-sbuyer@lab.test'); const me = await u.get('/me')
const b = await browser()
for (const h of ['sakura', 'neon', 'matcha', 'store']) {
  const ctx = await b.newContext({ viewport: { width: 1366, height: 900 }, locale: 'zh-CN' })
  await ctx.addInitScript(([t, p]) => { localStorage.setItem('user_token', t); localStorage.setItem('user_profile', JSON.stringify(p)) }, [u.token, me])
  const p = await ctx.newPage(); await p.goto(`https://${h}.dot2.com/`); await sleep(3500)
  await shot(p, 'reseller', `R008-${h}-home`, false)
  const title = await p.title(); const fav = await p.locator('link[rel*=icon]').first().getAttribute('href').catch(() => '')
  await p.goto(`https://${h}.dot2.com/me`); await sleep(2500)
  const menu = await p.locator('aside, nav').allInnerTexts()
  note('R-008', `${h}: <title>=${title} favicon=${fav} 分销中心 entry in /me: ${menu.join(' ').includes('分销中心')}`)
  await ctx.close()
}
await b.close()
