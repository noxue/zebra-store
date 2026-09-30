import type { PaymentChannel } from '@/api/types'

export interface PaymentChoice {
  channel: PaymentChannel
  type: string
}

const canonical = (type: string) => (type === 'wxpay' ? 'wechat' : type)

export const paymentChoices = (channels: PaymentChannel[]): PaymentChoice[] =>
  channels.flatMap((channel) => {
    const methods = Array.isArray(channel.supported_channel_types) && channel.supported_channel_types.length > 0
      ? channel.supported_channel_types
      : [channel.channel_type]
    return [...new Set(methods.map((value) => String(value || '').trim().toLowerCase()).filter(Boolean))].map((type) => ({ channel, type }))
  })

export const paymentTypeLabel = (type: string, translate: (key: string) => string) => {
  const normalized = canonical(type.trim().toLowerCase())
  const known: Record<string, string> = {
    wechat: 'wechat',
    alipay: 'alipay',
    qqpay: 'qqpay',
    paypal: 'paypal',
    stripe: 'stripe',
    usdt: 'usdt',
    epusdt: 'usdt',
    'usdt-trc20': 'usdtTrc20',
    'usdc-trc20': 'usdcTrc20',
    trx: 'trx',
  }
  const key = known[normalized]
  if (key) return translate(`payment.channelTypes.${key}`)
  const [token, network] = normalized.split('.', 2)
  if (token && network) return `${token.toUpperCase()} (${network.toUpperCase()})`
  const parts = normalized.split('-')
  if (parts.length > 1 && ['usdt', 'usdt0', 'usdc', 'trx', 'btc', 'eth'].includes(parts[parts.length - 1])) {
    const currency = parts.pop()!
    return `${currency.toUpperCase()} (${parts.join('-').toUpperCase()})`
  }
  return type
}

/** Add a channel name only to a payment type offered by more than one channel. */
export const paymentChoiceLabel = (
  choice: PaymentChoice,
  choices: PaymentChoice[],
  translate: (key: string) => string,
) => {
  const label = paymentTypeLabel(choice.type, translate)
  const duplicates = choices.filter((item) => canonical(item.type) === canonical(choice.type)).length > 1
  return duplicates ? `${label} · ${choice.channel.name}` : label
}
