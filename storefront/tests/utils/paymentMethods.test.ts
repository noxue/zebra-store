import { describe, expect, it } from 'vitest'
import type { PaymentChannel } from '../../src/api/types'
import { paymentChoiceLabel, paymentChoices } from '../../src/utils/paymentMethods'

const channel = (
  id: number,
  name: string,
  channel_type: string,
  supported_channel_types?: string[],
): PaymentChannel => ({
  id,
  name,
  channel_type,
  supported_channel_types,
  provider_type: id === 1 ? 'huifu' : 'epay',
  interaction_mode: 'redirect',
})

const t = (key: string) => key.split('.').at(-1) || key

describe('payment choices', () => {
  it('expands one multi-method Epay channel without making each method a channel choice', () => {
    const choices = paymentChoices([
      channel(2, 'Epay', 'wechat', ['wechat', 'alipay', 'qqpay']),
    ])

    expect(choices.map(({ type }) => type)).toEqual(['wechat', 'alipay', 'qqpay'])
    expect(choices.map((choice) => paymentChoiceLabel(choice, choices, t))).toEqual([
      'wechat',
      'alipay',
      'qqpay',
    ])
  })

  it('shows channel names only for payment types shared across channels', () => {
    const choices = paymentChoices([
      channel(1, 'Huifu', 'wechat', ['wechat', 'alipay']),
      channel(2, 'Epay', 'wechat', ['wechat', 'qqpay']),
    ])

    expect(choices.map((choice) => paymentChoiceLabel(choice, choices, t))).toEqual([
      'wechat · Huifu',
      'alipay',
      'wechat · Epay',
      'qqpay',
    ])
  })
})
