export default async ({ page, log, api }) => {
  const c = await api('POST', '/admin/member-levels', { name: { 'zh-CN': 'qa120', 'zh-TW': 'qa120', 'en-US': 'qa120' }, slug: 'qa-d-120', discount_rate: 120, recharge_threshold: 0, spend_threshold: 0, is_default: false, sort_order: 0, is_active: true })
  log('create 120', c.status_code, c.data?.discount_rate)
  try {
    log('assign', (await api('PUT', '/admin/users/8/member-level', { member_level_id: c.data.id })).status_code)
    const p = await page.evaluate(async () => { const t = (await (await fetch('/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'qa-user-d1790358804@lab.test', password: 'NewPass12345!' }) })).json()).data.token; return (await (await fetch('/api/v1/orders/preview', { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + t }, body: JSON.stringify({ items: [{ product_id: 8, sku_id: 12, quantity: 1 }] }) })).json()).data })
    log('preview with 120%', p.total_amount, p.member_discount_amount)
    const neg = await api('POST', '/admin/member-levels', { name: { 'zh-CN': 'qaneg', 'zh-TW': 'qaneg', 'en-US': 'qaneg' }, slug: 'qa-d-neg', discount_rate: -5, recharge_threshold: -1, spend_threshold: 0, is_default: false, sort_order: 0, is_active: true }); log('create neg', neg.status_code, neg.msg, neg.data?.discount_rate); if (neg.data?.id) await api('DELETE', '/admin/member-levels/' + neg.data.id)
  } finally {
    log('unassign', (await api('PUT', '/admin/users/8/member-level', { member_level_id: 0 })).status_code, 'delete', (await api('DELETE', '/admin/member-levels/' + c.data.id)).status_code)
  }
}
