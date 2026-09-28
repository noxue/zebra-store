import fs from 'node:fs'
const field = (scope, label, control = 'input') => scope.locator(`xpath=.//label[normalize-space(translate(., "*", ""))="${label}"]/following-sibling::div[1]`).first().locator(control).first()
const sel = async (s, label) => { const opts = await s.locator('option').allInnerTexts(); const i = opts.findIndex(o => o.trim() === label || o.includes(label)); if (i < 0) throw new Error(label + ' not in ' + opts.join('/')); await s.selectOption({ index: i }) }
export default async ({ page, shot, log, admin, api }) => {
  await page.goto(admin + '/payment-channels'); await page.waitForLoadState('networkidle')
  await page.getByRole('button', { name: 'New Channel' }).click(); await page.waitForTimeout(700)
  const dlg = page.locator('[role=dialog]').last()
  await shot(page, 'd-channel-create-empty', false)
  // empty save → validation
  const save = () => dlg.getByRole('button', { name: 'Save', exact: true }).click()
  const [r0] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /payment-channels$/.test(x.url()), { timeout: 4000 }).catch(() => null), save()])
  await page.waitForTimeout(600); log('empty save', r0 ? (await r0.text()).slice(0, 200) : 'NO REQUEST', '| errors:', (await dlg.locator('.text-danger-text,[class*=text-danger]').allInnerTexts()).join(' / '))
  await shot(page, 'd-channel-validation-empty', false)
  await field(dlg, 'Name').fill('QA-D disabled epay')
  await sel(field(dlg, 'Provider Type', 'select'), 'Epay')
  await page.waitForTimeout(300)
  await sel(field(dlg, 'Channel Type', 'select'), 'Alipay')
  await sel(field(dlg, 'Interaction Mode', 'select'), 'Redirect')
  await field(dlg, 'Fee Rate').fill('150').catch(e => log('no fee rate', e.message.slice(0, 50)))
  // leave gateway empty to test provider field validation
  const sw = dlg.locator('[role=switch]'); const n = await sw.count(); for (let i = 0; i < n; i++) log('switch', i, await sw.nth(i).getAttribute('aria-checked'), (await sw.nth(i).locator('xpath=..').innerText()).slice(0, 40))
  const [r1] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /payment-channels$/.test(x.url()), { timeout: 4000 }).catch(() => null), save()])
  await page.waitForTimeout(600); log('missing gateway save', r1 ? (await r1.text()).slice(0, 300) : 'NO REQUEST', '| errors:', (await dlg.locator('.text-danger-text,[class*=text-danger]').allInnerTexts()).join(' / '))
  await shot(page, 'd-channel-validation-fields', false)
  await field(dlg, 'Fee Rate').fill('1.5').catch(() => {})
  await sel(field(dlg, 'Version', 'select'), 'v1')
  await field(dlg, 'Gateway URL').fill('https://epay.qa-d.invalid')
  await field(dlg, 'Merchant ID').fill('1001')
  await field(dlg, 'Merchant Key').fill('qa-secret-key-1234567890')
  await field(dlg, 'Notify URL').fill('https://store.dot2.com/api/v1/payments/callback')
  await field(dlg, 'Return URL').fill('https://store.dot2.com/pay')
  // turn OFF enabled
  for (let i = 0; i < n; i++) { const lbl = await sw.nth(i).locator('xpath=..').innerText(); if (/Enabled/i.test(lbl) && await sw.nth(i).getAttribute('aria-checked') === 'true') { await sw.nth(i).click(); log('disabled switch', i) } }
  const [r2] = await Promise.all([page.waitForResponse(x => x.request().method() === 'POST' && /payment-channels$/.test(x.url()), { timeout: 6000 }).catch(() => null), save()])
  await page.waitForTimeout(1000); const body = r2 ? await r2.json() : null
  log('create', r2 ? r2.request().postData()?.slice(0, 700) + ' => ' + JSON.stringify(body).slice(0, 400) : 'NO REQUEST')
  await shot(page, 'd-channel-list-after-create')
  if (body?.data?.id) fs.writeFileSync('live/admin/d/channel.json', JSON.stringify(body.data))
  const pub = await page.evaluate(async () => JSON.stringify((await (await fetch('/api/v1/public/config')).json()).data.payment_channels))
  log('public payment_channels', pub.slice(0, 300))
}
