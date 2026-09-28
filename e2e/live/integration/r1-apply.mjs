import { user, browser, userCtx, shot, watch, note, sleep, load, save } from './lib.mjs'
const st = load()
const b = await browser(); const errs = []
for (const email of ['qa-rsl@lab.test', 'qa-rsl2@lab.test']) {
  const u = await user('store', email); const me = await u.get('/me'); st[email] = me.id
  const ctx = await userCtx(b, u.token, me); const p = await ctx.newPage(); watch(p, errs)
  await p.goto('https://store.dot2.com/reseller'); await sleep(3000)
  note('R-001', `${email} /reseller -> ${p.url()}`)
  await shot(p, 'reseller', `R001-${email.split('@')[0]}-01-console-before`, false)
  if (!p.url().includes('/apply')) await p.goto('https://store.dot2.com/reseller/apply'), await sleep(2500)
  const ta = p.locator('textarea:visible'); if (await ta.count()) await ta.first().fill('QA 分站测试申请：售卖游戏点卡')
  await p.getByRole('button', { name: /提交|申请/ }).last().click(); await sleep(2500)
  await shot(p, 'reseller', `R001-${email.split('@')[0]}-02-applied`, false)
  note('R-001', `${email} profile after apply: ${JSON.stringify((await u.get('/reseller/profile')).profile).slice(0, 200)}`)
  if (email === 'qa-rsl@lab.test') {
    // console on a subsite host
    await p.goto('https://sakura.dot2.com/reseller'); await sleep(3000)
    await shot(p, 'reseller', 'R001-03-console-on-subsite', false)
    note('R-001', `console on sakura host -> url ${p.url()} text: ${(await p.locator('main').innerText().catch(() => '')).replace(/\s+/g, ' ').slice(0, 150)}`)
  }
  await ctx.close()
}
save(st); console.log('ERRS', errs); await b.close()
