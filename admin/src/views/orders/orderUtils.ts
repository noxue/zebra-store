import type { AdminFulfillment, AdminOrder, AdminOrderItem, AdminPayment, Money } from '@/api/types'
import { getLocalizedText } from '@/utils/format'

// ---------------------------------------------------------------------------
// List helpers
// ---------------------------------------------------------------------------

export const ORDER_SORT_OPTIONS = [
  'created_at_desc',
  'created_at_asc',
  'updated_at_desc',
  'updated_at_asc',
  'total_amount_desc',
  'total_amount_asc',
] as const

/** `created_at_desc` -> `{ sort_by: 'created_at', sort_order: 'desc' }`; anything else -> `{}`. */
export const parseSortValue = (value: string): { sort_by?: string; sort_order?: 'asc' | 'desc' } => {
  if (!value || value === '__all__') return {}
  const lastUnderscore = value.lastIndexOf('_')
  if (lastUnderscore <= 0) return {}
  const order = value.slice(lastUnderscore + 1)
  if (order !== 'asc' && order !== 'desc') return {}
  return { sort_by: value.slice(0, lastUnderscore), sort_order: order }
}

/** First value of a route query entry, trimmed. */
export const toQueryText = (value: unknown): string => {
  if (Array.isArray(value)) return String(value[0] ?? '').trim()
  if (value === undefined || value === null) return ''
  return String(value).trim()
}

/** Positive integer id from a route query entry, or 0. */
export const toQueryId = (value: unknown): number => {
  const n = Number(toQueryText(value))
  return Number.isFinite(n) && n > 0 ? Math.trunc(n) : 0
}

export const normalizeMaxRefundDays = (raw: unknown): number => {
  const parsed = Number(raw)
  if (raw === null || raw === undefined || raw === '' || !Number.isFinite(parsed)) return 30
  const normalized = Math.trunc(parsed)
  if (normalized < 0) return 30
  if (normalized > 3650) return 3650
  return normalized
}

const LOCKED_STATUSES = ['completed', 'canceled', 'partially_refunded', 'refunded']

/**
 * Statuses an admin may pick in the generic status dropdown. Refund states are
 * reachable only through the refund flow, which moves money and stock (QA-A02).
 */
export const MANUAL_REFUND_STATUSES = ['partially_refunded', 'refunded']

export const editableOrderStatuses = (all: readonly string[]): string[] => all.filter((s) => !MANUAL_REFUND_STATUSES.includes(s))

/** Permissions behind the order-list actions (QA-A17: hide what the role cannot do). */
export const ORDER_PERMISSIONS = {
  updateStatus: 'PATCH:/admin/orders/:id',
  fulfill: 'POST:/admin/fulfillments',
  /** `order_config.max_refund_days` is read through the generic settings endpoint. */
  readSettings: 'GET:/admin/settings',
} as const

export type OrderActionFlags = Record<keyof typeof ORDER_PERMISSIONS, boolean>

export const orderActionFlags = (can: (permission: string) => boolean): OrderActionFlags => ({
  updateStatus: can(ORDER_PERMISSIONS.updateStatus),
  fulfill: can(ORDER_PERMISSIONS.fulfill),
  readSettings: can(ORDER_PERMISSIONS.readSettings),
})

export const canUpdateStatus = (order: AdminOrder | null | undefined) => !!order && !LOCKED_STATUSES.includes(order.status)

export const canCreateFulfillment = (order: AdminOrder | null | undefined) => {
  if (!order) return false
  if (order.fulfillment) return false
  if (order.parent_id == null && Array.isArray(order.children) && order.children.length > 0) return false
  if (Array.isArray(order.items) && order.items.length > 0 && !order.items.every((item) => item.fulfillment_type === 'manual')) {
    return false
  }
  return order.status === 'paid' || order.status === 'fulfilling'
}

export const canCreateChildFulfillment = (order: AdminOrder | null | undefined) => {
  if (!canCreateFulfillment(order)) return false
  if (!order?.items?.length) return false
  return order.items.every((item) => item.fulfillment_type === 'manual')
}

// ---------------------------------------------------------------------------
// Money helpers
// ---------------------------------------------------------------------------

export const parseMoneyValue = (value?: Money | null): number => {
  if (value === null || value === undefined || value === '') return 0
  const parsed = Number(value)
  return Number.isNaN(parsed) ? 0 : parsed
}

const positive = (value?: Money | null) => {
  const n = Number(value)
  return Number.isFinite(n) && n > 0 ? n : 0
}

const round2 = (n: number) => Number(n.toFixed(2))

export const itemRevenueAmount = (item: AdminOrderItem) => parseMoneyValue(item.total_price) - parseMoneyValue(item.coupon_discount_amount)

export const itemBaseProfit = (item: AdminOrderItem) => itemRevenueAmount(item) - parseMoneyValue(item.cost_price) * (item.quantity || 1)

export const itemDiscountTotal = (item: AdminOrderItem) =>
  round2(
    positive(item.promotion_discount_amount) +
      positive(item.member_discount_amount) +
      positive(item.wholesale_discount_amount) +
      positive(item.coupon_discount_amount),
  )

export const itemPaidAmount = (item: AdminOrderItem) => round2(Math.max(0, positive(item.total_price) - positive(item.coupon_discount_amount)))

const toCents = (amount: number) => Math.round(amount * 100)
const fromCents = (cents: number) => Number((cents / 100).toFixed(2))

/** Distribute the order's refunded amount across items, weighted by item revenue (cent-exact). */
export const buildRefundAllocation = (order: AdminOrder | null | undefined): number[] => {
  const items = order?.items ?? []
  if (!order || items.length === 0) return []
  const refundedCents = toCents(Math.max(parseMoneyValue(order.refunded_amount), 0))
  if (refundedCents <= 0) return items.map(() => 0)

  const weights = items.map((item) => Math.max(itemRevenueAmount(item), 0))
  const totalWeight = weights.reduce((sum, v) => sum + v, 0)
  if (totalWeight <= 0) {
    const base = Math.floor(refundedCents / items.length)
    let remainder = refundedCents - base * items.length
    return items.map(() => {
      const extra = remainder > 0 ? 1 : 0
      if (remainder > 0) remainder -= 1
      return fromCents(base + extra)
    })
  }
  let assigned = 0
  return items.map((_, index) => {
    if (index === items.length - 1) return fromCents(Math.max(refundedCents - assigned, 0))
    const share = Math.floor((refundedCents * (weights[index] ?? 0)) / totalWeight)
    assigned += share
    return fromCents(share)
  })
}

export const itemRefundAmount = (order: AdminOrder | null | undefined, index: number) => buildRefundAllocation(order)[index] ?? 0

export const itemProfit = (order: AdminOrder | null | undefined, item: AdminOrderItem, index: number) =>
  round2(itemBaseProfit(item) - itemRefundAmount(order, index))

export const orderPaymentFee = (order: AdminOrder | null | undefined) => {
  if (!order?.payments?.length) return 0
  const fee = order.payments.reduce((sum, p) => {
    if (p.status !== 'success' || p.fee_policy !== 'merchant_absorbed') return sum
    return sum + Math.max(parseMoneyValue(p.fee_amount), 0)
  }, 0)
  return round2(fee)
}

export const orderProfit = (order: AdminOrder | null | undefined) => {
  if (!order?.items?.length) return 0
  const itemTotal = order.items.reduce((sum, item, i) => sum + itemProfit(order, item, i), 0)
  return round2(itemTotal - orderPaymentFee(order))
}

export const formatFeeRate = (payment: Pick<AdminPayment, 'fee_rate' | 'fixed_fee'>) => {
  let display = '-'
  const { fee_rate: feeRate, fixed_fee: fixedFee } = payment
  if (feeRate !== undefined && feeRate !== null && feeRate !== '') {
    const rate = Number(feeRate)
    if (!Number.isNaN(rate)) display = `${rate.toFixed(2)}%`
  }
  if (fixedFee !== undefined && fixedFee !== null && fixedFee !== '') {
    const fixed = Number(fixedFee)
    if (!Number.isNaN(fixed) && fixed > 0) display = display === '-' ? fixed.toFixed(2) : `${display} + ${fixed.toFixed(2)}`
  }
  return display
}

// ---------------------------------------------------------------------------
// Refund rules
// ---------------------------------------------------------------------------

export const isPaidOrder = (order: AdminOrder | null | undefined) => !!order?.paid_at

export const isOrderRefundWindowExpired = (order: AdminOrder | null | undefined, maxRefundDays: number, nowMs = Date.now()) => {
  if (!order || !isPaidOrder(order)) return false
  const days = normalizeMaxRefundDays(maxRefundDays)
  if (days === 0) return false
  const baseAtRaw = order.paid_at || order.created_at
  if (!baseAtRaw) return false
  const baseAtMs = Date.parse(baseAtRaw)
  if (Number.isNaN(baseAtMs) || nowMs <= baseAtMs) return false
  return (nowMs - baseAtMs) / 86_400_000 > days
}

export const refundableAmountValue = (order: AdminOrder | null | undefined) => {
  if (!order || !isPaidOrder(order)) return 0
  const value = parseMoneyValue(order.total_amount) - parseMoneyValue(order.refunded_amount)
  if (!Number.isFinite(value)) return 0
  return round2(Math.max(value, 0))
}

export const canManualRefund = (order: AdminOrder | null | undefined, maxRefundDays: number) =>
  !!order && isPaidOrder(order) && !isOrderRefundWindowExpired(order, maxRefundDays) && refundableAmountValue(order) > 0

export const canRefundToWallet = (order: AdminOrder | null | undefined, maxRefundDays: number) => !!order?.user_id && canManualRefund(order, maxRefundDays)

export const shouldShowRefundCard = (order: AdminOrder | null | undefined, maxRefundDays: number) =>
  !!order && isPaidOrder(order) && order.status !== 'canceled' && !isOrderRefundWindowExpired(order, maxRefundDays)

export type RefundAmountCheck = 'ok' | 'invalid' | 'exceeded'

export const checkRefundAmount = (raw: string, refundable: number): RefundAmountCheck => {
  const amount = raw.trim()
  const value = Number(amount)
  if (!amount || Number.isNaN(value) || value <= 0) return 'invalid'
  if (value > refundable) return 'exceeded'
  return 'ok'
}

// ---------------------------------------------------------------------------
// Fulfillment / manual form rendering
// ---------------------------------------------------------------------------

export const isFulfillmentTruncated = (fulfillment?: AdminFulfillment | null) => (fulfillment?.payload_line_count ?? 0) > 100

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v)

export const fulfillmentDeliveryLines = (fulfillment?: AdminFulfillment | null): string[] => {
  const lines: string[] = []
  const raw = fulfillment ? (fulfillment.delivery_data ?? (fulfillment as unknown as Record<string, unknown>).logistics) : undefined
  if (!isRecord(raw)) return lines
  const note = String(raw.note ?? '').trim()
  if (note) lines.push(note)
  const entries = Array.isArray(raw.entries) ? raw.entries : []
  entries.forEach((entry) => {
    const e = isRecord(entry) ? entry : {}
    const key = String(e.key ?? '').trim()
    const value = String(e.value ?? '').trim()
    if (!key && !value) return
    lines.push(!key ? value : !value ? key : `${key}: ${value}`)
  })
  Object.entries(raw).forEach(([key, value]) => {
    if (key === 'note' || key === 'entries') return
    const text = (typeof value === 'object' && value !== null ? JSON.stringify(value) : String(value ?? '')).trim()
    if (text) lines.push(`${key}: ${text}`)
  })
  return lines
}

export const formatManualValue = (value: unknown): string => {
  if (Array.isArray(value)) return value.map((v) => String(v)).join(', ')
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

export interface ManualSubmissionRow {
  key: string
  label: string
  value: string
}

/** Rows of a manual-form submission, ordered by the schema snapshot first, then leftover keys. */
export const manualSubmissionRows = (submission: unknown, schemaSnapshot?: unknown): ManualSubmissionRow[] => {
  if (!isRecord(submission)) return []
  const entries = Object.entries(submission).filter(([key]) => key.trim() !== '')
  if (!entries.length) return []
  const valueMap = new Map(entries)
  const rows: ManualSubmissionRow[] = []
  const fields = isRecord(schemaSnapshot) && Array.isArray(schemaSnapshot.fields) ? schemaSnapshot.fields : []
  fields.forEach((field) => {
    if (!isRecord(field)) return
    const key = String(field.key ?? '').trim()
    if (!key || !valueMap.has(key)) return
    rows.push({ key, label: getLocalizedText(field.label) || key, value: formatManualValue(valueMap.get(key)) })
    valueMap.delete(key)
  })
  valueMap.forEach((value, key) => rows.push({ key, label: key, value: formatManualValue(value) }))
  return rows
}

export interface DeliveryEntry {
  key: string
  value: string
}

export const buildDeliveryDataPayload = (note: string, entries: DeliveryEntry[]): Record<string, unknown> => {
  const payload: Record<string, unknown> = {}
  const n = note.trim()
  if (n) payload.note = n
  const list = entries.map((e) => ({ key: e.key.trim(), value: e.value.trim() })).filter((e) => e.key || e.value)
  if (list.length) payload.entries = list
  return payload
}

export const parseOrderItemSkuId = (item: AdminOrderItem) => {
  const value = Number(item.sku_id || item.sku_snapshot?.sku_id || 0)
  if (!Number.isFinite(value)) return 0
  const n = Math.trunc(value)
  return n > 0 ? n : 0
}
