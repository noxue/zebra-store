import crypto from 'node:crypto'
const call = async (key, secret) => {
  const path = '/api/v1/channel/me', ts = Math.floor(Date.now() / 1000)
  const sig = crypto.createHmac('sha256', secret).update(`GET\n${path}\n${ts}\n${crypto.createHash('md5').update('').digest('hex')}`).digest('hex')
  const r = await fetch('https://store.dot2.com' + path + '?channel_user_id=1', { headers: { 'Dujiao-Next-Channel-Key': key, 'Dujiao-Next-Channel-Timestamp': String(ts), 'Dujiao-Next-Channel-Signature': sig } })
  return r.status + ' ' + (await r.text()).slice(0, 140)
}
export default async ({ page, admin, shot, log, problems, api }) => {
  let id
  try {
    await page.goto(`${admin}/telegram-bot/channel-clients`); await page.waitForLoadState('networkidle')
    await page.getByRole('button', { name: /新建|创建|添加/ }).first().click(); await page.waitForTimeout(600)
    await shot(page, 'b-115-create-dialog', false)
    const dlg = page.locator('[role=dialog]').last()
    // empty submit validation
    let n0 = problems.length
    log('submit disabled when empty', await dlg.getByRole('button', { name: '新建客户端' }).isDisabled())
    await dlg.locator('input').first().fill('qa-client-b115')
    await dlg.getByRole('button', { name: '新建客户端' }).click(); await page.waitForTimeout(1500)
    await shot(page, 'b-115-created', false)
    const list = (await api('GET', '/admin/channel-clients')).data
    const c = list.find(x => x.name === 'qa-client-b115'); id = c?.id
    log('created', JSON.stringify(c))
    const detail = (await api('GET', `/admin/channel-clients/${id}`)).data
    log('detail keys', Object.keys(detail), 'secret exposed in detail?', !!detail.channel_secret || !!detail.secret)
    const shown = await page.locator('body').innerText()
    const key = detail.channel_key || c.channel_key
    const secretMatch = shown.match(/[a-f0-9]{48,}/g)
    log('visible hex strings', secretMatch?.length)
    const secret = (secretMatch || []).find(s => s !== key)
    if (secret) log('call with creds', await call(key, secret))
    // close dialog if any
    await page.keyboard.press('Escape'); await page.waitForTimeout(300)
    // reset secret via api (UI button naming unknown) -> old creds must fail
    const rs = await api('POST', `/admin/channel-clients/${id}/reset-secret`)
    log('reset', rs.status_code, Object.keys(rs.data || {}))
    const newSecret = rs.data?.channel_secret || rs.data?.secret
    if (secret) log('old secret after reset', await call(key, secret))
    if (newSecret) log('new secret', await call(key, newSecret))
    const st = await api('PUT', `/admin/channel-clients/${id}/status`, { status: 0 })
    log('disable', st.status_code)
    if (newSecret) log('new secret after disable', await call(key, newSecret))
    log('bogus key', await call('f'.repeat(64), 'x'))
    await page.reload(); await page.waitForLoadState('networkidle'); await shot(page, 'b-115-list', false)
  } finally {
    if (id) log('delete', (await api('DELETE', `/admin/channel-clients/${id}`)).status_code)
    log('remaining', JSON.stringify((await api('GET', '/admin/channel-clients')).data).slice(0, 200))
  }
}
