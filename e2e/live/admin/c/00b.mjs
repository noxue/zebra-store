export default async ({ api, log }) => {
  const p = await api('GET', '/admin/products/13')
  log(JSON.stringify(p.data.skus.map((s) => [s.id, s.sku_code, s.sort_order, s.price_amount])))
}
