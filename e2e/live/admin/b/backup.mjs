import fs from 'node:fs'
export default async ({ api, log }) => {
  const out = {}
  for (const k of ['site_config','nav_config','order_config','registration_config','upstream_sync_config','home_announcement','dashboard_config','template_config','wallet_config','callback_routes','payment_callback_routes','order_risk_config'])
    out['settings:'+k] = await api('GET', '/admin/settings?key='+k)
  for (const p of ['/admin/settings/smtp','/admin/settings/captcha','/admin/settings/telegram-auth','/admin/settings/google-auth','/admin/settings/order-email-template','/admin/settings/notification-center','/admin/settings/telegram-bot','/admin/settings/telegram-bot/runtime-status','/admin/channel-clients','/admin/telegram-bot/broadcasts','/admin/settings/notification-center/logs?page=1&page_size=5'])
    out[p] = await api('GET', p)
  const f = 'live/admin/out/b-backup-' + Date.now() + '.json'
  fs.writeFileSync(f, JSON.stringify(out, null, 2))
  for (const [k, v] of Object.entries(out)) log(k, v.http, v.status_code, JSON.stringify(v.data ?? v.msg).slice(0, 400))
  log('saved', f)
}
