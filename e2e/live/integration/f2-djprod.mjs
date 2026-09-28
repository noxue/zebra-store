import { admin, load, save, note } from './lib.mjs'
const st = load(); const a = await admin('dujiao')
const zh = (t) => ({ 'zh-CN': t, 'zh-TW': t, 'en-US': t })
const [rows] = await a.page('/admin/products?page=1&page_size=100')
let p = rows.find((r) => r.slug === 'qa-dj-card')
const cats = await a.get('/admin/categories'); const cat = cats.find((c) => c.slug === 'lab-cards')
if (!p) p = await a.post('/admin/products', { category_id: cat.id, slug: 'qa-dj-card', title: zh('QA DJ 卡 7 元'), description: zh('QA'), price_amount: 7, fulfillment_type: 'auto', purchase_type: 'member', is_active: true })
const d = await a.get(`/admin/products/${p.id}`); const sku = d.skus[0]
await a.post('/admin/card-secrets/batch', { product_id: p.id, sku_id: sku.id, deduplicate: true, secrets: Array.from({ length: 20 }, (_, i) => `QA-DJ-${String(i + 1).padStart(4, '0')}`) })
st.djProd = { id: p.id, sku_id: sku.id }; save(st); note('I-042', `DJ product qa-dj-card #${p.id} price 7.00, 20 cards`)
