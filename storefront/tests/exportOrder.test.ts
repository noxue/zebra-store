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

  it('keeps the redemption URL and numbered delivery steps in the downloaded text', async () => {
    const guide = '<p>兑换网站：https://gpt.xiaodianpu.cc</p><p>1. 网页打不开时尝试全局代理。</p><p>2. 清理缓存并重新登录。</p><p>3. Token 过期时重新获取 Session。</p><p>4. 不要提交中文 Session。</p><p>5. 到期后仍显示 Plus 时先取消自动续费。</p>'
    const child = { ...order({ status: 'delivered', payload: 'GPT-GO-EXAMPLE-CARD' }, [{ title: {}, quantity: 1, unit_price: '1.00', total_price: '1.00', instructions: { 'zh-CN': guide } }]), order_no: 'DJ100-01' }
    const current = { ...order(null), children: [child] }
    expect(await orderExportText(current, 'zh-CN', labels, vi.fn())).toBe(
      '订单编号: DJ100\n\n卡密:\nGPT-GO-EXAMPLE-CARD\n\n交付使用说明:\n兑换网站：https://gpt.xiaodianpu.cc\n1. 网页打不开时尝试全局代理。\n2. 清理缓存并重新登录。\n3. Token 过期时重新获取 Session。\n4. 不要提交中文 Session。\n5. 到期后仍显示 Plus 时先取消自动续费。\n',
    )
  })
})
