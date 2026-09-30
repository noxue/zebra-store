import { describe, expect, it } from 'vitest'
import type { PaymentChannel } from '../src/api/types'
import { paymentChoiceLabel, paymentChoices, paymentTypeLabel } from '../src/utils/paymentMethods'

const channel = (id: number, name: string, types: string[], provider = 'huifu'): PaymentChannel => ({
  id,
  name,
  provider_type: provider,
  channel_type: types[0],
  supported_channel_types: types,
  interaction_mode: 'qr',
})

const t = (key: string) => ({
  'payment.channelTypes.wechat': '微信',
  'payment.channelTypes.alipay': '支付宝',
  'payment.channelTypes.usdt': 'USDT',
}[key] || key)

describe('payment type choices', () => {
  it('expands one Huifu channel into independent Alipay and WeChat methods', () => {
    const choices = paymentChoices([channel(1, 'Huifu', ['wechat', 'alipay'])])
    expect(choices.map((choice) => choice.type)).toEqual(['wechat', 'alipay'])
    expect(choices.map((choice) => paymentChoiceLabel(choice, choices, t))).toEqual(['微信', '支付宝'])
  })

  it('shows distinct methods without their channel names across providers', () => {
    const choices = paymentChoices([
      channel(1, 'Huifu', ['wechat', 'alipay']),
      channel(2, 'Epusdt', ['usdt.tron'], 'epusdt'),
    ])
    expect(choices.map((choice) => paymentChoiceLabel(choice, choices, t))).toEqual(['微信', '支付宝', 'USDT (TRON)'])
  })

  it('adds channel names only when the same payment method is duplicated', () => {
    const choices = paymentChoices([
      channel(1, 'Huifu', ['wechat', 'alipay']),
      channel(2, 'Epay', ['wechat', 'qqpay'], 'epay'),
    ])
    expect(choices.map((choice) => paymentChoiceLabel(choice, choices, t))).toEqual([
      '微信 · Huifu',
      '支付宝',
      '微信 · Epay',
      'payment.channelTypes.qqpay',
    ])
  })

  it('uses readable fallback labels for virtual currency networks', () => {
    expect(paymentTypeLabel('usdt.tron', t)).toBe('USDT (TRON)')
    expect(paymentTypeLabel('usdt.ethereum', t)).toBe('USDT (ETHEREUM)')
  })
})
