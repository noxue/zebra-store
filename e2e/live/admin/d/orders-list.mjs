import fs from 'node:fs'
const U = JSON.parse(fs.readFileSync(new URL('./qa-user.json', import.meta.url)))
const O = JSON.parse(fs.readFileSync(new URL('./orders.json', import.meta.url)))
export default async ({ page, shot, log, admin }) => {
  const reqs = []
  page.on('response', async (r) => { if (/\/admin\/orders\?/.test(r.url())) { try { const j = await r.json(); reqs.push(r.url().replace(/^.*\/api\/v1/, '') + ' => ' + j.pagination?.total + ' ' + (j.data || []).map(o => o.order_no.slice(-6) + ':' + o.total_amount).join(',')) } catch {} } })
  await page.goto(admin + '/orders'); await page.waitForLoadState('networkidle')
  const f = async (ph, v) => { await page.getByPlaceholder(ph).fill(v); await page.waitForTimeout(1300); log(ph, v, '=>', reqs.at(-1)?.slice(0, 250)); await page.getByPlaceholder(ph).fill(''); await page.waitForTimeout(1000) }
  await f('用户ID', String(U.id))
  await f('用户邮箱 / 昵称 / 第三方账号', 'qa-user-d')
  await f('订单号', O.B.order.order_no)
  await f('订单号', O.B.order.order_no.slice(0, 12))
  await f('游客邮箱', 'nobody@x.test')
  await f('商品名称', 'ChatGPT Team')
  await f('商品名称', 'Seat')
  const sels = page.locator('main select')
  log('status options', (await sels.nth(0).locator('option').allInnerTexts()).join('/'))
  log('sort options', (await sels.nth(1).locator('option').allInnerTexts()).join('/'))
  await sels.nth(0).selectOption({ label: '待支付' }); await page.waitForTimeout(1300); log('status=待支付 =>', reqs.at(-1)?.slice(0, 250))
  await sels.nth(0).selectOption({ index: 0 }); await page.waitForTimeout(1000)
  for (const lbl of (await sels.nth(1).locator('option').allInnerTexts()).filter(o => /金额/.test(o))) { await sels.nth(1).selectOption({ label: lbl }); await page.waitForTimeout(1300); log('sort', lbl, '=>', reqs.at(-1)?.slice(0, 300)) }
  // date filter: created today from
  const dt = page.locator('main input[type=datetime-local]')
  await dt.nth(0).fill('2026-09-26T02:00'); await page.waitForTimeout(1500); log('from 02:00 local =>', reqs.at(-1)?.slice(0, 300))
  await dt.nth(0).fill(''); await page.waitForTimeout(800)
  // paging
  await page.getByRole('button', { name: /下一页/ }).click(); await page.waitForTimeout(1300); log('page2 =>', reqs.at(-1)?.slice(0, 200))
  await page.getByPlaceholder('页码').fill('99'); await page.getByRole('button', { name: '跳转' }).click(); await page.waitForTimeout(1300); log('jump 99 =>', reqs.at(-1)?.slice(0, 200), await page.locator('main').innerText().then(t => t.match(/共[^\n]*/)?.[0]))
  await shot(page, 'd-orders-jump99')
  await page.locator('main select').last().selectOption({ label: '50' }).catch(e => log('pagesize fail', e.message.slice(0, 50))); await page.waitForTimeout(1300); log('size50 =>', reqs.at(-1)?.slice(0, 120))
}
