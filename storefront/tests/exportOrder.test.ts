import { describe, expect, it, vi } from 'vitest'
import type { Order } from '@/api/types'
import { hasDeliveredOrder, orderExportText } from '@/utils/exportOrder'

const labels = { orderNo: '订单编号', keys: '卡密', instructions: '交付使用说明' }
const order = (fulfillment: Order['fulfillment'], instructions: Order['items'] = []): Order => ({
  order_no: 'DJ100', status: 'completed', currency: 'CNY', total_amount: '1.00', created_at: '',
  fulfillment, items: instructions,
})

describe('DLV-02 order TXT export', () => {
  it('exports the visible card and strips HTML from paid delivery instructions', async () => {
    const current = order({ status: 'delivered', payload: 'SECRET-123' }, [{ title: { 'zh-CN': '商品' }, quantity: 1, unit_price: '1.00', total_price: '1.00', instructions: { 'zh-CN': '<p>打开 <strong>网站</strong><br>兑换 &amp; 使用</p>' } }])
    const download = vi.fn()
    expect(await orderExportText(current, 'zh-CN', labels, download)).toBe('订单编号: DJ100\n\n卡密:\nSECRET-123\n\n交付使用说明:\n打开 网站\n兑换 & 使用\n')
    expect(download).not.toHaveBeenCalled()
  })

  it('never exports pending fulfillment or instructions', async () => {
    const current = order({ status: 'pending', payload: 'SECRET-123' }, [{ title: {}, quantity: 1, unit_price: '1.00', total_price: '1.00', instructions: { 'zh-CN': 'private' } }])
    expect(hasDeliveredOrder(current)).toBe(false)
    expect(await orderExportText(current, 'zh-CN', labels, vi.fn())).toBe('订单编号: DJ100\n')
  })

  it('fetches complete truncated fulfillment before exporting', async () => {
    const current = order({ status: 'delivered', payload: 'preview', payload_line_count: 101 })
    const download = vi.fn(async () => new Blob(['FULL-KEY-1\nFULL-KEY-2']))
    expect(await orderExportText(current, 'zh-CN', labels, download)).toContain('FULL-KEY-1\nFULL-KEY-2')
    expect(download).toHaveBeenCalledWith('DJ100')
  })

  it('uses the child delivery once when the parent contains duplicate item summaries', async () => {
    const child = { ...order({ status: 'delivered', payload: 'CHILD-KEY' }), order_no: 'DJ100-01' }
    const current = { ...order(null), children: [child] }
    expect(await orderExportText(current, 'zh-CN', labels, vi.fn())).toBe('订单编号: DJ100\n\n卡密:\nCHILD-KEY\n\n交付使用说明:\n')
  })
})
