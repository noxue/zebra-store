import fs from 'node:fs'
export default async ({ api, log, page, admin, shot }) => {
  const orig = await api('GET', '/admin/settings?key=dashboard_config'); log('orig', JSON.stringify(orig.data))
  fs.writeFileSync('live/admin/out/b013-backup.json', JSON.stringify(orig.data))
  const kpi = async () => { const r = await api('GET', '/admin/dashboard/overview?range=today&tz=Asia%2FSingapore&force_refresh=true'); const k = r.data.kpi; return `gmv=${k.gmv_paid} cost=${k.total_cost} profit=${k.total_profit}` }
  log('before', await kpi())
  // toggle via UI
  await page.goto(admin + '/settings'); await page.waitForLoadState('networkidle')
  await page.getByText(/仪表盘 \(Dashboard\)/).first().click(); await page.waitForTimeout(500)
  const sw = page.locator('main [role=switch]').first(); const cur = await sw.getAttribute('aria-checked'); log('switch', cur)
  await sw.click(); await page.getByRole('button', { name: /保存/ }).first().click(); await page.waitForTimeout(1500)
  await shot(page, 'b013-setting')
  log('after toggle', JSON.stringify((await api('GET', '/admin/settings?key=dashboard_config')).data), await kpi())
  // restore
  const r = await api('PUT', '/admin/settings', { key: 'dashboard_config', value: orig.data }); log('restore', r.status_code)
  log('restored', JSON.stringify((await api('GET', '/admin/settings?key=dashboard_config')).data), await kpi())
}
