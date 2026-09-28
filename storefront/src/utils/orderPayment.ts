import type { Fulfillment, ManualFormSchema, Order, OrderItem, PaymentChannel } from '@/api/types'
import { amountToCents, basisPointsToPercent, centsToAmount, rateToBasisPoints } from './money'
import { localizedText } from './localized'

type ChannelLike = Pick<PaymentChannel, 'id' | 'provider_type' | 'channel_type' | 'min_amount' | 'max_amount' | 'hide_amount_out_range'>

/** epay only exposes wechat/wxpay/alipay/qqpay sub types (same as original). */
export const filterSupportedChannels = <T extends ChannelLike>(list: T[] | null | undefined): T[] => {
  if (!Array.isArray(list)) return []
  return list.filter((channel) => {
    const provider = String(channel?.provider_type || '').toLowerCase()
    const type = String(channel?.channel_type || '').toLowerCase()
    if (provider === 'epay') return ['wechat', 'wxpay', 'alipay', 'qqpay'].includes(type)
    return true
  })
}

/**
 * Intersection of per-product `payment_channel_ids` (products without a list
 * don't constrain). `null` = no restriction; `[]` = nothing allowed.
 */
export const intersectAllowedChannelIds = (lists: Array<number[] | null | undefined>): number[] | null => {
  let acc: number[] | null = null
  for (const ids of lists) {
    if (!Array.isArray(ids) || ids.length === 0) continue
    const set = new Set(ids.map(Number))
    acc = acc === null ? [...set] : acc.filter((id) => set.has(id))
  }
  return acc
}

export const restrictChannels = <T extends ChannelLike>(channels: T[], allowed: number[] | null): T[] => {
  if (allowed === null) return channels
  if (allowed.length === 0) return []
  const set = new Set(allowed)
  return channels.filter((c) => set.has(Number(c.id)))
}

export interface ChannelLimitMeta {
  minCents: number | null
  maxCents: number | null
  hasMin: boolean
  hasMax: boolean
  hideAmountOutRange: boolean
}

export const channelLimitMeta = (channel?: Partial<ChannelLike> | null): ChannelLimitMeta => {
  const minCents = amountToCents(String(channel?.min_amount ?? ''))
  const maxCents = amountToCents(String(channel?.max_amount ?? ''))
  return {
    minCents,
    maxCents,
    hasMin: minCents !== null && minCents > 0,
    hasMax: maxCents !== null && maxCents > 0,
    hideAmountOutRange: Boolean(channel?.hide_amount_out_range),
  }
}

/** Shown-but-disabled when the online amount is outside [min, max] and hiding is off. */
export const isChannelDisabledForAmount = (channel: Partial<ChannelLike> | null | undefined, onlineCents: number, requiresOnline: boolean): boolean => {
  if (!requiresOnline || onlineCents <= 0) return false
  const meta = channelLimitMeta(channel)
  if (!meta.hasMin && !meta.hasMax) return false
  const below = meta.hasMin && meta.minCents !== null && onlineCents < meta.minCents
  const above = meta.hasMax && meta.maxCents !== null && onlineCents > meta.maxCents
  if (!below && !above) return false
  return !meta.hideAmountOutRange
}

export const formatFeeRate = (rate: unknown): string => {
  const bp = rateToBasisPoints(rate)
  return bp === null ? '0.00%' : `${basisPointsToPercent(bp)}%`
}

export const isCustomerSurcharge = (policy: unknown) => {
  const p = String(policy || '').trim().toLowerCase()
  return p === 'customer_surcharge' || p === 'legacy_customer_surcharge'
}

/** Wallet deduction and online remainder (cents). */
export const splitWalletPayment = (totalAmount: unknown, walletBalance: unknown, useBalance: boolean) => {
  const total = amountToCents(totalAmount)
  if (total === null) return { walletCents: 0, onlineCents: 0 }
  const balance = amountToCents(walletBalance)
  const walletCents = useBalance && balance !== null ? Math.max(0, Math.min(balance, total)) : 0
  return { walletCents, onlineCents: Math.max(total - walletCents, 0) }
}

// ---------------------------------------------------------------- route query helpers
export type QueryRecord = Record<string, unknown>

const firstQueryString = (value: unknown): string => {
  if (Array.isArray(value)) {
    for (const item of value) {
      const text = String(item ?? '').trim()
      if (text) return text
    }
    return ''
  }
  return String(value ?? '').trim()
}

/** Reads a query value tolerating `amp;` prefixed keys and case (gateway returns). */
export const readQueryValue = (query: QueryRecord, key: string): string => {
  const lower = key.trim().toLowerCase()
  if (!lower) return ''
  for (const candidate of [key, lower, `amp;${key}`, `amp;${lower}`]) {
    const v = firstQueryString(query[candidate])
    if (v) return v
  }
  for (const [rawKey, rawValue] of Object.entries(query)) {
    const cleaned = rawKey.trim().toLowerCase().replace(/^(amp;)+/, '')
    if (cleaned !== lower) continue
    const v = firstQueryString(rawValue)
    if (v) return v
  }
  return ''
}

export const readQueryFlag = (query: QueryRecord, key: string) => ['1', 'true', 'yes'].includes(readQueryValue(query, key).toLowerCase())

export const PAYMENT_RETURN_MARKERS = [
  'epay_return',
  'alipay_return',
  'wechat_return',
  'epusdt_return',
  'bepusdt_return',
  'tokenpay_return',
  'okpay_return',
  'pp_return',
  'stripe_return',
] as const

/** Recharge number from a gateway return (`recharge_no`, or `order_no` starting with WR). */
export const resolveRechargeReturnNo = (query: QueryRecord): string => {
  const rechargeNo = readQueryValue(query, 'recharge_no')
  if (rechargeNo) return rechargeNo
  const orderNo = readQueryValue(query, 'order_no')
  return /^WR/i.test(orderNo) ? orderNo : ''
}

export const isRechargeReturn = (query: QueryRecord): boolean =>
  readQueryValue(query, 'biz_type').toLowerCase() === 'recharge' || /^WR/i.test(resolveRechargeReturnNo(query))

// ---------------------------------------------------------------- crypto
export interface CryptoDetail {
  key: string
  labelKey: string
  value: string
  detail?: string
}

const CHAIN_LABELS: Record<string, string> = {
  tron: 'TRON',
  trc20: 'TRON',
  base: 'Base',
  ethereum: 'Ethereum',
  eth: 'Ethereum',
  bsc: 'BNB Smart Chain',
  polygon: 'Polygon',
}

export const formatCryptoChain = (value: string) => CHAIN_LABELS[value.trim().toLowerCase()] || value

export const cryptoTokenLabel = (tokenId: string) => {
  const id = tokenId.trim()
  if (!id) return ''
  const parts = id.split('-').filter(Boolean)
  return String(parts[parts.length - 1] || id).toUpperCase()
}

export const buildCryptoDetails = (payment: { wallet_address?: string; chain_amount?: string; chain?: string; token_id?: string } | null | undefined): CryptoDetail[] => {
  const out: CryptoDetail[] = []
  const tokenId = String(payment?.token_id || '').trim()
  const label = cryptoTokenLabel(tokenId)
  if (label) out.push({ key: 'token', labelKey: 'payment.cryptoToken', value: label, detail: tokenId.toUpperCase() === label ? '' : tokenId })
  const chain = String(payment?.chain || '').trim()
  if (chain) out.push({ key: 'chain', labelKey: 'payment.cryptoChain', value: formatCryptoChain(chain) })
  const amount = String(payment?.chain_amount || '').trim()
  if (amount) out.push({ key: 'amount', labelKey: 'payment.cryptoAmount', value: amount })
  const address = String(payment?.wallet_address || '').trim()
  if (address) out.push({ key: 'wallet_address', labelKey: 'payment.walletAddress', value: address })
  return out
}

/** Direct image source for a qr_code value (data URL / image URL), else null (render locally). */
export const qrImageSource = (content: string): string | null => {
  if (content.startsWith('data:image/')) return content
  if (/^https?:\/\/.+\.(png|jpe?g|gif|webp|svg)(\?.*)?$/i.test(content)) return content
  return null
}

// ---------------------------------------------------------------- order display
export const positiveCents = (amount: unknown) => {
  const c = amountToCents(amount)
  return c !== null && c > 0 ? c : 0
}

export const hasPositive = (amount: unknown) => positiveCents(amount) > 0

export const itemDiscountTotal = (item: OrderItem) =>
  centsToAmount(
    positiveCents(item.promotion_discount_amount) +
      positiveCents(item.wholesale_discount_amount) +
      positiveCents(item.member_discount_amount) +
      positiveCents(item.coupon_discount_amount),
  )

/** Item paid = total_price − coupon share (coupon is applied after line totals). */
export const itemPaidAmount = (item: OrderItem) => {
  const total = amountToCents(item.total_price) ?? 0
  return centsToAmount(Math.max(0, total - positiveCents(item.coupon_discount_amount)))
}

export const resolveChildStatus = (child: Order, parent: Order | null) => {
  const status = String(child.status || '').trim()
  const refunded = amountToCents(child.refunded_amount)
  if (refunded !== null && refunded > 0) {
    const total = amountToCents(child.total_amount)
    if (total !== null && total > 0 && refunded >= total) return 'refunded'
    return 'partially_refunded'
  }
  if (parent?.status === 'refunded' && status !== 'refunded') return 'refunded'
  return status
}

/** Only > 100 lines are truncated by the backend preview. */
export const FULFILLMENT_PREVIEW_LINES = 100
export const isFulfillmentTruncated = (f: Fulfillment | null | undefined) => Number(f?.payload_line_count || 0) > FULFILLMENT_PREVIEW_LINES

export const fulfillmentDeliveryLines = (f: Fulfillment | null | undefined): string[] => {
  const data = (f?.delivery_data || f?.logistics) as Record<string, unknown> | null | undefined
  const lines: string[] = []
  if (!data || typeof data !== 'object') return lines
  const note = String(data.note ?? '').trim()
  if (note) lines.push(note)
  const entries = Array.isArray(data.entries) ? (data.entries as unknown[]) : []
  for (const entry of entries) {
    const row = (entry && typeof entry === 'object' ? entry : {}) as Record<string, unknown>
    const key = String(row.key ?? '').trim()
    const value = String(row.value ?? '').trim()
    if (!key && !value) continue
    lines.push(!key ? value : !value ? key : `${key}: ${value}`)
  }
  return lines
}

const formatManualValue = (value: unknown): string => {
  if (Array.isArray(value)) return value.map(String).join(', ')
  if (value === null || value === undefined) return '-'
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value)
    } catch {
      return String(value)
    }
  }
  return String(value)
}

/** Submitted manual form rows, ordered by the schema snapshot then extra keys. */
export const manualSubmissionRows = (
  submission: Record<string, unknown> | null | undefined,
  schema: ManualFormSchema | null | undefined,
  locale: string,
): Array<{ key: string; label: string; value: string }> => {
  if (!submission || typeof submission !== 'object') return []
  const values = new Map(Object.entries(submission).filter(([k]) => k.trim() !== ''))
  if (values.size === 0) return []
  const rows: Array<{ key: string; label: string; value: string }> = []
  const fields = Array.isArray(schema?.fields) ? schema.fields : []
  for (const field of fields) {
    const key = String(field?.key || '').trim()
    if (!key || !values.has(key)) continue
    const rawLabel: unknown = field.label
    const label = typeof rawLabel === 'string' ? rawLabel.trim() : localizedText(field.label, locale)
    rows.push({ key, label: label || key, value: formatManualValue(values.get(key)) })
    values.delete(key)
  }
  values.forEach((value, key) => rows.push({ key, label: key, value: formatManualValue(value) }))
  return rows
}
