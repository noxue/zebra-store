import { admin, load, save, note } from './lib.mjs'
const st = load(); const s = await admin('store'); st.sp = st.sp || {}
for (const cid of [3, 4]) {
  const [maps] = await s.page(`/admin/product-mappings?connection_id=${cid}&page=1&page_size=50`)
  for (const m of maps) {
    const p = await s.get(`/admin/products/${m.local_product_id}`)
    note(cid === 3 ? 'I-002' : 'I-042', `conn ${cid} up#${m.upstream_product_id} -> local #${p.id} "${p.title['zh-CN']}" active=${p.is_active} ftype=${p.fulfillment_type} cat=${p.category_id} upstream_fulfillment=${m.upstream_fulfillment_type} skus=${p.skus.map((x) => `${x.id}:${x.price_amount}/cost ${x.cost_price_amount}/stock ${x.auto_stock_available}|${x.manual_stock_total}`).join(',')} form=${JSON.stringify(p.manual_form_schema).slice(0, 200)}`)
    st.sp[`${cid}:${m.upstream_product_id}`] = { product_id: p.id, sku_id: p.skus[0].id, slug: p.slug, map: m.id }
    if (!p.is_active) await s.patch(`/admin/products/${p.id}`, { is_active: true, ...(p.category_id ? {} : { category_id: 4 }) })
  }
}
save(st)
