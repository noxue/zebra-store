export default async ({ api, log }) => {
  log(JSON.stringify(await api('GET', '/admin/settings?key=dashboard')).slice(0, 600))
  log(JSON.stringify(await api('GET', '/admin/order-refunds?page=1&page_size=5')).slice(0, 600))
}
