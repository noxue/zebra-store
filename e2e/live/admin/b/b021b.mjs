import fs from 'node:fs'
export default async ({ page, base, log, api }) => {
  const sc = (await api('GET', '/admin/settings?key=site_config')).data
  sc.scripts = [{ name: 'qa-tag', enabled: true, position: 'body_end', code: '<script>window.__QA_TAG="ok"</script>' }]
  log('put', JSON.stringify(await api('PUT', '/admin/settings', { key: 'site_config', value: sc })).slice(0, 200))
  const sp = await page.context().newPage()
  await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1500)
  log('tag script', await sp.evaluate(() => window.__QA_TAG))
  // restore from backup
  const bk = JSON.parse(fs.readFileSync(fs.readdirSync('live/admin/out').filter(f => f.startsWith('b-backup-')).sort().map(f => 'live/admin/out/' + f)[0], 'utf8'))
  const orig = bk['settings:site_config'].data
  log('restore', JSON.stringify(await api('PUT', '/admin/settings', { key: 'site_config', value: orig })).slice(0, 100))
  const now = (await api('GET', '/admin/settings?key=site_config')).data
  log('restored equal?', JSON.stringify(now) === JSON.stringify(orig), now.brand.site_name, now.currency, JSON.stringify(now.scripts))
  await sp.goto(base + '/'); await sp.waitForLoadState('networkidle'); await sp.waitForTimeout(1000)
  log('storefront after restore', await sp.evaluate(() => ({ t: document.title, icon: [...document.querySelectorAll('link[rel*=icon]')].map(l => l.href) })))
}
