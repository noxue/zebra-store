import { admin, load, save, log } from './lib.mjs'
const s = await admin('store'); const st = load()
const zh = (t, e) => ({ 'zh-CN': t, 'zh-TW': t, 'en-US': e || t })
const cats = await s.get('/admin/categories')
const cat = cats.find((c) => c.slug === 'lab')
const [rows] = await s.page('/admin/products?page=1&page_size=200')
async function ensure(slug, title, price, prefix, n) {
  let p = rows.find((r) => r.slug === slug)
  if (!p) {
    p = await s.post('/admin/products', { category_id: cat.id, slug, title: zh(title, slug), description: zh('QA 对接测试'), content: zh('<p>QA</p>'), images: [], tags: [], price_amount: price, purchase_type: 'member', fulfillment_type: 'auto', is_active: true, sort_order: 50, min_purchase_quantity: 1, max_purchase_quantity: 10, stock_display_mode: 'exact' })
  }
  const d = await s.get(`/admin/products/${p.id}`)
  const sku = d.skus[0]
  const [, pg] = await s.page(`/admin/card-secrets?product_id=${p.id}&status=available&page=1&page_size=1`)
  if (!pg.total) await s.post('/admin/card-secrets/batch', { product_id: p.id, sku_id: sku.id, batch_no: prefix, note: 'qa', deduplicate: true, secrets: Array.from({ length: n }, (_, i) => `${prefix}-${String(i + 1).padStart(4, '0')}`) })
  log(slug, p.id, sku.id, JSON.stringify(sku).slice(0, 300))
  return { product_id: p.id, sku_id: sku.id }
}
st.qaA = await ensure('qa-int-a', 'QA 对接卡 A 5 元', 5, 'QA-INT-A', 30)
st.qaB = await ensure('qa-int-b', 'QA 对接卡 B 3 元', 3, 'QA-INT-B', 30)
save(st)
