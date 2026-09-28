import { describe, expect, it } from 'vitest'
import {
  conversionRateText,
  formToSettings,
  pickStatAmount,
  pickStatNumber,
  resolveProfileID,
  resolveProfileStatus,
  resolveUserID,
  settingsToForm,
  splitChannels,
} from './affiliateUtils'

describe('affiliate user stats', () => {
  it('reads PascalCase (Go, no json tags) and snake_case keys', () => {
    expect(pickStatNumber({ ClickCount: 5 }, 'ClickCount', 'click_count')).toBe(5)
    expect(pickStatNumber({ click_count: '7' }, 'ClickCount', 'click_count')).toBe(7)
    expect(pickStatNumber(undefined, 'ClickCount', 'click_count')).toBe(0)
    expect(pickStatAmount({ PendingCommission: '1.20' }, 'PendingCommission', 'pending_commission')).toBe('1.20')
    expect(pickStatAmount({}, 'PendingCommission', 'pending_commission')).toBe('0.00')
    expect(conversionRateText({ ConversionRate: 12.345 })).toBe('12.35%')
  })
  it('resolves ids from profile first', () => {
    const row = { id: 9, user_id: 8, status: 'disabled', profile: { id: 3, user_id: 4, status: 'active' } }
    expect(resolveProfileID(row)).toBe(3)
    expect(resolveUserID(row)).toBe(4)
    expect(resolveProfileStatus(row)).toBe('active')
    expect(resolveProfileID({ id: 9 })).toBe(9)
  })
})

describe('affiliate settings', () => {
  it('splits channels on newlines and commas', () => {
    expect(splitChannels('alipay\r\n wechat ,usdt\n\n')).toEqual(['alipay', 'wechat', 'usdt'])
  })
  it('round-trips and clamps numbers', () => {
    const form = settingsToForm({ enabled: true, commission_rate: 150, confirm_days: -1, min_withdraw_amount: 10, withdraw_channels: ['a', ' b '] })
    expect(form).toEqual({ enabled: true, commission_rate: 100, confirm_days: 0, min_withdraw_amount: 10, withdraw_channels_text: 'a\nb' })
    expect(formToSettings({ ...form, commission_rate: '' })).toEqual({
      enabled: true,
      commission_rate: 0,
      confirm_days: 0,
      min_withdraw_amount: 10,
      withdraw_channels: ['a', 'b'],
    })
  })
})
