export type TranslateFn = (key: string, params?: Record<string, unknown>) => string
export type BadgeTone = 'success' | 'warning' | 'info' | 'danger' | 'accent' | 'neutral' | 'primary'

const ORDER_STATUSES = [
  'pending_payment',
  'paid',
  'fulfilling',
  'partially_delivered',
  'partially_refunded',
  'delivered',
  'completed',
  'expired',
  'canceled',
  'refunded',
] as const

export const orderStatusList = ORDER_STATUSES

export const orderStatusLabel = (t: TranslateFn, status?: string): string => {
  if (!status) return '-'
  return (ORDER_STATUSES as readonly string[]).includes(status) ? t(`order.status.${status}`) : status
}

export const orderStatusVariant = (status?: string): BadgeTone => {
  switch (status) {
    case 'pending_payment':
    case 'partially_refunded':
      return 'warning'
    case 'paid':
    case 'delivered':
    case 'completed':
      return 'success'
    case 'partially_delivered':
    case 'refunded':
      return 'info'
    case 'fulfilling':
      return 'accent'
    case 'expired':
      return 'danger'
    default:
      return 'neutral'
  }
}

/** Statuses after which the payment page redirects to the order detail. */
export const PAID_ORDER_STATUSES = new Set(['paid', 'fulfilling', 'partially_delivered', 'delivered', 'completed'])
