export default async ({ api, log }) => {
  const ch = await api('GET', '/admin/payment-channels?page=1&page_size=50')
  log('channels', JSON.stringify(ch.data?.map?.((c) => ({ id: c.id, name: c.name, active: c.is_active, prov: c.provider_type, scope: c.scope ?? c.usage })) ?? ch).slice(0, 1500))
  const p = await api('GET', '/admin/products?page=1&page_size=100')
  log('products total', p.pagination?.total, JSON.stringify(p.data.map((x) => [x.id, x.slug, x.is_active, x.fulfillment_type])).slice(0, 1500))
}
