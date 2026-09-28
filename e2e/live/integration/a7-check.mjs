import { admin, load, save } from './lib.mjs'
const z = await admin('zs2'); const st = load()
const [maps] = await z.page('/admin/product-mappings?connection_id=2&page=1&page_size=50')
st.z2 = {}
for (const m of maps) {
  const p = await z.get(`/admin/products/${m.local_product_id}`)
  console.log('map', m.id, 'up', m.upstream_product_id, 'local', m.local_product_id, m.upstream_status, 'active', p.is_active, JSON.stringify(p.title['zh-CN']), p.skus.map((s) => `sku${s.id} price=${s.price_amount} cost=${s.cost_price_amount} upstock=${s.upstream_stock}`).join(';'))
  st.z2[m.upstream_product_id] = { map: m.id, product_id: p.id, sku_id: p.skus[0].id }
}
const c = await z.get('/admin/site-connections/2'); console.log('seq', c.last_change_seq, c.sync_mode, c.webhook_status)
save(st)
