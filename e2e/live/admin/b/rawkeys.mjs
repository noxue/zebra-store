export default async ({ api, log }) => {
  for (const k of ['smtp_config', 'smtp', 'captcha_config', 'telegram_auth_config', 'notification_center_config', 'telegram_bot_config', 'payment_callback_routes', 'callback_routes_config', 'foo_bar_unknown'])
    { const r = await api('GET', '/admin/settings?key=' + k); log('GET', k, r.status_code, r.msg, JSON.stringify(r.data).slice(0, 200)) }
  const r = await api('PUT', '/admin/settings', { key: 'foo_bar_unknown', value: { a: 1 } }); log('PUT unknown', r.status_code, r.msg)
}
