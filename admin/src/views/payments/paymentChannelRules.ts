/**
 * Pure provider rules for the payment-channel dialog (ported from the original PaymentChannelModal.vue):
 * provider → channel type options, interaction modes, per-provider config defaults / parsing / building,
 * and the final save payload. No Vue, no API — fully unit-tested in paymentChannelRules.test.ts.
 */
import type { AdminPaymentChannel } from '@/api/types'
import { resolveOkpayChannelTypeFromConfig } from '@/utils/paymentChannelDisplay'

export const PROVIDER_TYPES = ['official', 'dujiaopay', 'epay', 'bepusdt', 'epusdt', 'okpay', 'tokenpay'] as const
export type ProviderType = (typeof PROVIDER_TYPES)[number]

export interface KeyOption {
  value: string
  /** i18n key of the label */
  labelKey: string
}

const ct = (key: string) => `admin.paymentChannels.channelTypes.${key}`
const im = (key: string) => `admin.paymentChannels.interactionModes.${key}`

export const EPAY_CHANNEL_OPTIONS: KeyOption[] = [
  { value: 'wechat', labelKey: ct('wechat') },
  { value: 'alipay', labelKey: ct('alipay') },
  { value: 'qqpay', labelKey: ct('qqpay') },
]
export const OFFICIAL_CHANNEL_OPTIONS: KeyOption[] = [
  { value: 'paypal', labelKey: ct('paypal') },
  { value: 'stripe', labelKey: ct('stripe') },
  { value: 'alipay', labelKey: ct('alipay') },
  { value: 'wechat', labelKey: ct('wechat') },
]
export const BEPUSDT_CHANNEL_OPTIONS: KeyOption[] = [
  { value: 'usdt-trc20', labelKey: ct('usdtTrc20') },
  { value: 'usdc-trc20', labelKey: ct('usdcTrc20') },
  { value: 'trx', labelKey: ct('trx') },
]
export const OKPAY_CHANNEL_OPTIONS: KeyOption[] = [
  { value: 'usdt', labelKey: ct('usdt') },
  { value: 'trx', labelKey: ct('trx') },
]
const FALLBACK_CHANNEL_OPTIONS: KeyOption[] = [...EPAY_CHANNEL_OPTIONS, ...OFFICIAL_CHANNEL_OPTIONS, ...OKPAY_CHANNEL_OPTIONS]

/** DujiaoPay token_id is typed in by the admin (e.g. tron-usdt); this is only the initial value. */
export const DUJIAOPAY_DEFAULT_TOKEN_ID = 'tron-usdt'

export const channelOptionsFor = (provider: string): KeyOption[] => {
  switch (provider) {
    case 'epay':
      return EPAY_CHANNEL_OPTIONS
    case 'official':
      return OFFICIAL_CHANNEL_OPTIONS
    case 'bepusdt':
      return BEPUSDT_CHANNEL_OPTIONS
    case 'okpay':
      return OKPAY_CHANNEL_OPTIONS
    default:
      return FALLBACK_CHANNEL_OPTIONS
  }
}

/** tokenpay / bepusdt / epusdt / dujiaopay derive channel_type themselves — no channel select. */
export const showsChannelTypeSelect = (provider: string) => !['tokenpay', 'bepusdt', 'epusdt', 'dujiaopay'].includes(provider)

export type OrderMode = 'transaction' | 'cashier'

export interface OrderModes {
  bepusdt: string
  dujiaopay: string
}

const QR_REDIRECT = ['qr', 'redirect']

/** Allowed interaction modes; the first one is the default. */
export const interactionModesFor = (provider: string, channelType: string, orderModes: OrderModes): string[] => {
  if (provider === 'bepusdt') return orderModes.bepusdt === 'cashier' ? ['redirect'] : QR_REDIRECT
  if (provider === 'epusdt') return ['redirect']
  if (provider === 'dujiaopay') return orderModes.dujiaopay === 'cashier' ? ['redirect'] : QR_REDIRECT
  if (provider === 'official') {
    if (channelType === 'paypal' || channelType === 'stripe') return ['redirect']
    if (channelType === 'alipay') return ['qr', 'wap', 'page']
  }
  return QR_REDIRECT
}

export const interactionModeOptions = (modes: string[]): KeyOption[] => modes.map((m) => ({ value: m, labelKey: im(m) }))

/** Keep `current` when allowed, otherwise fall back to the first allowed mode. */
export const pickInteractionMode = (current: string, allowed: string[]) => (allowed.includes(current) ? current : allowed[0] || 'qr')

/** DujiaoPay token ids look like `<chain>-<token>` (tron-usdt, x-layer-usdt0). */
export const isDujiaopayTokenId = (value: string) => /^[a-z0-9]+(?:-[a-z0-9]+)+$/i.test(value.trim())

/** New channel_type after the provider select changes (original provider_type watcher). */
export const channelTypeForProvider = (provider: string, current: string, dujiaopayOrderMode: string): string => {
  switch (provider) {
    case 'epay':
    case 'official':
    case 'okpay': {
      const allowed = channelOptionsFor(provider).map((o) => o.value)
      return allowed.includes(current) ? current : allowed[0] || current
    }
    case 'bepusdt':
      return 'bepusdt'
    case 'epusdt':
      return 'epusdt'
    case 'tokenpay':
      return 'usdt'
    case 'dujiaopay':
      if (dujiaopayOrderMode === 'cashier') return 'dujiaopay'
      // keep a typed-in token id (chain-token, e.g. base-usdc); anything left over from another
      // provider (alipay, bepusdt, usdt, dujiaopay…) falls back to the default token id
      return isDujiaopayTokenId(current) ? current : DUJIAOPAY_DEFAULT_TOKEN_ID
    default:
      return current
  }
}

// ---------------------------------------------------------------------------------------------
// Per-provider config forms
// ---------------------------------------------------------------------------------------------

export interface EpayConfig {
  epay_version: string
  gateway_url: string
  merchant_id: string
  merchant_key: string
  private_key: string
  platform_public_key: string
  notify_url: string
  return_url: string
  target_currency: string
  exchange_rate: string
}
export interface PaypalConfig {
  client_id: string
  client_secret: string
  base_url: string
  return_url: string
  cancel_url: string
  webhook_id: string
  brand_name: string
  locale: string
  target_currency: string
  exchange_rate: string
}
export interface StripeConfig {
  secret_key: string
  publishable_key: string
  webhook_secret: string
  success_url: string
  cancel_url: string
  api_base_url: string
  /** comma separated in the form, array on the wire */
  payment_method_types: string
  target_currency: string
  exchange_rate: string
}
export interface AlipayConfig {
  app_id: string
  private_key: string
  alipay_public_key: string
  gateway_url: string
  notify_url: string
  return_url: string
  sign_type: string
  app_cert_sn: string
  alipay_root_cert_sn: string
  target_currency: string
  exchange_rate: string
}
export const WECHAT_VERIFICATION_MODES = ['platform_certificate', 'wechatpay_public_key', 'combined'] as const
export interface WechatConfig {
  appid: string
  mchid: string
  merchant_serial_no: string
  merchant_private_key: string
  api_v3_key: string
  verification_mode: string
  wechatpay_public_key_id: string
  wechatpay_public_key: string
  notify_url: string
  h5_redirect_url: string
  h5_type: string
  h5_wap_url: string
  h5_wap_name: string
  target_currency: string
  exchange_rate: string
}
export interface BepusdtConfig {
  gateway_url: string
  auth_token: string
  order_mode: string
  trade_type: string
  currencies: string
  fiat: string
  notify_url: string
  return_url: string
}
export interface EpusdtConfig {
  gateway_url: string
  pid: string
  secret_key: string
  order_mode: string
  token: string
  network: string
  currency: string
  notify_url: string
  return_url: string
}
export interface TokenpayConfig {
  gateway_url: string
  notify_secret: string
  currency: string
  notify_url: string
  redirect_url: string
  base_currency: string
}
export interface OkpayConfig {
  gateway_url: string
  merchant_id: string
  merchant_token: string
  exchange_rate: string
  return_url: string
  callback_url: string
  display_name: string
}
export interface DujiaopayConfig {
  api_base_url: string
  api_key_id: string
  api_secret: string
  webhook_secret: string
  order_mode: string
  allowed_methods: string
  fiat_currency: string
  success_url: string
  cancel_url: string
}

export interface ProviderConfigs {
  epay: EpayConfig
  paypal: PaypalConfig
  stripe: StripeConfig
  alipay: AlipayConfig
  wechat: WechatConfig
  bepusdt: BepusdtConfig
  epusdt: EpusdtConfig
  tokenpay: TokenpayConfig
  okpay: OkpayConfig
  dujiaopay: DujiaopayConfig
}
export type ConfigSection = keyof ProviderConfigs

const DEFAULT_NOTIFY_URL = 'https://api.yourdomain.com/api/v1/payments/callback'
const DEFAULT_RETURN_URL = 'https://yourdomain.com/pay'
const PAYPAL_SANDBOX = 'https://api-m.sandbox.paypal.com'
const STRIPE_API = 'https://api.stripe.com'
const ALIPAY_GATEWAY = 'https://openapi.alipay.com/gateway.do'
const OKPAY_GATEWAY = 'https://api.okaypay.me/shop'
const DUJIAOPAY_API = 'https://www.dujiaopay.com'

/** Values used for a fresh "create" dialog (original reset*Config functions). */
export const defaultConfigs = (): ProviderConfigs => ({
  epay: {
    epay_version: 'v2',
    gateway_url: '',
    merchant_id: '',
    merchant_key: '',
    private_key: '',
    platform_public_key: '',
    notify_url: '',
    return_url: '',
    target_currency: '',
    exchange_rate: '',
  },
  paypal: {
    client_id: '',
    client_secret: '',
    base_url: PAYPAL_SANDBOX,
    return_url: '',
    cancel_url: '',
    webhook_id: '',
    brand_name: '',
    locale: '',
    target_currency: '',
    exchange_rate: '',
  },
  stripe: {
    secret_key: '',
    publishable_key: '',
    webhook_secret: '',
    success_url: '',
    cancel_url: '',
    api_base_url: STRIPE_API,
    payment_method_types: 'card',
    target_currency: '',
    exchange_rate: '',
  },
  alipay: {
    app_id: '',
    private_key: '',
    alipay_public_key: '',
    gateway_url: ALIPAY_GATEWAY,
    notify_url: '',
    return_url: '',
    sign_type: 'RSA2',
    app_cert_sn: '',
    alipay_root_cert_sn: '',
    target_currency: '',
    exchange_rate: '',
  },
  wechat: {
    appid: '',
    mchid: '',
    merchant_serial_no: '',
    merchant_private_key: '',
    api_v3_key: '',
    verification_mode: 'platform_certificate',
    wechatpay_public_key_id: '',
    wechatpay_public_key: '',
    notify_url: '',
    h5_redirect_url: '',
    h5_type: 'WAP',
    h5_wap_url: '',
    h5_wap_name: '',
    target_currency: '',
    exchange_rate: '',
  },
  bepusdt: {
    gateway_url: '',
    auth_token: '',
    order_mode: 'transaction',
    trade_type: 'usdt.trc20',
    currencies: '',
    fiat: 'CNY',
    notify_url: DEFAULT_NOTIFY_URL,
    return_url: DEFAULT_RETURN_URL,
  },
  epusdt: {
    gateway_url: '',
    pid: '',
    secret_key: '',
    order_mode: 'transaction',
    token: 'usdt',
    network: 'tron',
    currency: 'cny',
    notify_url: DEFAULT_NOTIFY_URL,
    return_url: DEFAULT_RETURN_URL,
  },
  tokenpay: {
    gateway_url: '',
    notify_secret: '',
    currency: 'USDT',
    notify_url: DEFAULT_NOTIFY_URL,
    redirect_url: DEFAULT_RETURN_URL,
    base_currency: 'CNY',
  },
  okpay: {
    gateway_url: OKPAY_GATEWAY,
    merchant_id: '',
    merchant_token: '',
    exchange_rate: '1',
    return_url: DEFAULT_RETURN_URL,
    callback_url: DEFAULT_NOTIFY_URL,
    display_name: '',
  },
  dujiaopay: {
    api_base_url: DUJIAOPAY_API,
    api_key_id: '',
    api_secret: '',
    webhook_secret: '',
    order_mode: 'transaction',
    allowed_methods: '',
    fiat_currency: 'CNY',
    success_url: DEFAULT_RETURN_URL,
    cancel_url: DEFAULT_RETURN_URL,
  },
})

type Raw = Record<string, unknown>

/** `String(raw[key] || fallback)` like the original apply* functions (falsy → fallback). */
const str = (raw: Raw, key: string, fallback = '') => {
  const v = raw[key]
  return v === undefined || v === null || v === '' || v === 0 || v === false ? fallback : String(v)
}
const cashierOr = (raw: Raw): OrderMode => (str(raw, 'order_mode', 'transaction') === 'cashier' ? 'cashier' : 'transaction')

/** Parse a stored config_json into every provider form (original apply*Config functions). */
export const applyConfigs = (raw: Raw): ProviderConfigs => {
  const version = str(raw, 'epay_version').toLowerCase()
  const methodTypes = Array.isArray(raw.payment_method_types)
    ? (raw.payment_method_types as unknown[]).map((item) => String(item ?? '').trim()).filter(Boolean)
    : []
  const verificationMode = str(raw, 'verification_mode', 'platform_certificate').trim().toLowerCase()
  const bepusdtMode = cashierOr(raw)
  const epusdtMode = cashierOr(raw)
  const dujiaoMode = cashierOr(raw)
  return {
    epay: {
      epay_version: version === 'v1' ? 'v1' : 'v2',
      gateway_url: str(raw, 'gateway_url'),
      merchant_id: str(raw, 'merchant_id'),
      merchant_key: str(raw, 'merchant_key'),
      private_key: str(raw, 'private_key'),
      platform_public_key: str(raw, 'platform_public_key'),
      notify_url: str(raw, 'notify_url'),
      return_url: str(raw, 'return_url'),
      target_currency: str(raw, 'target_currency'),
      exchange_rate: str(raw, 'exchange_rate'),
    },
    paypal: {
      client_id: str(raw, 'client_id'),
      client_secret: str(raw, 'client_secret'),
      base_url: str(raw, 'base_url', PAYPAL_SANDBOX),
      return_url: str(raw, 'return_url'),
      cancel_url: str(raw, 'cancel_url'),
      webhook_id: str(raw, 'webhook_id'),
      brand_name: str(raw, 'brand_name'),
      locale: str(raw, 'locale'),
      target_currency: str(raw, 'target_currency'),
      exchange_rate: str(raw, 'exchange_rate'),
    },
    stripe: {
      secret_key: str(raw, 'secret_key'),
      publishable_key: str(raw, 'publishable_key'),
      webhook_secret: str(raw, 'webhook_secret'),
      success_url: str(raw, 'success_url'),
      cancel_url: str(raw, 'cancel_url'),
      api_base_url: str(raw, 'api_base_url', STRIPE_API),
      payment_method_types: methodTypes.length > 0 ? methodTypes.join(',') : 'card',
      target_currency: str(raw, 'target_currency'),
      exchange_rate: str(raw, 'exchange_rate'),
    },
    alipay: {
      app_id: str(raw, 'app_id'),
      private_key: str(raw, 'private_key'),
      alipay_public_key: str(raw, 'alipay_public_key'),
      gateway_url: str(raw, 'gateway_url', ALIPAY_GATEWAY),
      notify_url: str(raw, 'notify_url'),
      return_url: str(raw, 'return_url'),
      sign_type: str(raw, 'sign_type', 'RSA2').toUpperCase(),
      app_cert_sn: str(raw, 'app_cert_sn'),
      alipay_root_cert_sn: str(raw, 'alipay_root_cert_sn'),
      target_currency: str(raw, 'target_currency'),
      exchange_rate: str(raw, 'exchange_rate'),
    },
    wechat: {
      appid: str(raw, 'appid'),
      mchid: str(raw, 'mchid'),
      merchant_serial_no: str(raw, 'merchant_serial_no'),
      merchant_private_key: str(raw, 'merchant_private_key'),
      api_v3_key: str(raw, 'api_v3_key'),
      verification_mode: (WECHAT_VERIFICATION_MODES as readonly string[]).includes(verificationMode) ? verificationMode : 'platform_certificate',
      wechatpay_public_key_id: str(raw, 'wechatpay_public_key_id'),
      wechatpay_public_key: str(raw, 'wechatpay_public_key'),
      notify_url: str(raw, 'notify_url'),
      h5_redirect_url: str(raw, 'h5_redirect_url'),
      h5_type: str(raw, 'h5_type', 'WAP').toUpperCase(),
      h5_wap_url: str(raw, 'h5_wap_url'),
      h5_wap_name: str(raw, 'h5_wap_name'),
      target_currency: str(raw, 'target_currency'),
      exchange_rate: str(raw, 'exchange_rate'),
    },
    bepusdt: {
      gateway_url: str(raw, 'gateway_url'),
      auth_token: str(raw, 'auth_token'),
      order_mode: bepusdtMode,
      trade_type: bepusdtMode === 'cashier' ? '' : str(raw, 'trade_type', 'usdt.trc20'),
      currencies: bepusdtMode === 'cashier' ? str(raw, 'currencies') : '',
      fiat: str(raw, 'fiat', 'CNY'),
      notify_url: str(raw, 'notify_url'),
      return_url: str(raw, 'return_url'),
    },
    epusdt: {
      gateway_url: str(raw, 'gateway_url'),
      pid: str(raw, 'pid'),
      secret_key: str(raw, 'secret_key'),
      order_mode: epusdtMode,
      token: epusdtMode === 'cashier' ? '' : str(raw, 'token', 'usdt'),
      network: epusdtMode === 'cashier' ? '' : str(raw, 'network', 'tron'),
      currency: str(raw, 'currency', 'cny'),
      notify_url: str(raw, 'notify_url'),
      return_url: str(raw, 'return_url'),
    },
    tokenpay: {
      gateway_url: str(raw, 'gateway_url'),
      notify_secret: str(raw, 'notify_secret'),
      currency: str(raw, 'currency', 'USDT'),
      notify_url: str(raw, 'notify_url'),
      redirect_url: str(raw, 'redirect_url'),
      base_currency: str(raw, 'base_currency', 'CNY'),
    },
    okpay: {
      gateway_url: str(raw, 'gateway_url', OKPAY_GATEWAY),
      merchant_id: str(raw, 'merchant_id'),
      merchant_token: str(raw, 'merchant_token'),
      exchange_rate: str(raw, 'exchange_rate', '1'),
      return_url: str(raw, 'return_url'),
      callback_url: str(raw, 'callback_url'),
      display_name: str(raw, 'display_name'),
    },
    dujiaopay: {
      api_base_url: str(raw, 'api_base_url', DUJIAOPAY_API),
      api_key_id: str(raw, 'api_key_id'),
      api_secret: str(raw, 'api_secret'),
      webhook_secret: str(raw, 'webhook_secret'),
      order_mode: dujiaoMode,
      allowed_methods: dujiaoMode === 'cashier' ? str(raw, 'allowed_methods') : '',
      fiat_currency: str(raw, 'fiat_currency', 'CNY').toUpperCase(),
      success_url: str(raw, 'success_url'),
      cancel_url: str(raw, 'cancel_url'),
    },
  }
}

/** Which dedicated config form belongs to a provider/channel pair (null → only the raw JSON). */
export const configSectionFor = (provider: string, channelType: string): ConfigSection | null => {
  if (provider === 'official') {
    return channelType === 'paypal' || channelType === 'stripe' || channelType === 'alipay' || channelType === 'wechat' ? channelType : null
  }
  return provider === 'epay' || provider === 'bepusdt' || provider === 'epusdt' || provider === 'tokenpay' || provider === 'okpay' || provider === 'dujiaopay'
    ? provider
    : null
}

const pickNonEmpty = (source: object, keys: string[]): Raw => {
  const out: Raw = {}
  const src = source as Record<string, unknown>
  for (const key of keys) {
    const trimmed = String(src[key] ?? '').trim()
    if (trimmed !== '') out[key] = trimmed
  }
  return out
}

export const buildEpayConfig = (c: EpayConfig): Raw =>
  pickNonEmpty(c, [
    'epay_version',
    'gateway_url',
    'merchant_id',
    'notify_url',
    'return_url',
    ...(c.epay_version === 'v1' ? ['merchant_key'] : ['private_key', 'platform_public_key']),
    'target_currency',
    'exchange_rate',
  ])

export const buildPaypalConfig = (c: PaypalConfig): Raw =>
  pickNonEmpty(c, ['client_id', 'client_secret', 'base_url', 'return_url', 'cancel_url', 'webhook_id', 'brand_name', 'locale', 'target_currency', 'exchange_rate'])

export const buildStripeConfig = (c: StripeConfig): Raw => {
  const out = pickNonEmpty(c, ['secret_key', 'publishable_key', 'webhook_secret', 'success_url', 'cancel_url', 'api_base_url'])
  const methodTypes = String(c.payment_method_types || '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
  if (methodTypes.length > 0) out.payment_method_types = methodTypes
  return { ...out, ...pickNonEmpty(c, ['target_currency', 'exchange_rate']) }
}

export const buildAlipayConfig = (c: AlipayConfig): Raw =>
  pickNonEmpty(c, [
    'app_id',
    'private_key',
    'alipay_public_key',
    'gateway_url',
    'notify_url',
    'return_url',
    'sign_type',
    'app_cert_sn',
    'alipay_root_cert_sn',
    'target_currency',
    'exchange_rate',
  ])

export const buildWechatConfig = (c: WechatConfig): Raw =>
  pickNonEmpty(c, [
    'appid',
    'mchid',
    'merchant_serial_no',
    'merchant_private_key',
    'api_v3_key',
    'verification_mode',
    ...(c.verification_mode !== 'platform_certificate' ? ['wechatpay_public_key_id', 'wechatpay_public_key'] : []),
    'notify_url',
    'h5_redirect_url',
    'h5_type',
    'h5_wap_url',
    'h5_wap_name',
    'target_currency',
    'exchange_rate',
  ])

export const buildBepusdtConfig = (c: BepusdtConfig): Raw => {
  const orderMode = c.order_mode === 'cashier' ? 'cashier' : 'transaction'
  const out: Raw = {
    gateway_url: String(c.gateway_url || '').trim(),
    auth_token: String(c.auth_token || '').trim(),
    order_mode: orderMode,
    notify_url: String(c.notify_url || '').trim() || DEFAULT_NOTIFY_URL,
    return_url: String(c.return_url || '').trim() || DEFAULT_RETURN_URL,
  }
  const tradeType = String(c.trade_type || '').trim()
  if (orderMode !== 'cashier' && tradeType !== '') out.trade_type = tradeType
  const currencies = String(c.currencies || '')
    .trim()
    .toUpperCase()
    .replace(/\s+/g, '')
  if (orderMode === 'cashier' && currencies !== '') out.currencies = currencies
  const fiat = String(c.fiat || '').trim()
  if (fiat !== '') out.fiat = fiat
  return out
}

export const buildEpusdtConfig = (c: EpusdtConfig): Raw => {
  const orderMode = c.order_mode === 'cashier' ? 'cashier' : 'transaction'
  return {
    gateway_url: String(c.gateway_url || '').trim(),
    pid: String(c.pid || '').trim(),
    secret_key: String(c.secret_key || '').trim(),
    order_mode: orderMode,
    token: orderMode === 'cashier' ? '' : String(c.token || '').trim().toLowerCase(),
    network: orderMode === 'cashier' ? '' : String(c.network || '').trim().toLowerCase(),
    currency: String(c.currency || '').trim().toLowerCase(),
    notify_url: String(c.notify_url || '').trim(),
    return_url: String(c.return_url || '').trim(),
  }
}

export const buildTokenpayConfig = (c: TokenpayConfig): Raw =>
  pickNonEmpty(c, ['gateway_url', 'notify_secret', 'currency', 'notify_url', 'redirect_url', 'base_currency'])

export const buildOkpayConfig = (c: OkpayConfig, channelType: string): Raw => {
  const out = pickNonEmpty(c, ['gateway_url', 'merchant_id', 'merchant_token', 'exchange_rate', 'return_url', 'callback_url', 'display_name'])
  if (channelType === 'usdt') out.coin = 'USDT'
  else if (channelType === 'trx') out.coin = 'TRX'
  return out
}

export const buildDujiaopayConfig = (c: DujiaopayConfig, channelType: string): Raw => {
  const out = pickNonEmpty({ ...c, fiat_currency: String(c.fiat_currency || '').toUpperCase() }, [
    'api_base_url',
    'api_key_id',
    'api_secret',
    'webhook_secret',
    'fiat_currency',
    'success_url',
    'cancel_url',
  ])
  out.order_mode = c.order_mode === 'cashier' ? 'cashier' : 'transaction'
  if (out.order_mode === 'cashier') {
    const methods = String(c.allowed_methods || '')
      .split(',')
      .map((m) => m.trim().toLowerCase())
      .filter(Boolean)
    if (methods.length > 0) out.allowed_methods = methods.join(',')
  } else if (channelType) {
    out.token_id = String(channelType).trim().toLowerCase()
  }
  return out
}

// ---------------------------------------------------------------------------------------------
// Order-mode side effects (original order_mode watchers). They mutate the given config.
// ---------------------------------------------------------------------------------------------

export const applyBepusdtOrderMode = (c: BepusdtConfig) => {
  if (c.order_mode === 'cashier') {
    c.trade_type = ''
  } else {
    c.currencies = ''
    if (String(c.trade_type || '').trim() === '') c.trade_type = 'usdt.trc20'
  }
}

export const applyEpusdtOrderMode = (c: EpusdtConfig) => {
  if (c.order_mode === 'cashier') {
    c.token = ''
    c.network = ''
    return
  }
  if (String(c.token || '').trim() === '') c.token = 'usdt'
  if (String(c.network || '').trim() === '') c.network = 'tron'
}

/** Returns the new channel_type for DujiaoPay after its order mode changed. */
export const applyDujiaopayOrderMode = (c: DujiaopayConfig, channelType: string): string => {
  if (c.order_mode === 'cashier') return 'dujiaopay'
  c.allowed_methods = ''
  return String(channelType || '').trim() === '' || channelType === 'dujiaopay' ? DUJIAOPAY_DEFAULT_TOKEN_ID : channelType
}

// ---------------------------------------------------------------------------------------------
// Common form + payload
// ---------------------------------------------------------------------------------------------

export interface ChannelForm {
  name: string
  icon: string
  provider_type: string
  channel_type: string
  interaction_mode: string
  fee_rate: string | number
  fixed_fee: string | number
  min_amount: string | number
  max_amount: string | number
  hide_amount_out_range: boolean
  payment_types: string[]
  payment_roles: string[]
  member_levels: number[]
  /** advanced raw JSON text */
  config_json: string
  is_active: boolean
  sort_order: string | number
}

export const defaultChannelForm = (): ChannelForm => ({
  name: '',
  icon: '',
  provider_type: 'epay',
  channel_type: 'alipay',
  interaction_mode: 'qr',
  fee_rate: '0',
  fixed_fee: '0',
  min_amount: '0',
  max_amount: '0',
  hide_amount_out_range: false,
  payment_types: [],
  payment_roles: [],
  member_levels: [],
  config_json: '',
  is_active: true,
  sort_order: 10,
})

const moneyText = (v: unknown) => (v !== undefined && v !== null ? String(v) : '0')

/** Map a loaded channel onto the dialog state (form + every provider config). */
export const channelToState = (channel: AdminPaymentChannel): { form: ChannelForm; configs: ProviderConfigs } => {
  const raw = channel.config_json && typeof channel.config_json === 'object' ? channel.config_json : null
  const form: ChannelForm = {
    name: channel.name,
    icon: channel.icon || '',
    provider_type: channel.provider_type,
    channel_type: channel.channel_type,
    interaction_mode: channel.interaction_mode,
    fee_rate: moneyText(channel.fee_rate),
    fixed_fee: moneyText(channel.fixed_fee),
    min_amount: moneyText(channel.min_amount),
    max_amount: moneyText(channel.max_amount),
    hide_amount_out_range: Boolean(channel.hide_amount_out_range),
    payment_types: Array.isArray(channel.payment_types) ? [...channel.payment_types] : [],
    payment_roles: Array.isArray(channel.payment_roles) ? [...channel.payment_roles] : [],
    member_levels: Array.isArray(channel.member_levels) ? [...channel.member_levels] : [],
    config_json: channel.config_json ? JSON.stringify(channel.config_json, null, 2) : '',
    is_active: !!channel.is_active,
    sort_order: channel.sort_order || 0,
  }
  if (raw && channel.provider_type === 'okpay' && !String(form.channel_type || '').trim()) {
    form.channel_type = resolveOkpayChannelTypeFromConfig(raw)
  }
  form.interaction_mode = pickInteractionMode(
    form.interaction_mode,
    interactionModesFor(form.provider_type, form.channel_type, {
      bepusdt: raw ? cashierOr(raw) : 'transaction',
      dujiaopay: raw ? cashierOr(raw) : 'transaction',
    }),
  )
  return { form, configs: raw ? applyConfigs(raw) : defaultConfigs() }
}

/** channel_type actually sent to the backend. */
export const resolvePayloadChannelType = (form: Pick<ChannelForm, 'provider_type' | 'channel_type'>, configs: Pick<ProviderConfigs, 'dujiaopay'>) => {
  switch (form.provider_type) {
    case 'tokenpay':
      return 'usdt'
    case 'bepusdt':
      return 'bepusdt'
    case 'epusdt':
      return 'epusdt'
    case 'dujiaopay':
      return configs.dujiaopay.order_mode === 'cashier' ? 'dujiaopay' : String(form.channel_type || '').trim().toLowerCase()
    default:
      return form.channel_type
  }
}

export type BuildPayloadResult = { ok: true; payload: Record<string, unknown> } | { ok: false; error: 'invalidConfig' }

const withoutKeys = (obj: Raw, keys: string[]) => {
  const out = { ...obj }
  for (const k of keys) delete out[k]
  return out
}

/**
 * Merge the advanced raw JSON with the dedicated provider form (form wins), keeping explicit
 * `null`s from the raw JSON (they mean "clear this secret" rather than "keep masked value").
 */
export const buildConfigJson = (form: ChannelForm, configs: ProviderConfigs): { ok: true; config: Raw } | { ok: false } => {
  let config: Raw = {}
  let explicitNulls: string[] = []
  if (form.config_json && form.config_json.trim() !== '') {
    try {
      const parsed: unknown = JSON.parse(form.config_json)
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        config = { ...(parsed as Raw) }
        explicitNulls = Object.entries(config)
          .filter(([, v]) => v === null)
          .map(([k]) => k)
      }
    } catch {
      return { ok: false }
    }
  }

  const section = configSectionFor(form.provider_type, form.channel_type)
  const fx = ['target_currency', 'exchange_rate']
  switch (section) {
    case 'epay':
      config = {
        ...withoutKeys(config, [...(configs.epay.epay_version === 'v1' ? ['private_key', 'platform_public_key'] : ['merchant_key']), ...fx]),
        ...buildEpayConfig(configs.epay),
      }
      break
    case 'paypal':
      config = { ...withoutKeys(config, fx), ...buildPaypalConfig(configs.paypal) }
      break
    case 'stripe':
      config = { ...withoutKeys(config, fx), ...buildStripeConfig(configs.stripe) }
      break
    case 'alipay':
      config = { ...withoutKeys(config, fx), ...buildAlipayConfig(configs.alipay) }
      break
    case 'wechat':
      config = { ...withoutKeys(config, fx), ...buildWechatConfig(configs.wechat) }
      break
    case 'bepusdt':
      config = {
        ...withoutKeys(config, ['currencies', ...(configs.bepusdt.order_mode === 'cashier' ? ['trade_type'] : [])]),
        ...buildBepusdtConfig(configs.bepusdt),
      }
      break
    case 'epusdt':
      config = { ...config, ...buildEpusdtConfig(configs.epusdt) }
      break
    case 'tokenpay':
      config = { ...config, ...buildTokenpayConfig(configs.tokenpay) }
      break
    case 'okpay':
      config = { ...withoutKeys(config, ['exchange_rate']), ...buildOkpayConfig(configs.okpay, form.channel_type) }
      break
    case 'dujiaopay':
      config = {
        ...withoutKeys(config, ['chain', 'allowed_methods', ...(configs.dujiaopay.order_mode === 'cashier' ? ['token_id'] : [])]),
        ...buildDujiaopayConfig(configs.dujiaopay, form.channel_type),
      }
      break
    default:
      break
  }
  for (const key of explicitNulls) config[key] = null
  return { ok: true, config }
}

const trimOr0 = (v: string | number) => String(v === '' || v === null || v === undefined ? '0' : v).trim() || '0'

/** Build the create/update request body. */
export const buildChannelPayload = (form: ChannelForm, configs: ProviderConfigs): BuildPayloadResult => {
  const built = buildConfigJson(form, configs)
  if (!built.ok) return { ok: false, error: 'invalidConfig' }
  return {
    ok: true,
    payload: {
      name: form.name,
      icon: form.icon || '',
      provider_type: form.provider_type,
      channel_type: resolvePayloadChannelType(form, configs),
      interaction_mode: form.interaction_mode,
      fee_rate: trimOr0(form.fee_rate),
      fixed_fee: trimOr0(form.fixed_fee),
      min_amount: trimOr0(form.min_amount),
      max_amount: trimOr0(form.max_amount),
      hide_amount_out_range: form.hide_amount_out_range,
      payment_types: [...form.payment_types],
      payment_roles: [...form.payment_roles],
      member_levels: [...form.member_levels],
      config_json: built.config,
      is_active: form.is_active,
      sort_order: Number(form.sort_order) || 0,
    },
  }
}
