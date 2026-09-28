import type { Product, ProductSku } from '@/api/types'

type StockProduct = Pick<Product, 'fulfillment_type' | 'stock_display_mode' | 'stock_quantity_hidden'> | null | undefined
type StockSku = Partial<ProductSku> | null | undefined

export type PublicStockDisplay =
  | { kind: 'unlimited' }
  | { kind: 'out' }
  | { kind: 'remaining'; count: number }
  | { kind: 'in_stock' }
  | { kind: 'low_stock' }
  | { kind: 'hidden' }
  | { kind: 'range'; min: number; max: number }
  | { kind: 'range_plus'; min: number }

const normalizeStockNumber = (value: unknown) => {
  const n = Number(value)
  if (!Number.isFinite(n)) return 0
  return Math.max(Math.floor(n), 0)
}

const normalizeManualStockTotal = (value: unknown) => {
  const n = Number(value)
  if (!Number.isFinite(n)) return 0
  const i = Math.floor(n)
  if (i === -1) return -1
  return Math.max(i, 0)
}

const normalizeStockStatus = (value: unknown, fallbackQuantity: number | null) => {
  const status = String(value || '').trim()
  if (['unlimited', 'in_stock', 'low_stock', 'out_of_stock'].includes(status)) return status
  if (fallbackQuantity === null || fallbackQuantity === -1) return 'unlimited'
  if (fallbackQuantity <= 0) return 'out_of_stock'
  if (fallbackQuantity <= 5) return 'low_stock'
  return 'in_stock'
}

const isQuantityHidden = (product: StockProduct, sku: StockSku) => {
  if (sku?.stock_quantity_hidden === true) return true
  if (product?.stock_quantity_hidden === true) return true
  return String(product?.stock_display_mode || sku?.stock_display_mode || 'exact').trim() !== 'exact'
}

/** Available quantity for a SKU; `null` = unlimited / not disclosed. */
export const resolveSkuAvailableStock = (product: StockProduct, sku: StockSku): number | null => {
  if (!sku) return 0
  if (isQuantityHidden(product, sku)) {
    return normalizeStockStatus(sku.stock_status, null) === 'out_of_stock' ? 0 : null
  }
  if (product?.fulfillment_type === 'upstream') {
    const upstream = Number(sku.upstream_stock ?? 0)
    if (upstream === -1) return null
    return Math.max(Math.floor(upstream), 0)
  }
  if (product?.fulfillment_type === 'auto') {
    const auto = Number(sku.auto_stock_available ?? 0)
    if (auto < 0) return null
    return normalizeStockNumber(auto)
  }
  const total = normalizeManualStockTotal(sku.manual_stock_total)
  if (total === -1) return null
  // manual_stock_total is the remaining sellable stock on the public API.
  return total
}

export const resolveSkuStockDisplay = (product: StockProduct, sku: StockSku): PublicStockDisplay => {
  const available = resolveSkuAvailableStock(product, sku)
  const hidden = isQuantityHidden(product, sku)
  const status = normalizeStockStatus(sku?.stock_status, available)
  if (status === 'unlimited') return { kind: 'unlimited' }
  if (status === 'out_of_stock') return { kind: 'out' }
  if (!hidden) return { kind: 'remaining', count: Math.max(available ?? 0, 0) }
  const display = String(sku?.stock_display || '').trim()
  const min = Number(sku?.stock_range_min)
  const max = Number(sku?.stock_range_max)
  if (display.startsWith('range_') && Number.isFinite(min) && min > 0) {
    if (Number.isFinite(max) && max >= min) return { kind: 'range', min: Math.floor(min), max: Math.floor(max) }
    return { kind: 'range_plus', min: Math.floor(min) }
  }
  if (display === 'hidden') return { kind: 'hidden' }
  if (status === 'low_stock') return { kind: 'low_stock' }
  return { kind: 'in_stock' }
}

export { normalizeManualStockTotal, normalizeStockNumber }
