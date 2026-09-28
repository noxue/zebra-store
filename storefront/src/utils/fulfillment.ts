import type { TranslateFn } from './status'

export const fulfillmentTypeLabel = (t: TranslateFn, type?: string): string => {
  if (!type) return '-'
  if (type === 'auto') return t('orderDetail.fulfillmentTypes.auto')
  if (type === 'manual' || type === 'upstream') return t('orderDetail.fulfillmentTypes.manual')
  return type
}

export const fulfillmentStatusLabel = (t: TranslateFn, status?: string): string => {
  if (!status) return '-'
  if (status === 'pending' || status === 'delivered') return t(`orderDetail.fulfillmentStatuses.${status}`)
  return status
}
