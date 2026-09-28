import fs from 'node:fs'
const C = JSON.parse(fs.readFileSync('live/admin/d/channel.json'))
const field = (scope, label, control = 'input') => scope.locator(`xpath=.//label[normalize-space(translate(., "*", ""))="${label}"]/following-sibling::div[1]`).first().locator(control).first()
export default async ({ page, shot, log, admin, api }) => {
  const d0 = await api('GET', '/admin/payment-channels/' + C.id); log('api detail config', JSON.stringify(d0.data?.config_json))
  await page.goto(admin + '/payment-channels'); await page.waitForLoadState('networkidle')
  await shot(page, 'd-channels-list')
  log('row', (await page.locator('tbody tr', { hasText: 'QA-D' }).first().innerText()).replace(/\s+/g, ' '))
  await page.locator('tbody tr', { hasText: 'QA-D' }).first().getByRole('button', { name: /Edit/ }).click(); await page.waitForTimeout(1200)
  const dlg = page.locator('[role=dialog]').last()
  log('merchant key shown as', JSON.stringify(await field(dlg, 'Merchant Key').inputValue()), 'type', await field(dlg, 'Merchant Key').getAttribute('type'))
  await shot(page, 'd-channel-edit', false)
  await field(dlg, 'Percentage Fee (%) (Optional)').fill('150')
  const save = () => dlg.getByRole('button', { name: 'Save', exact: true }).click()
  const [r1] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 4000 }).catch(() => null), save()])
  await page.waitForTimeout(700); log('fee 150', r1 ? (await r1.text()).slice(0, 200) : 'NO REQUEST (client validation?)', (await page.locator('[role=alert],[role=status]').allInnerTexts().catch(() => [])).join('|').slice(0, 150))
  await shot(page, 'd-channel-fee150', false)
  await field(dlg, 'Percentage Fee (%) (Optional)').fill('1.5')
  await field(dlg, 'Sort Order').fill('99')
  const [r2] = await Promise.all([page.waitForResponse(x => x.request().method() === 'PUT', { timeout: 6000 }).catch(() => null), save()])
  await page.waitForTimeout(900); log('edit save body', r2?.request().postData()?.slice(0, 600), '=>', (await r2?.text())?.slice(0, 200))
  const d1 = await api('GET', '/admin/payment-channels/' + C.id); log('after edit config', JSON.stringify(d1.data?.config_json), d1.data?.fee_rate, d1.data?.sort_order, d1.data?.is_active)
  // toggle active from list? just check list has toggle
  log('list row after', (await page.locator('tbody tr', { hasText: 'QA-D' }).first().innerText()).replace(/\s+/g, ' '))
  // wechat test button in new form
  await page.getByRole('button', { name: 'New Channel' }).click(); await page.waitForTimeout(600)
  const d2 = page.locator('[role=dialog]').last(); const ps = field(d2, 'Provider Type', 'select'); const opts = await ps.locator('option').allInnerTexts(); log('providers', opts.join('/'))
  const wi = opts.findIndex(o => /wechat/i.test(o)); if (wi >= 0) { await ps.selectOption({ index: wi }); await page.waitForTimeout(500); await shot(page, 'd-channel-wechat-form', false); log('wechat test btn', await d2.getByRole('button', { name: /Test/i }).count(), (await d2.innerText()).match(/[^\n]*(public key|Public Key)[^\n]*/g)?.slice(0, 4).join(' / ')) }
  await d2.getByRole('button', { name: 'Cancel' }).click()
}
