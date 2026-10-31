import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { adminAPI } from '@/api/admin'
import type { AdminOrder } from '@/api/types'
import { confirmAction } from '@/utils/confirm'
import i18n from '@/i18n'
import { useOrderDetail } from './useOrderDetail'
vi.mock('@/utils/confirm', () => ({ confirmAction: vi.fn() }))
const wrappers: VueWrapper[] = []
const envelope = <T>(data: T) => ({ status_code: 0, msg: 'ok', data })
const sample = { id: 137, order_no: 'DJ-ADMIN', status: 'completed', paid_at: '2026-10-30T00:00:00Z', created_at: '2026-10-30T00:00:00Z', total_amount: '125.00', refunded_amount: '0.00', online_paid_amount: '125.00', refund_mode: 'contact_support', original_refund_supported: true } as AdminOrder
function page() {
  let d!: ReturnType<typeof useOrderDetail>
  wrappers.push(mount(defineComponent({ setup() { d = useOrderDetail({ maxRefundDays: () => 30, onChanged: vi.fn() }); return () => h('div') } }), { global: { plugins: [i18n] } }))
  d.order.value = { ...sample }
  return d
}
beforeEach(() => {
  setActivePinia(createPinia())
  vi.mocked(confirmAction).mockResolvedValue(true)
  vi.spyOn(adminAPI, 'getOrder').mockResolvedValue(envelope(sample))
  vi.spyOn(adminAPI, 'getProcurementOrders').mockResolvedValue(envelope([]))
  vi.spyOn(adminAPI, 'originalRefundOrder').mockResolvedValue(envelope({ attempt: { status: 'succeeded', id: 1 } }))
})
afterEach(() => { wrappers.splice(0).forEach((w) => w.unmount()); vi.restoreAllMocks() })
describe('RFD-ADMIN-01 administrator original-route refund', () => {
  it('selects original-route based on the provider capability for a direct-card order', async () => {
    const d = page(); await d.fetchDetail(137)
    expect(d.refundTab.value).toBe('original')
    expect(d.original.forceRefund).toBe(false)
    expect(d.original.restockCards).toBe(false)
  })
  it('requires confirmation and sends explicit approval with optional inventory return', async () => {
    const d = page(); d.original.amount = '125'; d.original.forceRefund = true; d.original.restockCards = true
    await d.submitOriginal()
    expect(confirmAction).toHaveBeenCalledOnce()
    expect(adminAPI.originalRefundOrder).toHaveBeenCalledWith(137, { amount: '125', remark: undefined, payment_fee_refunded: false, force_refund: true, restock_cards: true })
    expect(d.original.error).toBe('')
  })
  it('allows an administrator override beyond the buyer refund window and keeps stock unchanged by default', async () => {
    const d = page(); d.order.value = { ...sample, paid_at: '2020-01-01T00:00:00Z' }; d.original.amount = '125'; d.original.forceRefund = true
    await d.submitOriginal()
    expect(adminAPI.originalRefundOrder).toHaveBeenCalledWith(137, expect.objectContaining({ force_refund: true, restock_cards: false }))
  })
  it('does not submit when the approval confirmation is canceled', async () => {
    vi.mocked(confirmAction).mockResolvedValue(false)
    const d = page(); d.original.amount = '125'; d.original.forceRefund = true
    await d.submitOriginal(); expect(adminAPI.originalRefundOrder).not.toHaveBeenCalled()
  })
  it.each(['126', '0', '-1'])('keeps amount protection for %s', async (amount) => {
    const d = page(); d.original.amount = amount; d.original.forceRefund = true
    await d.submitOriginal(); expect(adminAPI.originalRefundOrder).not.toHaveBeenCalled(); expect(d.original.error).not.toBe('')
  })
  it('rejects partial restocking before submitting to the provider', async () => {
    const d = page(); d.original.amount = '10'; d.original.forceRefund = true; d.original.restockCards = true
    await d.submitOriginal(); expect(adminAPI.originalRefundOrder).not.toHaveBeenCalled()
    expect(d.original.error).toContain('首次全额退款')
    d.close(); expect(d.original.forceRefund).toBe(false); expect(d.original.restockCards).toBe(false)
  })
})
