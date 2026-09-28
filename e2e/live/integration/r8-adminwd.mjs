import { admin, user, browser, adminCtx, shot, watch, note, sleep } from './lib.mjs'
const s = await admin('store'); const b = await browser(); const errs = []
const ctx = await adminCtx(b, 'store', s.token); const p = await ctx.newPage(); watch(p, errs)
const net = []; p.on('response', async (x) => { if (x.url().includes('/resellers/withdraws/') && x.request().method() === 'POST') net.push(`${new URL(x.url()).pathname} ${(await x.text()).slice(0, 200)}`) })
await p.goto('https://store.dot2.com/admin/resellers/withdraws'); await sleep(3000)
await shot(p, 'reseller', 'R017-01-admin-withdraws')
for (const [amt, act] of [['0.30', '打款'], ['0.22', '驳回']]) {
  const row = p.locator('tr', { hasText: 'reseller-sakura' }).filter({ hasText: amt }).filter({ hasText: /待处理|pending|待审核/ }).first()
  console.log(amt, (await row.innerText().catch(() => 'NOROW')).replace(/\s+/g, ' ').slice(0, 200), await row.locator('button').allInnerTexts().catch(() => []))
  await row.getByRole('button', { name: new RegExp(act + '|' + (act === '驳回' ? '拒绝' : '标记已打款')) }).first().click(); await sleep(1200)
  const ta = p.locator('[role=dialog] textarea:visible, [role=dialog] input[type=text]:visible'); if (await ta.count()) await ta.first().fill('QA ' + act)
  await shot(p, 'reseller', `R017-02-${act}-dialog`, false)
  await p.locator('[role=dialog] button:visible').filter({ hasText: /确认|确定|打款|驳回|拒绝/ }).last().click(); await sleep(2500)
}
await p.reload(); await sleep(3000); await shot(p, 'reseller', 'R017-03-after')
note('R-017', net.join(' || '))
const r = await user('store', 'reseller-sakura@lab.test', { register: false }); note('R-017', 'sakura balance after: ' + JSON.stringify(await r.get('/reseller/balance-accounts')))
const [led] = await r.page('/reseller/ledger-entries?page=1&page_size=5'); note('R-017', 'ledger: ' + JSON.stringify(led.slice(0, 4).map((e) => [e.id, e.type, e.amount, e.status])))
console.log('ERRS', errs); await b.close()
