// Regression tests for bugfix-lessons §21: FE-09 (loaded interaction mode is not
// overwritten by the provider watchers) and FE-21 (create mode starts from defaults).
import { nextTick, reactive } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const getPaymentChannel = vi.fn()

vi.mock('@/api/admin', () => ({
  adminAPI: {
    getPaymentChannel: (id: number) => getPaymentChannel(id),
    getMemberLevels: () => Promise.resolve({ data: [] }),
  },
}))

const { usePaymentChannelModal } = await import('./usePaymentChannelModal')

const flush = async () => {
  for (let i = 0; i < 5; i++) await nextTick()
  await new Promise((r) => setTimeout(r, 0))
}

const epayRedirect = {
  id: 5,
  name: 'Epay A',
  icon: '',
  provider_type: 'epay',
  channel_type: 'wechat',
  interaction_mode: 'redirect',
  fee_rate: '2.50',
  fixed_fee: '0.30',
  min_amount: '1.00',
  max_amount: '0.00',
  hide_amount_out_range: true,
  payment_types: ['order'],
  payment_roles: ['member'],
  member_levels: [],
  config_json: { gateway_url: 'https://pay.example.com', merchant_id: '1001' },
  is_active: false,
  sort_order: 3,
}

describe('payment channel modal', () => {
  beforeEach(() => {
    getPaymentChannel.mockReset()
    getPaymentChannel.mockResolvedValue({ data: epayRedirect })
  })

  it('FE-09 keeps the saved interaction mode and resets it only on a provider switch', async () => {
    const state = reactive({ open: true, id: 5 as number | null })
    const modal = usePaymentChannelModal({ isOpen: () => state.open, channelId: () => state.id, close: () => {}, onSuccess: () => {} })
    await flush()
    expect(modal.form.provider_type).toBe('epay')
    expect(modal.form.interaction_mode).toBe('redirect')
    modal.form.provider_type = 'official'
    modal.form.channel_type = 'stripe'
    await flush()
    expect(modal.form.interaction_mode).toBe('redirect')
    modal.form.channel_type = 'alipay'
    await flush()
    expect(['qr', 'wap', 'page']).toContain(modal.form.interaction_mode)
  })

  it('FE-21 opening "create" after editing starts from the defaults', async () => {
    const state = reactive({ open: true, id: 5 as number | null })
    const modal = usePaymentChannelModal({ isOpen: () => state.open, channelId: () => state.id, close: () => {}, onSuccess: () => {} })
    await flush()
    expect(modal.form.name).toBe('Epay A')
    state.open = false
    await flush()
    state.id = null
    state.open = true
    await flush()
    expect(modal.form.name).toBe('')
    expect(modal.form.provider_type).toBe('epay')
    expect(modal.form.fee_rate).toBe('0')
    expect(modal.form.fixed_fee).toBe('0')
    expect(modal.form.config_json).toBe('')
    expect(modal.form.payment_roles).toEqual([])
    expect(modal.configs.epay.gateway_url).toBe('')
  })
})
