export default async ({ api, log }) => {
  const list = await api('GET', `/admin/coupons?page=1&page_size=50`)
  log(list.pagination?.total, JSON.stringify(list.data.map((c) => [c.id, c.code, c.value])))
}
