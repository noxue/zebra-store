export default async ({ api, log, page }) => {
  const p = (await api('GET', '/admin/products?search=qa-manual&page=1&page_size=10')).data
  log(JSON.stringify(p.map((x) => ({ id: x.id, slug: x.slug, stock: x.manual_stock_total, schema: x.manual_form_schema }))).slice(0, 1200))
  const pub = await page.evaluate(async () => (await (await fetch(`/api/v1/public/products/qa-manual-0926`)).json()))
  log('public', JSON.stringify({ stock: pub.data?.manual_stock_available, status: pub.data?.stock_status, schema: pub.data?.manual_form_schema }).slice(0, 800))
}
