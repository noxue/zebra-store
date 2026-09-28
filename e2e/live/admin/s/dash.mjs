export default async ({ page, api, log }) => {
  for (const p of ['/admin/dashboard/overview?range=7d', '/admin/dashboard/trends?range=7d', '/admin/dashboard/inventory-alerts', '/admin/orders?page=1&page_size=5']) {
    const r = await api('GET', p); log(p, JSON.stringify(r).slice(0, 1500))
  }
}
