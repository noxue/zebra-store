export default async ({ api, log }) => {
  const cur = (await api('GET', '/admin/products/14')).data
  const body = { ...cur, payment_channel_ids: [1, 999, 1, -3] }
  const r = await api('PUT', '/admin/products/14', body)
  log('PUT', r.status_code, r.msg, JSON.stringify(r.data?.payment_channel_ids))
  const again = (await api('GET', '/admin/products/14')).data
  log('stored', JSON.stringify(again.payment_channel_ids), 'title intact', JSON.stringify(again.title), 'schema intact', !!again.manual_form_schema?.fields?.length, 'stock', again.manual_stock_total)
}
