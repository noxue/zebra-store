import type { CartItem } from '@/stores/cart'
import { cartItemPurchaseLimit, cartItemPurchaseMin } from './cartStock'
import { parseInteger } from './money'
import { normalizeManualStockTotal, normalizeStockNumber } from './publicStock'

export type Translate = (key: string, params?: Record<string, unknown>) => string

const hasSnapshot = (item: CartItem) => Boolean(String(item.skuStockSnapshotAt || '').trim())

const shouldEnforce = (item: CartItem) => {
  if (item.skuStockQuantityHidden === true) return false
  if (item.fulfillmentType === 'auto' || item.fulfillmentType === 'upstream') return true
  if (item.fulfillmentType !== 'manual') return false
  if (!hasSnapshot(item)) return false
  if (normalizeManualStockTotal(item.skuManualStockTotal) === -1) return false
  if (item.skuStockEnforced === false) return false
  return true
}

/** Available stock of a checkout line (null = unlimited / not enforced). Mirrors original checkout. */
export const checkoutItemAvailableStock = (item: CartItem): number | null => {
  if (item.skuStockStatus === 'out_of_stock') return 0
  if (item.skuStockQuantityHidden === true) return item.skuStockStatus === 'out_of_stock' ? 0 : null
  if (!shouldEnforce(item)) return null
  if (item.fulfillmentType === 'upstream') {
    if (item.skuUpstreamStock === undefined) return null
    const upstream = Number(item.skuUpstreamStock ?? 0)
    if (upstream === -1) return null
    return Math.max(upstream, 0)
  }
  if (item.fulfillmentType === 'auto') return item.skuAutoStockAvailable === undefined ? null : normalizeStockNumber(item.skuAutoStockAvailable)
  if (item.skuManualStockTotal === undefined) return null
  const total = normalizeManualStockTotal(item.skuManualStockTotal)
  return total === -1 ? null : total
}

export const checkoutItemMaxQuantity = (item: CartItem): number => {
  const available = checkoutItemAvailableStock(item)
  const limit = cartItemPurchaseLimit(item)
  if (available === null && limit === null) return Number.MAX_SAFE_INTEGER
  if (available === null) return limit || 0
  if (limit === null) return Math.max(available, 0)
  return Math.max(Math.min(available, limit), 0)
}

export const checkoutItemStockExceeded = (item: CartItem) => {
  const qty = parseInteger(item.quantity)
  if (qty === null) return true
  return qty > checkoutItemMaxQuantity(item)
}

export const checkoutItemMinNotMet = (item: CartItem) => {
  const qty = parseInteger(item.quantity)
  if (qty === null) return false
  return qty < cartItemPurchaseMin(item)
}

export const checkoutItemStockHint = (item: CartItem, t: Translate): string => {
  const available = checkoutItemAvailableStock(item)
  const limit = cartItemPurchaseLimit(item)
  const max = checkoutItemMaxQuantity(item)
  if (checkoutItemMinNotMet(item)) return t('cart.minPurchaseNotMet', { count: cartItemPurchaseMin(item) })
  if (available === null && limit === null) return ''
  if (max <= 0) return t('cart.stockOut')
  if (checkoutItemStockExceeded(item)) {
    if (limit !== null && max === limit && (available === null || limit < available)) return t('cart.maxPurchaseExceeded', { count: limit })
    return t('cart.stockExceeded', { count: max })
  }
  if (available === null) return ''
  if (available <= 0) return t('cart.stockOut')
  return t('cart.stockRemaining', { count: available })
}
