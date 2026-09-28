export default async ({ page, log, api }) => {
  const orig = (await api('GET', '/admin/settings/affiliate')).data
  const pub = () => page.evaluate(async () => (await (await fetch('/api/v1/public/config?_=' + Date.now())).json()).data.affiliate?.enabled)
  try {
    log('put', (await api('PUT', '/admin/settings/affiliate', { ...orig, enabled: true, commission_rate: 10, withdraw_channels: ['alipay'], min_withdraw_amount: 1 })).status_code)
    const t0 = Date.now()
    for (let i = 0; i < 14; i++) { const v = await pub(); log('t+' + Math.round((Date.now() - t0) / 1000) + 's public.affiliate.enabled =', v); if (v) break; await page.waitForTimeout(5000) }
  } finally { log('restore', (await api('PUT', '/admin/settings/affiliate', orig)).status_code); await page.waitForTimeout(500); log('public after restore', await pub()) }
}
