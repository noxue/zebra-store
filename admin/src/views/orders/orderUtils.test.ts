import { describe, expect, it } from 'vitest'
import type { AdminOrder, AdminOrderItem } from '@/api/types'
import {
  buildDeliveryDataPayload,
  buildRefundAllocation,
  canCreateChildFulfillment,
  canCreateFulfillment,
  canManualRefund,
  canRefundToWallet,
  canUpdateStatus,
  checkRefundAmount,
  formatFeeRate,
  fulfillmentDeliveryLines,
  isOrderRefundWindowExpired,
  itemDiscountTotal,
  itemPaidAmount,
  manualSubmissionRows,
  normalizeMaxRefundDays,
  orderPaymentFee,
  orderProfit,
  parseSortValue,
  refundableAmountValue,
  shouldShowRefundCard,
  toQueryId,
  toQueryText,
} from './orderUtils'

const item = (over: Partial<AdminOrderItem> = {}): AdminOrderItem => ({
  id: 1,
  order_id: 1,
  product_id: 1,
  sku_id: 0,
  title: { 'zh-CN': 'x' },
  quantity: 1,
  original_unit_price: '10.00',
  unit_price: '10.00',
  cost_price: '0',
  original_total_price: '10.00',
  total_price: '10.00',
  fulfillment_type: 'manual',
  created_at: '',
  ...over,
})

const order = (over: Partial<AdminOrder> = {}): AdminOrder => ({
  id: 1,
  order_no: 'NO1',
  user_id: 1,
  status: 'paid',
  currency: 'CNY',
  original_amount: '10.00',
  discount_amount: '0',
  promotion_discount_amount: '0',
  total_amount: '10.00',
  wallet_paid_amount: '0',
  online_paid_amount: '10.00',
  refunded_amount: '0',
  created_at: '2026-09-01T00:00:00Z',
  updated_at: '2026-09-01T00:00:00Z',
  paid_at: '2026-09-01T00:00:00Z',
  items: [item()],
  ...over,
})

describe('list helpers', () => {
  it('parseSortValue', () => {
    expect(parseSortValue('created_at_desc')).toEqual({ sort_by: 'created_at', sort_order: 'desc' })
    expect(parseSortValue('total_amount_asc')).toEqual({ sort_by: 'total_amount', sort_order: 'asc' })
    expect(parseSortValue('__all__')).toEqual({})
    expect(parseSortValue('')).toEqual({})
    expect(parseSortValue('created_at_up')).toEqual({})
    expect(parseSortValue('_desc')).toEqual({})
  })

  it('query helpers', () => {
    expect(toQueryText([' 5 ', '6'])).toBe('5')
    expect(toQueryText(undefined)).toBe('')
    expect(toQueryId('12')).toBe(12)
    expect(toQueryId('abc')).toBe(0)
    expect(toQueryId('-1')).toBe(0)
  })

  it('normalizeMaxRefundDays', () => {
    expect(normalizeMaxRefundDays(undefined)).toBe(30)
    expect(normalizeMaxRefundDays('7.9')).toBe(7)
    expect(normalizeMaxRefundDays(-1)).toBe(30)
    expect(normalizeMaxRefundDays(99999)).toBe(3650)
    expect(normalizeMaxRefundDays(0)).toBe(0)
  })

  it('status update rules', () => {
    for (const s of ['completed', 'canceled', 'partially_refunded', 'refunded']) expect(canUpdateStatus(order({ status: s }))).toBe(false)
    expect(canUpdateStatus(order({ status: 'paid' }))).toBe(true)
  })

  it('fulfillment rules', () => {
    expect(canCreateFulfillment(order())).toBe(true)
    expect(canCreateFulfillment(order({ status: 'fulfilling' }))).toBe(true)
    expect(canCreateFulfillment(order({ status: 'delivered' }))).toBe(false)
    expect(canCreateFulfillment(order({ items: [item({ fulfillment_type: 'auto' })] }))).toBe(false)
    expect(canCreateFulfillment(order({ fulfillment: { id: 1, order_id: 1, type: 'manual', status: 'delivered', content: '', created_at: '', updated_at: '' } }))).toBe(false)
    expect(canCreateFulfillment(order({ children: [order({ id: 2 })] }))).toBe(false)
    expect(canCreateFulfillment(order({ parent_id: 9, children: [] }))).toBe(true)
    expect(canCreateChildFulfillment(order({ parent_id: 9, items: [] }))).toBe(false)
  })
})

describe('money helpers', () => {
  it('refund allocation is cent exact and weighted', () => {
    const o = order({
      refunded_amount: '10.00',
      items: [item({ id: 1, total_price: '10' }), item({ id: 2, total_price: '20' })],
    })
    const alloc = buildRefundAllocation(o)
    expect(alloc).toEqual([3.33, 6.67])
    expect(alloc.reduce((a, b) => a + b, 0)).toBeCloseTo(10)
  })

  it('refund allocation with zero weights splits evenly', () => {
    const o = order({ refunded_amount: '0.05', items: [item({ total_price: '0' }), item({ id: 2, total_price: '0' })] })
    expect(buildRefundAllocation(o)).toEqual([0.03, 0.02])
  })

  it('profit subtracts cost, refund and merchant-absorbed fees', () => {
    const o = order({
      refunded_amount: '2.00',
      items: [item({ total_price: '10', cost_price: '3', quantity: 2 })],
      payments: [
        { status: 'success', fee_policy: 'merchant_absorbed', fee_amount: '0.50' },
        { status: 'failed', fee_policy: 'merchant_absorbed', fee_amount: '9' },
      ] as AdminOrder['payments'],
    })
    expect(orderPaymentFee(o)).toBe(0.5)
    // 10 - 3*2 - 2 - 0.5
    expect(orderProfit(o)).toBe(1.5)
  })

  it('item discount/paid amounts', () => {
    const it1 = item({ total_price: '8', coupon_discount_amount: '1', promotion_discount_amount: '2', member_discount_amount: '-1' })
    expect(itemDiscountTotal(it1)).toBe(3)
    expect(itemPaidAmount(it1)).toBe(7)
  })

  it('formatFeeRate', () => {
    expect(formatFeeRate({ fee_rate: '1.5', fixed_fee: '0.3' })).toBe('1.50% + 0.30')
    expect(formatFeeRate({ fee_rate: '', fixed_fee: 2 })).toBe('2.00')
    expect(formatFeeRate({ fee_rate: '', fixed_fee: 0 })).toBe('-')
  })
})

describe('refund rules', () => {
  const now = Date.parse('2026-09-10T00:00:00Z')
  it('refund window', () => {
    expect(isOrderRefundWindowExpired(order(), 5, now)).toBe(true)
    expect(isOrderRefundWindowExpired(order(), 30, now)).toBe(false)
    expect(isOrderRefundWindowExpired(order(), 0, now)).toBe(false)
    expect(isOrderRefundWindowExpired(order({ paid_at: undefined }), 1, now)).toBe(false)
  })

  it('refundable amount + availability', () => {
    const o = order({ paid_at: new Date().toISOString(), refunded_amount: '4.00' })
    expect(refundableAmountValue(o)).toBe(6)
    expect(canManualRefund(o, 30)).toBe(true)
    expect(canRefundToWallet(o, 30)).toBe(true)
    expect(canRefundToWallet({ ...o, user_id: 0 }, 30)).toBe(false)
    expect(canManualRefund({ ...o, refunded_amount: '10' }, 30)).toBe(false)
    expect(shouldShowRefundCard({ ...o, status: 'canceled' }, 30)).toBe(false)
    expect(shouldShowRefundCard(order({ paid_at: undefined }), 30)).toBe(false)
  })

  it('checkRefundAmount', () => {
    expect(checkRefundAmount('', 5)).toBe('invalid')
    expect(checkRefundAmount('abc', 5)).toBe('invalid')
    expect(checkRefundAmount('0', 5)).toBe('invalid')
    expect(checkRefundAmount('5.01', 5)).toBe('exceeded')
    expect(checkRefundAmount(' 5 ', 5)).toBe('ok')
  })
})

describe('fulfillment helpers', () => {
  it('delivery lines', () => {
    const lines = fulfillmentDeliveryLines({
      id: 1,
      order_id: 1,
      type: 'manual',
      status: 'delivered',
      content: '',
      created_at: '',
      updated_at: '',
      delivery_data: { note: ' hi ', entries: [{ key: 'a', value: '1' }, { key: '', value: 'v' }, { key: 'k', value: '' }, {}], extra: 'x' },
    })
    expect(lines).toEqual(['hi', 'a: 1', 'v', 'k', 'extra: x'])
  })

  it('manual submission rows follow schema order then leftovers', () => {
    const rows = manualSubmissionRows({ b: [1, 2], a: 'x', z: null }, { fields: [{ key: 'a', label: { 'zh-CN': 'A', 'en-US': 'A' } }, { key: 'b' }] })
    expect(rows.map((r) => [r.key, r.value])).toEqual([
      ['a', 'x'],
      ['b', '1, 2'],
      ['z', '-'],
    ])
    expect(rows[1]?.label).toBe('b')
    expect(manualSubmissionRows(null)).toEqual([])
  })

  it('buildDeliveryDataPayload drops empties', () => {
    expect(buildDeliveryDataPayload('  ', [{ key: ' ', value: '' }])).toEqual({})
    expect(buildDeliveryDataPayload(' n ', [{ key: 'k', value: ' v ' }, { key: '', value: '' }])).toEqual({ note: 'n', entries: [{ key: 'k', value: 'v' }] })
  })
})

describe('live QA order status fixes', () => {
  it('QA-A02 the generic status dropdown never offers refund statuses', async () => {
    const { editableOrderStatuses } = await import('./orderUtils')
    const { ORDER_STATUSES } = await import('@/utils/status')
    const editable = editableOrderStatuses(ORDER_STATUSES)
    expect(editable).not.toContain('refunded')
    expect(editable).not.toContain('partially_refunded')
    expect(editable).toEqual(['pending_payment', 'paid', 'fulfilling', 'partially_delivered', 'delivered', 'completed', 'canceled'])
  })

  it('QA-A29 completed and refunded badges use different tones', async () => {
    const { orderStatusTone } = await import('@/utils/status')
    expect(orderStatusTone('completed')).toBe('success')
    expect(orderStatusTone('refunded')).toBe('info')
    expect(orderStatusTone('completed')).not.toBe(orderStatusTone('refunded'))
  })
})

describe('order action permissions', () => {
  it('QA-A17 a read-only order role gets no status/fulfil actions and no settings read', async () => {
    const { orderActionFlags } = await import('./orderUtils')
    const { hasPermission } = await import('@/utils/permission')
    const viewer = orderActionFlags((p) => hasPermission(['GET:/admin/orders', 'GET:/admin/orders/:id'], p, false))
    expect(viewer).toEqual({ updateStatus: false, fulfill: false, readSettings: false })
    const operator = orderActionFlags((p) => hasPermission(['*:/admin/orders/:id', 'POST:/admin/fulfillments', 'GET:/admin/settings'], p, false))
    expect(operator).toEqual({ updateStatus: true, fulfill: true, readSettings: true })
  })
})
