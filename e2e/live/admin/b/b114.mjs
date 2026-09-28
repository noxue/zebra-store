import fs from 'node:fs'
export default async ({ page, admin, shot, log, problems, api }) => {
  const orig = (await api('GET', '/admin/settings/telegram-bot')).data
  fs.writeFileSync('live/admin/out/b-backup-telegram-bot.json', JSON.stringify(orig))
  try {
    await page.goto(`${admin}/telegram-bot/settings`); await page.waitForLoadState('networkidle')
    log('labels', (await page.locator('main label').allInnerTexts()).join(' / ').slice(0, 400))
    const inp = page.locator('main input[type=text], main input:not([type])').first()
    await inp.fill('QA Bot 名称')
    let n0 = problems.length
    await page.getByRole('button', { name: /保存/ }).first().click(); await page.waitForTimeout(1500)
    await shot(page, 'b-114-settings-saved', false)
    const now = (await api('GET', '/admin/settings/telegram-bot')).data
    log('saved', JSON.stringify(problems.slice(n0)), JSON.stringify(now.basic), 'config_version', orig.config_version, '->', now.config_version)
    // help center: add item
    await page.goto(`${admin}/telegram-bot/help-center`); await page.waitForLoadState('networkidle'); await shot(page, 'b-114-help', true)
    await page.goto(`${admin}/telegram-bot/menu`); await page.waitForLoadState('networkidle'); await shot(page, 'b-114-menu', true)
    log('menu text', (await page.locator('main').innerText()).slice(0, 500).replace(/\n/g, ' / '))
    await page.goto(`${admin}/telegram-bot/status`); await page.waitForLoadState('networkidle'); await shot(page, 'b-114-status', false)
    log('status text', (await page.locator('main').innerText()).slice(0, 400).replace(/\n/g, ' / '))
    // broadcasts
    const users = await api('GET', '/admin/telegram-bot/users?page=1&page_size=5')
    log('tg users', users.pagination?.total, JSON.stringify(users.data).slice(0, 200))
    await page.goto(`${admin}/telegram-bot/broadcasts/create`); await page.waitForLoadState('networkidle')
    n0 = problems.length
    await page.getByRole('button', { name: /发送|创建|提交|确认/ }).last().click(); await page.waitForTimeout(1200)
    await shot(page, 'b-116-empty-submit', false)
    log('broadcast empty submit', JSON.stringify(problems.slice(n0)), (await page.locator('main').innerText()).slice(0, 600).replace(/\n/g, ' / '))
    const bc = await api('POST', '/admin/telegram-bot/broadcasts', { title: 'qa', audience: 'specific', user_ids: [], message: '' })
    log('api broadcast invalid', bc.status_code, bc.msg)
  } finally {
    const r = await api('PUT', '/admin/settings/telegram-bot', orig)
    const now = (await api('GET', '/admin/settings/telegram-bot')).data
    const strip = (o) => { const c = JSON.parse(JSON.stringify(o)); delete c.config_version; delete c.updated_at; return JSON.stringify(c) }
    log('restored', r.status_code, strip(now) === strip(orig), 'version', now.config_version)
  }
}
