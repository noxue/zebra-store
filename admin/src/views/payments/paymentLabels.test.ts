import { describe, expect, it } from 'vitest'
import { channelTypeLabel, formatAmountWithCurrency, formatFeeRate, providerTypeLabel, resolveChannelTypeDisplay } from './paymentLabels'

const t = (key: string) => `T(${key})`

describe('paymentLabels', () => {
  it('formats fee rate with optional fixed fee', () => {
    expect(formatFeeRate('0.6', 0)).toBe('0.60%')
    expect(formatFeeRate('1', '2')).toBe('1.00% + 2.00')
    expect(formatFeeRate('', '2')).toBe('2.00')
    expect(formatFeeRate(undefined, undefined)).toBe('-')
    expect(formatFeeRate('abc', '')).toBe('-')
  })

  it('formats amounts', () => {
    expect(formatAmountWithCurrency('12.30', 'CNY')).toBe('12.30 CNY')
    expect(formatAmountWithCurrency('', 'CNY')).toBe('-')
    expect(formatAmountWithCurrency('1', '')).toBe('1')
  })

  it('labels provider and channel types, falling back to the raw value', () => {
    expect(providerTypeLabel(t, 'epay')).toBe('T(admin.paymentChannels.providerTypes.epay)')
    expect(providerTypeLabel(t, 'foo')).toBe('foo')
    expect(providerTypeLabel(t, '')).toBe('-')
    expect(channelTypeLabel(t, 'usdt-trc20')).toBe('T(admin.paymentChannels.channelTypes.usdtTrc20)')
    expect(channelTypeLabel(t, 'epusdt')).toBe('T(admin.paymentChannels.channelTypes.epusdt)')
    expect(channelTypeLabel(t, 'epusdt', 'payments')).toBe('T(admin.paymentChannels.channelTypes.epusdtCashier)')
    expect(channelTypeLabel(t, 'weird')).toBe('weird')
  })

  it('resolves the channel list type display per provider', () => {
    expect(resolveChannelTypeDisplay(t, { provider_type: 'tokenpay', channel_type: 'usdt', config_json: { currency: 'trx' } })).toBe('TRX')
    expect(resolveChannelTypeDisplay(t, { provider_type: 'tokenpay', channel_type: 'usdt', config_json: {} })).toBe('USDT')
    expect(resolveChannelTypeDisplay(t, { provider_type: 'bepusdt', channel_type: 'bepusdt', config_json: { order_mode: 'cashier' } })).toBe(
      'T(admin.paymentChannels.channelTypes.bepusdtCashier)',
    )
    expect(resolveChannelTypeDisplay(t, { provider_type: 'bepusdt', channel_type: 'bepusdt', config_json: { trade_type: 'usdt.trc20' } })).toBe('usdt.trc20')
    expect(resolveChannelTypeDisplay(t, { provider_type: 'epusdt', channel_type: 'epusdt', config_json: { token: 'USDT', network: 'Tron' } })).toBe('usdt.tron')
    expect(resolveChannelTypeDisplay(t, { provider_type: 'epusdt', channel_type: 'epusdt', config_json: {} })).toBe(
      'T(admin.paymentChannels.channelTypes.epusdtCashier)',
    )
    expect(resolveChannelTypeDisplay(t, { provider_type: 'okpay', channel_type: 'usdt', config_json: { coin: 'trx' } })).toBe('TRX')
    expect(resolveChannelTypeDisplay(t, { provider_type: 'dujiaopay', channel_type: 'dujiaopay', config_json: {} })).toBe(
      'T(admin.paymentChannels.channelTypes.dujiaopayCashier)',
    )
    expect(resolveChannelTypeDisplay(t, { provider_type: 'dujiaopay', channel_type: 'tron-usdt', config_json: { token_id: 'base-usdc' } })).toBe(
      'T(admin.paymentChannels.channelTypes.baseUsdc)',
    )
    expect(resolveChannelTypeDisplay(t, { provider_type: 'epay', channel_type: 'alipay', config_json: {} })).toBe('T(admin.paymentChannels.channelTypes.alipay)')
  })
})
