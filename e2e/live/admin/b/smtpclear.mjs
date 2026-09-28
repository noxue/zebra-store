export default async ({ api, log }) => {
  const raw = (await api('GET', '/admin/settings?key=smtp_config')).data
  raw.password = ''
  const r = await api('PUT', '/admin/settings', { key: 'smtp_config', value: raw }); log('generic put', r.status_code, r.msg)
  log('now', JSON.stringify((await api('GET', '/admin/settings/smtp')).data))
  for (const k of ['captcha_config', 'telegram_auth_config', 'notification_center_config', 'telegram_bot_config', 'google_auth_config']) { const x = await api('GET', '/admin/settings?key=' + k); log(k, JSON.stringify(x.data).slice(0, 160)) }
  const g = await api('PUT', '/admin/settings', { key: 'google_auth_config', value: {} }); log('generic PUT google_auth_config', g.status_code, g.msg)
  log('unknown reset', (await api('PUT', '/admin/settings', { key: 'foo_bar_unknown', value: {} })).status_code)
}
