export default async ({ api, log }) => {
  const b = await api('GET', `/admin/card-secrets/batches?product_id=13`)
  log(JSON.stringify(b.data.map((x) => [x.id, x.sku_id, x.source, x.total_count])))
  const c = await api('GET', `/admin/card-secrets?product_id=13&page=1&page_size=50`)
  log(c.pagination?.total, JSON.stringify(c.data.map((x) => [x.id, x.secret, x.status, x.batch_id])))
}
