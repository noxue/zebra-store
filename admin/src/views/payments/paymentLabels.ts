import type { AdminPayment, AdminPaymentChannel, Money } from '@/api/types'
import { resolveOkpayConfiguredCoin } from '@/utils/paymentChannelDisplay'

export type Translate = (key: string, params?: Record<string, unknown>) => string

const PROVIDER_TYPE_KEYS = ['official', 'epay', 'bepusdt', 'epusdt', 'okpay', 'dujiaopay', 'tokenpay', 'wallet']

/** channel_type value → i18n suffix under admin.paymentChannels.channelTypes */
const CHANNEL_TYPE_KEYS: Record<string, string> = {
  wechat: 'wechat',
  alipay: 'alipay',
  qqpay: 'qqpay',
  paypal: 'paypal',
  stripe: 'stripe',
  usdt: 'usdt',
  'usdt-trc20': 'usdtTrc20',
  'usdc-trc20': 'usdcTrc20',
  trx: 'trx',
  bepusdt: 'bepusdtCashier',
  epusdt: 'epusdt',
  balance: 'balance',
  'tron-usdt': 'tronUsdt',
  'tron-trx': 'tronTrx',
  'ethereum-usdt': 'ethereumUsdt',
  'ethereum-usdc': 'ethereumUsdc',
  'ethereum-eth': 'ethereumEth',
  'bsc-usdt': 'bscUsdt',
  'bsc-usdc': 'bscUsdc',
  'bsc-bnb': 'bscBnb',
  'polygon-usdc': 'polygonUsdc',
  'polygon-usdt0': 'polygonUsdt0',
  'base-usdc': 'baseUsdc',
  'arbitrum-usdc': 'arbitrumUsdc',
  'arbitrum-usdt0': 'arbitrumUsdt0',
  'plasma-usdt0': 'plasmaUsdt0',
  'x-layer-usdt0': 'xLayerUsdt0',
  'solana-usdc': 'solanaUsdc',
  'solana-usdt': 'solanaUsdt',
  'aptos-usdc': 'aptosUsdc',
  'aptos-usdt': 'aptosUsdt',
}

/** Channel types offered by the channel list filter (original PaymentChannels.vue). */
export const CHANNEL_FILTER_TYPES = [
  'wechat',
  'alipay',
  'qqpay',
  'paypal',
  'stripe',
  'usdt',
  'usdt-trc20',
  'usdc-trc20',
  'trx',
  'tron-usdt',
  'tron-trx',
  'ethereum-usdt',
  'ethereum-usdc',
  'base-usdc',
  'solana-usdt',
  'aptos-usdt',
]

const INTERACTION_MODES = ['qr', 'redirect', 'wap', 'page', 'balance']

export const providerTypeLabel = (t: Translate, value?: string) => {
  if (!value) return '-'
  return PROVIDER_TYPE_KEYS.includes(value) ? t(`admin.paymentChannels.providerTypes.${value}`) : value
}

/**
 * @param context 'payments' shows the epusdt channel type as "Epusdt 收银台" like the original Payments.vue.
 */
export const channelTypeLabel = (t: Translate, value?: string, context: 'channels' | 'payments' = 'channels') => {
  if (!value) return '-'
  if (context === 'payments' && value === 'epusdt') return t('admin.paymentChannels.channelTypes.epusdtCashier')
  const key = CHANNEL_TYPE_KEYS[value]
  return key ? t(`admin.paymentChannels.channelTypes.${key}`) : value
}

export const interactionModeLabel = (t: Translate, value?: string) => {
  if (!value) return '-'
  return INTERACTION_MODES.includes(value) ? t(`admin.paymentChannels.interactionModes.${value}`) : value
}

const isBlank = (v: unknown) => v === undefined || v === null || v === ''

/** `0.60%`, `0.60% + 1.00`, `1.00` or `-`. */
export const formatFeeRate = (feeRate?: Money | null, fixedFee?: Money | null) => {
  let display = '-'
  if (!isBlank(feeRate)) {
    const rate = Number(feeRate)
    if (!Number.isNaN(rate)) display = `${rate.toFixed(2)}%`
  }
  if (!isBlank(fixedFee)) {
    const fixed = Number(fixedFee)
    if (!Number.isNaN(fixed) && fixed > 0) display = display === '-' ? fixed.toFixed(2) : `${display} + ${fixed.toFixed(2)}`
  }
  return display
}

/** `12.30 CNY` (raw amount string, as the original did) or `-`. */
export const formatAmountWithCurrency = (value?: Money | null, currency?: string) => {
  if (isBlank(value)) return '-'
  return currency ? `${value} ${currency}` : String(value)
}

export const formatPayload = (payload: unknown) => {
  if (!payload) return '-'
  try {
    return JSON.stringify(payload, null, 2)
  } catch {
    return String(payload)
  }
}

export const paymentChannelTypeLabel = (t: Translate, p: Pick<AdminPayment, 'display_channel_type' | 'channel_type'>) =>
  channelTypeLabel(t, String(p.display_channel_type || p.channel_type || '').trim(), 'payments')

const cfgString = (cfg: Record<string, unknown> | undefined, key: string) => {
  const v = cfg?.[key]
  return typeof v === 'string' || typeof v === 'number' ? String(v).trim() : ''
}

/** Second line of the "type" column in the channel list (original resolveChannelTypeDisplay). */
export const resolveChannelTypeDisplay = (t: Translate, channel: Pick<AdminPaymentChannel, 'provider_type' | 'channel_type' | 'config_json'>) => {
  const cfg = channel.config_json
  switch (channel.provider_type) {
    case 'tokenpay':
      return cfgString(cfg, 'currency').toUpperCase() || 'USDT'
    case 'bepusdt': {
      if (cfgString(cfg, 'order_mode') === 'cashier') return channelTypeLabel(t, 'bepusdt')
      return cfgString(cfg, 'trade_type') || channelTypeLabel(t, channel.channel_type)
    }
    case 'epusdt': {
      const token = cfgString(cfg, 'token').toLowerCase()
      const network = cfgString(cfg, 'network').toLowerCase()
      if (!token && !network) return t('admin.paymentChannels.channelTypes.epusdtCashier')
      if (token && network) return `${token}.${network}`
      return channelTypeLabel(t, channel.channel_type)
    }
    case 'okpay':
      return resolveOkpayConfiguredCoin(cfg) || channelTypeLabel(t, channel.channel_type)
    case 'dujiaopay': {
      if (cfgString(cfg, 'order_mode') === 'cashier' || channel.channel_type === 'dujiaopay') {
        return t('admin.paymentChannels.channelTypes.dujiaopayCashier')
      }
      return channelTypeLabel(t, cfgString(cfg, 'token_id') || String(channel.channel_type || '').trim())
    }
    default:
      return channelTypeLabel(t, channel.channel_type)
  }
}
