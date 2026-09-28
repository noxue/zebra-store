export default async ({ page, api, log, admin, shot }) => {
  await page.goto(`${admin}/coupons`); await page.waitForLoadState('networkidle'); await page.waitForTimeout(800)
  log('coupon rows', (await page.locator('tbody tr').allInnerTexts()).map((s) => s.replace(/\s+/g, ' ').slice(0, 90)).join(' || '))
  await shot(page, 'c-coupon-list-final')
  for (const id of [3, 4]) log('coupon del', id, (await api('DELETE', `/admin/coupons/${id}`)).status_code)
  const left = await api('GET', '/admin/media?page=1&page_size=30')
  log('media total', left.data.total, 'qa media left', left.data.items.filter((m) => /^qa-media|^qa$|passwd/.test(m.name)).length)
  log('qa categories left', (await api('GET', '/admin/categories')).data.filter((c) => c.slug.startsWith('qa-cat')).length)
  log('qa products left', (await api('GET', '/admin/products?search=qa-product&page=1&page_size=20')).data.length)
}
