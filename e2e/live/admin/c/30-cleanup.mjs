import fs from 'node:fs'
import { TS } from './common.mjs'
const note = async (page) => { await page.waitForTimeout(1500); return (await page.locator('[role=status]').allInnerTexts()).join(' | ').slice(0, 300) }
const confirm = async (page) => { await page.waitForTimeout(500); const d = page.locator('[role=dialog]').last(); log2('confirm text', (await d.innerText()).replace(/\s+/g, ' ').slice(0, 160)); await d.getByRole('button').last().click() }
let log2
export default async ({ page, api, shot, log, admin }) => {
  log2 = log
  const ids13 = (await api('GET', '/admin/card-secrets?product_id=13&page=1&page_size=100')).data.map((c) => c.id)
  log('delete remaining secrets', JSON.stringify(await api('POST', '/admin/card-secrets/batch-delete', { ids: ids13 })).slice(0, 150))
  await page.goto(`${admin}/products`); await page.waitForLoadState('networkidle')
  await page.locator('#admin-products-search').fill('qa-'); await page.locator('#admin-products-search').press('Enter'); await page.waitForTimeout(1500)
  await page.locator('tbody tr', { hasText: `qa-product-${TS}` }).getByRole('button', { name: 'Delete' }).click()
  await confirm(page)
  log('delete product w/o stock:', await note(page), (await api('GET', '/admin/products/13')).status_code)
  log('batches after', JSON.stringify((await api('GET', '/admin/card-secrets/batches?product_id=13')).data?.map?.((b) => b.id)))
  log('coupons referencing 13', JSON.stringify((await api('GET', '/admin/coupons?page=1&page_size=50')).data.filter((c) => String(c.scope_ref_ids).includes('13')).map((c) => [c.id, c.scope_ref_ids])))
  log('post 1 related', JSON.stringify((await api('GET', '/admin/posts/1/products')).data?.map?.((p) => p.id)))
  const pp = await page.evaluate(async () => (await (await fetch('/api/v1/public/posts/qa-post-0926')).json()))
  log('public post related after product delete', JSON.stringify(pp.data?.related_products?.map?.((x) => x.slug)))
  // batch delete manual product
  await page.waitForTimeout(800)
  await page.locator('tbody tr', { hasText: `qa-manual-${TS}` }).getByRole('checkbox').click()
  await page.getByRole('button', { name: 'Batch Delete' }).click()
  await confirm(page)
  log('batch delete manual:', await note(page), (await api('GET', '/admin/products/14')).status_code)
  // coupons via UI
  await page.goto(`${admin}/coupons`); await page.waitForLoadState('networkidle')
  for (const code of ['QAC0926', 'qac0926']) {
    const row = page.locator('tbody tr').filter({ hasText: new RegExp(`\\b${code}\\b`) }).first()
    if (!(await row.count())) { log('coupon row missing', code); continue }
    await row.getByRole('button', { name: 'Delete' }).click(); await confirm(page); log('coupon delete', code, await note(page))
  }
  // media batch delete via UI
  await page.goto(`${admin}/media`); await page.waitForLoadState('networkidle')
  await page.getByRole('button', { name: 'Batch Mode' }).click()
  const ids = [13, 14, 15, 16]
  const items = (await api('GET', '/admin/media?page=1&page_size=30')).data.items.filter((m) => ids.includes(m.id))
  for (const m of items) {
    const t = page.locator(`[title="${m.name}"], p:text-is("${m.name}")`).first()
    // toggle via card container click
    await page.locator('div.group', { has: page.locator(`p:text-is("${m.name}")`) }).first().click().catch((e) => log('media click fail', m.name))
  }
  await shot(page, 'c-media-batch')
  const delBtn = page.getByRole('button', { name: 'Delete Selected' })
  log('media selected text', (await page.locator('main').innerText()).match(/\d+ media selected/)?.[0])
  if (await delBtn.isEnabled()) { await delBtn.click(); await confirm(page); log('media batch delete', await note(page)) }
  const left = (await api('GET', '/admin/media?page=1&page_size=30')).data.items.filter((m) => ids.includes(m.id)).map((m) => m.id)
  log('media left', JSON.stringify(left))
  for (const id of left) log('api del media', id, (await api('DELETE', `/admin/media/${id}`)).status_code)
  // rest via API
  const gift = JSON.parse(fs.readFileSync('live/admin/out/c-gift-ids.json', 'utf8'))
  for (const id of gift) await api('DELETE', `/admin/gift-cards/${id}`)
  log('gift left', (await api('GET', '/admin/gift-cards?page=1&page_size=50')).data.filter((g) => gift.includes(g.id)).length)
  log('banner del', (await api('DELETE', '/admin/banners/1')).status_code)
  for (const id of [1, 2, 3]) log('post del', id, (await api('DELETE', `/admin/posts/${id}`)).status_code)
  log('pcat del parent-with-child first', JSON.stringify(await api('DELETE', '/admin/post-categories/1')).slice(0, 150))
  for (const id of [2, 1]) log('pcat del', id, (await api('DELETE', `/admin/post-categories/${id}`)).status_code)
  for (const id of [1, 2]) log('chan del', id, (await api('DELETE', `/admin/payment-channels/${id}`)).status_code)
  for (const id of [9, 8, 7]) { const r = await api('DELETE', `/admin/categories/${id}`); log('cat del', id, r.status_code, r.msg) }
}
