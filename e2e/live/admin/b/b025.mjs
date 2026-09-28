import fs from 'node:fs'
export default async ({ page, admin, base, shot, log, problems, api, b }) => {
  const bk = JSON.parse(fs.readFileSync(fs.readdirSync('live/admin/out').filter(f => f.startsWith('b-backup-')).sort().map(f => 'live/admin/out/' + f)[0], 'utf8'))
  const orig = bk['settings:site_config'].data, ann = bk['settings:home_announcement'].data
  const sp = await b.newPage({ viewport: { width: 1440, height: 900 } })
  try {
    await page.goto(`${admin}/settings?tab=legal`); await page.waitForLoadState('networkidle')
    const eds = page.locator('main [contenteditable=true], main textarea')
    log('legal editors', await eds.count())
    await eds.first().click(); await page.keyboard.type('QA服务条款文本 zh-CN')
    await page.getByTestId('settings-save').click(); await page.waitForTimeout(1500)
    const sc = (await api('GET', '/admin/settings?key=site_config')).data
    log('legal saved', JSON.stringify(sc.legal))
    await sp.goto(base + '/terms'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(800)
    log('storefront /terms has text', (await sp.locator('body').innerText()).includes('QA服务条款文本'))
    await shot(sp, 'b-025-storefront-terms', false)
    // about
    await page.goto(`${admin}/settings?tab=about`); await page.waitForLoadState('networkidle')
    await shot(page, 'b-025-about-tab')
    // external nav render
    await api('PUT', '/admin/settings', { key: 'nav_config', value: { builtin: { blog: true, notice: true, about: true }, custom_items: [{ id: 1, title: { 'zh-CN': 'QA外链', 'zh-TW': 'QA外鏈', 'en-US': 'QA Ext' }, link_type: 'external', url: 'https://example.com/x', target: '_blank', sort_order: 1, enabled: true, icon: 'link' }] } })
    await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(800)
    log('nav ext', JSON.stringify(await sp.evaluate(() => [...document.querySelectorAll('header a')].filter(a => a.href.includes('example.com')).map(a => a.innerText + '|' + a.target))))
    await api('PUT', '/admin/settings', { key: 'nav_config', value: {} })
    // announcement schedule: start in future -> hidden
    await page.goto(`${admin}/settings?tab=home_announcement`); await page.waitForLoadState('networkidle')
    await shot(page, 'b-025-announcement-tab')
    const future = new Date(Date.now() + 86400e3 * 2).toISOString()
    let r = await api('PUT', '/admin/settings', { key: 'home_announcement', value: { ...ann, start_at: future } })
    log('ann future put', r.status_code, JSON.stringify(r.data).slice(0, 200))
    const pc = await sp.evaluate(async () => (await (await fetch('/api/v1/public/config')).json()).data)
    log('public ann (future)', JSON.stringify(pc.home_announcement ?? pc.announcement ?? Object.keys(pc)).slice(0, 300))
    r = await api('PUT', '/admin/settings', { key: 'home_announcement', value: { ...ann, start_at: '2026-01-02T00:00:00Z', end_at: '2026-01-01T00:00:00Z' } })
    log('ann end<start', r.status_code, r.msg)
  } finally {
    await api('PUT', '/admin/settings', { key: 'site_config', value: orig })
    await api('PUT', '/admin/settings', { key: 'home_announcement', value: ann })
    await api('PUT', '/admin/settings', { key: 'nav_config', value: {} })
    log('restored site', JSON.stringify((await api('GET', '/admin/settings?key=site_config')).data) === JSON.stringify(orig), 'ann', JSON.stringify((await api('GET', '/admin/settings?key=home_announcement')).data) === JSON.stringify(ann))
  }
}
