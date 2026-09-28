export default async ({ api, log }) => {
  const r = await api('GET', '/admin/order-refunds?page=1&page_size=10')
  for (const x of r.data) {
    log('refund', x.id, x.order_id, x.amount, x.type || x.refund_type, x.created_at)
    const o = await api('GET', `/admin/orders/${x.order_id}`)
    const d = o.data; log('  order', d.order_no, d.status, d.total_amount, 'refunded', d.refunded_amount, 'items', JSON.stringify((d.items||[]).map(i => [i.cost_price, i.quantity, i.total_price])), 'children', (d.children||[]).length)
  }
}
