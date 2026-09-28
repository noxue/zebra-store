import type { BadgeTone } from '@/components/ui/Badge'

export type TranslateFn = (key: string, params?: Record<string, unknown>) => string

const label = (t: TranslateFn, prefix: string, keys: readonly string[], status?: string) => {
  if (!status) return '-'
  return keys.includes(status) ? t(`${prefix}.${status}`) : status
}

export const ORDER_STATUSES = [
  'pending_payment',
  'paid',
  'fulfilling',
  'partially_delivered',
  'partially_refunded',
  'delivered',
  'completed',
  'canceled',
  'refunded',
] as const

export const orderStatusLabel = (t: TranslateFn, status?: string) => label(t, 'order.status', ORDER_STATUSES, status)

export const orderStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'pending_payment':
      return 'warning'
    case 'paid':
      return 'gold'
    case 'fulfilling':
      return 'secondary'
    case 'partially_delivered':
    case 'partially_refunded':
      return 'warning'
    case 'delivered':
      return 'primary'
    case 'completed':
      return 'success'
    case 'refunded':
      return 'info'
    default:
      return 'neutral'
  }
}

export const PAYMENT_STATUSES = ['initiated', 'pending', 'success', 'failed', 'expired'] as const

export const paymentStatusLabel = (t: TranslateFn, status?: string) => label(t, 'payment.status', PAYMENT_STATUSES, status)

export const paymentStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'success':
      return 'success'
    case 'pending':
      return 'warning'
    case 'failed':
      return 'danger'
    default:
      return 'neutral'
  }
}

export const userStatusLabel = (t: TranslateFn, status?: string) =>
  label(t, 'admin.users.status', ['active', 'disabled'], status)

export const userStatusTone = (status?: string): BadgeTone =>
  status === 'active' ? 'success' : status === 'disabled' ? 'danger' : 'neutral'

/** Generic tone for common lifecycle words used across list pages. */
export const genericStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'active':
    case 'success':
    case 'approved':
    case 'available':
    case 'paid':
    case 'completed':
    case 'verified':
    case 'fulfilled':
    case 'normal':
    case 'matched':
      return 'success'
    case 'pending':
    case 'pending_review':
    case 'pending_confirm':
    case 'pending_verification':
    case 'reserved':
    case 'running':
    case 'initiated':
    case 'accepted':
    case 'submitted':
      return 'warning'
    case 'failed':
    case 'rejected':
    case 'disabled':
    case 'deleted':
    case 'frozen':
    case 'frozen_review':
    case 'negative_balance':
    case 'mismatched':
      return 'danger'
    case 'used':
    case 'redeemed':
    case 'withdrawn':
    case 'refunded':
    case 'partially_refunded':
      return 'info'
    default:
      return 'neutral'
  }
}
