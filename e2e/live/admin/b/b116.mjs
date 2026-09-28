export default async ({ page, admin, shot, log, problems, api }) => {
  const users = await api('GET', '/admin/telegram-bot/users?page=1&page_size=5')
  if ((users.pagination?.total ?? users.data?.length ?? 1) !== 0) { log('ABORT: tg users exist'); return }
  let id
  try {
    await page.goto(`${admin}/telegram-bot/broadcasts/create`); await page.waitForLoadState('networkidle')
    await page.getByPlaceholder(/三月优惠/).fill('QA 群发测试（0 接收人）')
    await page.locator('main [contenteditable=true]').first().click(); await page.keyboard.type('QA broadcast <b>test</b>')
    const n0 = problems.length
    page.on('request', r => { if (r.url().includes('/broadcasts') && r.method() === 'POST') log('REQ', r.postData()) })
    page.on('response', async r => { if (r.url().includes('/broadcasts') && r.request().method() === 'POST') log('RES', (await r.text()).slice(0, 400)) })
    await page.getByRole('button', { name: '提交群发' }).click(); await page.waitForTimeout(2500)
    log('submit', JSON.stringify(problems.slice(n0)), page.url())
    await shot(page, 'b-116-after-submit', false)
    await page.waitForTimeout(4000)
    const list = await api('GET', '/admin/telegram-bot/broadcasts?page=1&page_size=10')
    const b = (list.data || []).find(x => (x.title || '').includes('QA 群发')); id = b?.id
    log('broadcast', JSON.stringify(b))
    if (id) {
      await page.goto(`${admin}/telegram-bot/broadcasts/${id}`); await page.waitForLoadState('networkidle'); await shot(page, 'b-116-detail', false)
      log('detail text', (await page.locator('main').innerText()).slice(0, 400).replace(/\n/g, ' / '))
      await page.goto(`${admin}/telegram-bot/broadcasts`); await page.waitForLoadState('networkidle'); await shot(page, 'b-116-list', false)
    }
  } finally {
    if (id) log('delete', JSON.stringify(await api('DELETE', `/admin/telegram-bot/broadcasts/${id}`)))
    log('remaining', JSON.stringify((await api('GET', '/admin/telegram-bot/broadcasts')).data).slice(0, 200))
  }
}
