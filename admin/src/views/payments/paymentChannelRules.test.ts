import { describe, expect, it } from 'vitest'
import type { AdminPaymentChannel } from '@/api/types'
import {
  applyBepusdtOrderMode,
  applyConfigs,
  applyDujiaopayOrderMode,
  applyEpusdtOrderMode,
  buildChannelPayload,
  buildConfigJson,
  channelOptionsFor,
  channelToState,
  channelTypeForProvider,
  configSectionFor,
  defaultChannelForm,
  defaultConfigs,
  interactionModesFor,
  pickInteractionMode,
  resolvePayloadChannelType,
  showsChannelTypeSelect,
  type ChannelForm,
} from './paymentChannelRules'

const tx = { bepusdt: 'transaction', dujiaopay: 'transaction' }
const form = (patch: Partial<ChannelForm>): ChannelForm => ({ ...defaultChannelForm(), ...patch })

describe('channel type options', () => {
  it('maps providers to their channel types', () => {
    expect(channelOptionsFor('epay').map((o) => o.value)).toEqual(['wechat', 'alipay', 'qqpay'])
    expect(channelOptionsFor('huifu').map((o) => o.value)).toEqual(['wechat', 'alipay'])
    expect(channelOptionsFor('official').map((o) => o.value)).toEqual(['paypal', 'stripe', 'alipay', 'wechat'])
    expect(channelOptionsFor('bepusdt').map((o) => o.value)).toEqual(['usdt-trc20', 'usdc-trc20', 'trx'])
    expect(channelOptionsFor('okpay').map((o) => o.value)).toEqual(['usdt', 'trx'])
  })

  it('hides the channel select for self-derived providers', () => {
    expect(showsChannelTypeSelect('epay')).toBe(true)
    expect(showsChannelTypeSelect('official')).toBe(true)
    expect(showsChannelTypeSelect('okpay')).toBe(true)
    for (const p of ['tokenpay', 'bepusdt', 'epusdt', 'dujiaopay']) expect(showsChannelTypeSelect(p)).toBe(false)
  })

  it('resets channel_type when switching provider', () => {
    expect(channelTypeForProvider('epay', 'paypal', 'transaction')).toBe('wechat')
    expect(channelTypeForProvider('huifu', 'qqpay', 'transaction')).toBe('wechat')
    expect(channelTypeForProvider('huifu', 'alipay', 'transaction')).toBe('alipay')
    expect(channelTypeForProvider('epay', 'alipay', 'transaction')).toBe('alipay')
    expect(channelTypeForProvider('official', 'alipay', 'transaction')).toBe('alipay')
    expect(channelTypeForProvider('official', 'qqpay', 'transaction')).toBe('paypal')
    expect(channelTypeForProvider('okpay', 'wechat', 'transaction')).toBe('usdt')
    expect(channelTypeForProvider('bepusdt', 'wechat', 'transaction')).toBe('bepusdt')
    expect(channelTypeForProvider('epusdt', 'wechat', 'transaction')).toBe('epusdt')
    expect(channelTypeForProvider('tokenpay', 'wechat', 'transaction')).toBe('usdt')
    expect(channelTypeForProvider('dujiaopay', 'dujiaopay', 'transaction')).toBe('tron-usdt')
    expect(channelTypeForProvider('dujiaopay', '', 'transaction')).toBe('tron-usdt')
    expect(channelTypeForProvider('dujiaopay', 'base-usdc', 'transaction')).toBe('base-usdc')
    expect(channelTypeForProvider('dujiaopay', 'base-usdc', 'cashier')).toBe('dujiaopay')
    // leftovers from other providers are not token ids
    expect(channelTypeForProvider('dujiaopay', 'bepusdt', 'transaction')).toBe('tron-usdt')
    expect(channelTypeForProvider('dujiaopay', 'alipay', 'transaction')).toBe('tron-usdt')
    expect(channelTypeForProvider('dujiaopay', 'x-layer-usdt0', 'transaction')).toBe('x-layer-usdt0')
  })
})

describe('interaction modes', () => {
  it('depends on provider, channel and order mode', () => {
    expect(interactionModesFor('epay', 'alipay', tx)).toEqual(['qr', 'redirect'])
    expect(interactionModesFor('huifu', 'alipay', tx)).toEqual(['redirect'])
    expect(interactionModesFor('bepusdt', 'bepusdt', tx)).toEqual(['qr', 'redirect'])
    expect(interactionModesFor('bepusdt', 'bepusdt', { ...tx, bepusdt: 'cashier' })).toEqual(['redirect'])
    expect(interactionModesFor('epusdt', 'epusdt', tx)).toEqual(['redirect'])
    expect(interactionModesFor('dujiaopay', 'tron-usdt', { ...tx, dujiaopay: 'cashier' })).toEqual(['redirect'])
    expect(interactionModesFor('official', 'paypal', tx)).toEqual(['redirect'])
    expect(interactionModesFor('official', 'stripe', tx)).toEqual(['redirect'])
    expect(interactionModesFor('official', 'alipay', tx)).toEqual(['qr', 'wap', 'page'])
    expect(interactionModesFor('official', 'wechat', tx)).toEqual(['qr', 'redirect'])
  })

  it('keeps an allowed mode and falls back to the first otherwise', () => {
    expect(pickInteractionMode('redirect', ['qr', 'redirect'])).toBe('redirect')
    expect(pickInteractionMode('qr', ['redirect'])).toBe('redirect')
    expect(pickInteractionMode('x', [])).toBe('qr')
  })
})

describe('config sections', () => {
  it('selects the dedicated form', () => {
    expect(configSectionFor('epay', 'wechat')).toBe('epay')
    expect(configSectionFor('huifu', 'wechat')).toBe('huifu')
    expect(configSectionFor('official', 'stripe')).toBe('stripe')
    expect(configSectionFor('official', 'wechat')).toBe('wechat')
    expect(configSectionFor('dujiaopay', 'tron-usdt')).toBe('dujiaopay')
    expect(configSectionFor('unknown', 'x')).toBeNull()
  })
})

describe('Huifu defaults', () => {
  it('prefills callback URLs from the current shop origin', () => {
    const config = defaultConfigs('https://store.example.com/').huifu
    expect(config.api_base_url).toBe('https://api.huifu.com')
    expect(config.skill_source).toBe('hfps/1.3.5')
    expect(config.project_title).toBe('Zebra Store')
    expect(config.notify_url).toBe('https://store.example.com/api/v1/payments/callback')
    expect(config.return_url).toBe('https://store.example.com/pay')
  })
})

describe('applyConfigs', () => {
  it('parses stored values with defaults', () => {
    const c = applyConfigs({ epay_version: 'V1', gateway_url: 'https://g', payment_method_types: ['card', ' alipay '], sign_type: 'rsa', h5_type: 'ios' })
    expect(c.epay.epay_version).toBe('v1')
    expect(c.epay.gateway_url).toBe('https://g')
    expect(c.stripe.payment_method_types).toBe('card,alipay')
    expect(c.alipay.sign_type).toBe('RSA')
    expect(c.wechat.h5_type).toBe('IOS')
    expect(c.wechat.verification_mode).toBe('platform_certificate')
    expect(c.paypal.base_url).toBe('https://api-m.sandbox.paypal.com')
    expect(c.okpay.exchange_rate).toBe('1')
  })

  it('clears mode-specific fields for cashier mode', () => {
    const c = applyConfigs({ order_mode: 'cashier', trade_type: 'usdt.trc20', currencies: 'USDT', token: 'usdt', network: 'tron', allowed_methods: 'a,b' })
    expect(c.bepusdt.trade_type).toBe('')
    expect(c.bepusdt.currencies).toBe('USDT')
    expect(c.epusdt.token).toBe('')
    expect(c.epusdt.network).toBe('')
    expect(c.dujiaopay.allowed_methods).toBe('a,b')
    const t = applyConfigs({ currencies: 'USDT', allowed_methods: 'a' })
    expect(t.bepusdt.currencies).toBe('')
    expect(t.bepusdt.trade_type).toBe('usdt.trc20')
    expect(t.dujiaopay.allowed_methods).toBe('')
  })
})

describe('order mode side effects', () => {
  it('bepusdt', () => {
    const c = defaultConfigs().bepusdt
    c.order_mode = 'cashier'
    applyBepusdtOrderMode(c)
    expect(c.trade_type).toBe('')
    c.currencies = 'USDT'
    c.order_mode = 'transaction'
    applyBepusdtOrderMode(c)
    expect(c.currencies).toBe('')
    expect(c.trade_type).toBe('usdt.trc20')
  })

  it('epusdt', () => {
    const c = defaultConfigs().epusdt
    c.order_mode = 'cashier'
    applyEpusdtOrderMode(c)
    expect([c.token, c.network]).toEqual(['', ''])
    c.order_mode = 'transaction'
    applyEpusdtOrderMode(c)
    expect([c.token, c.network]).toEqual(['usdt', 'tron'])
  })

  it('dujiaopay', () => {
    const c = defaultConfigs().dujiaopay
    c.order_mode = 'cashier'
    expect(applyDujiaopayOrderMode(c, 'tron-usdt')).toBe('dujiaopay')
    c.allowed_methods = 'x'
    c.order_mode = 'transaction'
    expect(applyDujiaopayOrderMode(c, 'dujiaopay')).toBe('tron-usdt')
    expect(c.allowed_methods).toBe('')
  })
})

describe('buildConfigJson', () => {
  it('rejects invalid raw JSON', () => {
    expect(buildConfigJson(form({ config_json: '{bad' }), defaultConfigs())).toEqual({ ok: false })
  })

  it('epay v2 drops merchant_key from raw JSON and keeps only non-empty fields', () => {
    const configs = defaultConfigs()
    configs.epay.gateway_url = ' https://pay.example.com '
    configs.epay.merchant_id = '1001'
    configs.epay.private_key = 'PK'
    configs.epay.merchant_key = 'ignored'
    const res = buildConfigJson(form({ provider_type: 'epay', config_json: '{"merchant_key":"old","extra":1,"exchange_rate":"7"}' }), configs)
    expect(res).toEqual({
      ok: true,
      config: { extra: 1, epay_version: 'v2', gateway_url: 'https://pay.example.com', merchant_id: '1001', private_key: 'PK' },
    })
  })

  it('explicit nulls in raw JSON win over form values', () => {
    const configs = defaultConfigs()
    configs.epay.merchant_id = '1001'
    const res = buildConfigJson(form({ provider_type: 'epay', config_json: '{"merchant_id":null}' }), configs)
    expect(res.ok && res.config.merchant_id).toBeNull()
  })

  it('stripe sends payment_method_types as an array', () => {
    const configs = defaultConfigs()
    configs.stripe.payment_method_types = 'card, alipay ,'
    const res = buildConfigJson(form({ provider_type: 'official', channel_type: 'stripe' }), configs)
    expect(res.ok && res.config.payment_method_types).toEqual(['card', 'alipay'])
    expect(res.ok && res.config.api_base_url).toBe('https://api.stripe.com')
  })

  it('wechat only sends public key fields outside platform_certificate mode', () => {
    const configs = defaultConfigs()
    configs.wechat.wechatpay_public_key = 'KEY'
    let res = buildConfigJson(form({ provider_type: 'official', channel_type: 'wechat' }), configs)
    expect(res.ok && 'wechatpay_public_key' in res.config).toBe(false)
    configs.wechat.verification_mode = 'combined'
    res = buildConfigJson(form({ provider_type: 'official', channel_type: 'wechat' }), configs)
    expect(res.ok && res.config.wechatpay_public_key).toBe('KEY')
  })

  it('bepusdt fills default urls and normalizes currencies in cashier mode', () => {
    const configs = defaultConfigs()
    configs.bepusdt.notify_url = ''
    configs.bepusdt.order_mode = 'cashier'
    configs.bepusdt.currencies = ' usdt.trc20, trx '
    const res = buildConfigJson(form({ provider_type: 'bepusdt', config_json: '{"trade_type":"x"}' }), configs)
    expect(res.ok && res.config).toEqual({
      gateway_url: '',
      auth_token: '',
      order_mode: 'cashier',
      notify_url: 'https://api.yourdomain.com/api/v1/payments/callback',
      return_url: 'https://yourdomain.com/pay',
      currencies: 'USDT.TRC20,TRX',
      fiat: 'CNY',
    })
  })

  it('okpay derives coin from channel type', () => {
    const res = buildConfigJson(form({ provider_type: 'okpay', channel_type: 'trx' }), defaultConfigs())
    expect(res.ok && res.config.coin).toBe('TRX')
  })

  it('dujiaopay writes token_id in transaction mode and allowed_methods in cashier mode', () => {
    const configs = defaultConfigs()
    let res = buildConfigJson(form({ provider_type: 'dujiaopay', channel_type: ' Tron-USDT ', config_json: '{"chain":"tron"}' }), configs)
    expect(res.ok && res.config.token_id).toBe('tron-usdt')
    expect(res.ok && 'chain' in res.config).toBe(false)
    configs.dujiaopay.order_mode = 'cashier'
    configs.dujiaopay.allowed_methods = ' Tron-USDT, ,base-usdc'
    res = buildConfigJson(form({ provider_type: 'dujiaopay', channel_type: 'dujiaopay', config_json: '{"token_id":"x"}' }), configs)
    expect(res.ok && res.config.allowed_methods).toBe('tron-usdt,base-usdc')
    expect(res.ok && 'token_id' in res.config).toBe(false)
  })
})

describe('payload', () => {
  it('resolves the sent channel_type', () => {
    const configs = defaultConfigs()
    expect(resolvePayloadChannelType({ provider_type: 'tokenpay', channel_type: 'x' }, configs)).toBe('usdt')
    expect(resolvePayloadChannelType({ provider_type: 'bepusdt', channel_type: 'x' }, configs)).toBe('bepusdt')
    expect(resolvePayloadChannelType({ provider_type: 'epusdt', channel_type: 'x' }, configs)).toBe('epusdt')
    expect(resolvePayloadChannelType({ provider_type: 'dujiaopay', channel_type: ' Base-USDC ' }, configs)).toBe('base-usdc')
    expect(resolvePayloadChannelType({ provider_type: 'epay', channel_type: 'qqpay' }, configs)).toBe('qqpay')
  })

  it('builds a full payload with money as strings', () => {
    const res = buildChannelPayload(form({ name: 'A', fee_rate: 0.6, fixed_fee: '', sort_order: '5', member_levels: [1] }), defaultConfigs())
    expect(res.ok).toBe(true)
    if (!res.ok) return
    expect(res.payload).toMatchObject({
      name: 'A',
      provider_type: 'epay',
      channel_type: 'alipay',
      interaction_mode: 'qr',
      fee_rate: '0.6',
      fixed_fee: '0',
      min_amount: '0',
      max_amount: '0',
      member_levels: [1],
      is_active: true,
      sort_order: 5,
    })
  })

  it('reports invalid advanced JSON', () => {
    expect(buildChannelPayload(form({ config_json: '[' }), defaultConfigs())).toEqual({ ok: false, error: 'invalidConfig' })
  })
})

describe('channelToState', () => {
  const channel = (patch: Partial<AdminPaymentChannel>): AdminPaymentChannel => ({
    id: 1,
    name: 'n',
    provider_type: 'epay',
    channel_type: 'alipay',
    interaction_mode: 'redirect',
    fee_rate: '0.00',
    config_json: {},
    icon: '',
    is_active: true,
    sort_order: 1,
    created_at: '',
    updated_at: '',
    ...patch,
  })

  it('loads form fields and pretty JSON', () => {
    const { form: f, configs } = channelToState(channel({ config_json: { gateway_url: 'https://g' }, fixed_fee: 1 }))
    expect(f.fee_rate).toBe('0.00')
    expect(f.fixed_fee).toBe('1')
    expect(f.min_amount).toBe('0')
    expect(f.config_json).toBe('{\n  "gateway_url": "https://g"\n}')
    expect(configs.epay.gateway_url).toBe('https://g')
  })

  it('resolves okpay channel type from coin and fixes invalid interaction modes', () => {
    const s = channelToState(channel({ provider_type: 'okpay', channel_type: '', config_json: { coin: 'trx' } }))
    expect(s.form.channel_type).toBe('trx')
    const p = channelToState(channel({ provider_type: 'official', channel_type: 'paypal', interaction_mode: 'qr' }))
    expect(p.form.interaction_mode).toBe('redirect')
  })
})
