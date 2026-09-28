import { describe, expect, it } from 'vitest'
import type { PaymentChannel, PublicMemberLevel } from '@/api/types'
import {
  buildPromotionUrl,
  buildTelegramPayload,
  commissionStatusKey,
  computeChannelFee,
  computeUpgradeProgress,
  discountNumber,
  filterRechargeChannels,
  formatPercent,
  isChannelOutOfRange,
  maskSecret,
  rechargeStatusTone,
  resolveNextLevel,
  signedAmount,
  withdrawStatusTone,
} from '@/utils/personal'

const level = (id: number, sort: number, extra: Partial<PublicMemberLevel> = {}): PublicMemberLevel => ({
  id,
  name: { 'zh-CN': `L${id}` },
  slug: `l${id}`,
  icon: '',
  discount_rate: 100,
  recharge_threshold: 0,
  spend_threshold: 0,
  is_default: false,
  sort_order: sort,
  ...extra,
})

const channel = (extra: Partial<PaymentChannel>): PaymentChannel => ({
  id: 1,
  name: 'c',
  channel_type: 'alipay',
  provider_type: 'epay',
  interaction_mode: 'redirect',
  ...extra,
})

describe('member level helpers', () => {
  it('picks the first level by sort order when user has none', () => {
    expect(resolveNextLevel([level(2, 5), level(1, 1)], null)?.id).toBe(1)
  })
  it('returns the following level and null at the top', () => {
    const levels = [level(1, 1), level(2, 2), level(3, 3)]
    expect(resolveNextLevel(levels, levels[0])?.id).toBe(2)
    expect(resolveNextLevel(levels, levels[2])).toBeNull()
  })
  it('computes capped progress and null when thresholds are 0', () => {
    const p = computeUpgradeProgress(level(1, 1, { recharge_threshold: 100, spend_threshold: 0 }), '250.00', '10')
    expect(p?.rechargePercent).toBe(100)
    expect(p?.spendPercent).toBeNull()
    expect(computeUpgradeProgress(level(1, 1, { recharge_threshold: 200 }), '50', 0)?.rechargePercent).toBe(25)
    expect(computeUpgradeProgress(null, 1, 1)).toBeNull()
  })
  it('formats discount rate as 折 number', () => {
    expect(discountNumber(95)).toBe('9.5')
    expect(discountNumber(80)).toBe('8')
    expect(discountNumber(100)).toBeNull()
    expect(discountNumber(0)).toBeNull()
  })
})

describe('affiliate helpers', () => {
  it('builds promotion url from path or ?aff fallback', () => {
    expect(buildPromotionUrl('https://s.io', 'ABCD', '/p?aff=ABCD')).toBe('https://s.io/p?aff=ABCD')
    expect(buildPromotionUrl('https://s.io', 'ABCD', '')).toBe('https://s.io/?aff=ABCD')
    expect(buildPromotionUrl('https://s.io', '', '/x')).toBe('')
  })
  it('formats conversion rate', () => {
    expect(formatPercent(12.345)).toBe('12.35%')
    expect(formatPercent(undefined)).toBe('0.00%')
  })
  it('maps statuses', () => {
    expect(commissionStatusKey('pending_confirm')).toBe('pendingConfirm')
    expect(commissionStatusKey('weird')).toBeNull()
    expect(withdrawStatusTone('paid')).toBe('success')
    expect(withdrawStatusTone('pending_review')).toBe('warning')
  })
})

describe('wallet helpers', () => {
  it('filters unsupported epay types, hidden out-of-range and non-allowed channels', () => {
    const list = [
      channel({ id: 1 }),
      channel({ id: 2, channel_type: 'usdt' }),
      channel({ id: 3, provider_type: 'stripe', channel_type: 'stripe', min_amount: '50.00', hide_amount_out_range: true }),
      channel({ id: 4, provider_type: 'stripe', channel_type: 'stripe', min_amount: '50.00' }),
    ]
    expect(filterRechargeChannels(list, 1000, undefined).map((c) => c.id)).toEqual([1, 4])
    expect(filterRechargeChannels(list, 1000, [4]).map((c) => c.id)).toEqual([4])
    expect(filterRechargeChannels(list, 6000, []).map((c) => c.id)).toEqual([1, 3, 4])
  })
  it('detects out of range amounts', () => {
    expect(isChannelOutOfRange({ min_amount: '1.00', max_amount: '10.00' }, 1100)).toBe(true)
    expect(isChannelOutOfRange({ min_amount: '0.00', max_amount: '0.00' }, 999999)).toBe(false)
  })
  it('computes fee = rate% + fixed', () => {
    expect(computeChannelFee('100.00', { fee_rate: '2.5', fixed_fee: '1.00' })).toBe('3.50')
    expect(computeChannelFee('', { fee_rate: '2.5', fixed_fee: '1.00' })).toBe('0.00')
  })
  it('signs amounts and tones recharge statuses', () => {
    expect(signedAmount('out', '5.00 CNY')).toBe('-5.00 CNY')
    expect(signedAmount('in', '5.00 CNY')).toBe('+5.00 CNY')
    expect(rechargeStatusTone('expired')).toBe('danger')
    expect(rechargeStatusTone('pending')).toBe('warning')
  })
})

describe('misc', () => {
  it('masks api secret tail', () => {
    expect(maskSecret('ab12')).toBe('••••••••ab12')
  })
  it('validates telegram widget payload', () => {
    expect(buildTelegramPayload({ id: 7, auth_date: 100, hash: 'h', username: ' bob ' })).toEqual({
      id: 7,
      first_name: '',
      last_name: '',
      username: 'bob',
      photo_url: '',
      auth_date: 100,
      hash: 'h',
    })
    expect(buildTelegramPayload({ id: 7, auth_date: 100 })).toBeNull()
    expect(buildTelegramPayload(null)).toBeNull()
  })
})
