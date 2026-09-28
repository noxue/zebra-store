export default async ({ api, log }) => {
  const item = (o) => ({ id: 1, title: { 'zh-CN': 'QA外链', 'zh-TW': '', 'en-US': '' }, link_type: 'internal', url: '/terms', target: '_self', sort_order: 1, enabled: true, icon: 'link', ...o })
  for (const o of [{}, { link_type: 'external', url: 'https://example.com/x', target: '_blank' }, { url: 'https://example.com/x' }]) {
    const r = await api('PUT', '/admin/settings', { key: 'nav_config', value: { builtin: { blog: true, notice: true, about: true }, custom_items: [item(o)] } })
    log(JSON.stringify(o), JSON.stringify(r.data?.custom_items ?? r))
  }
  log('restore', JSON.stringify(await api('PUT', '/admin/settings', { key: 'nav_config', value: {} })))
}
