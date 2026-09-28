import { productAPI } from '@/api/catalog'
import type { Product, ProductSku } from '@/api/types'
import type { CartItem } from '@/stores/cart'
import { resolveCartPricingSnapshot } from './cartPricingSnapshot'
import { normalizeManualStockTotal, normalizeStockNumber } from './publicStock'
import { normalizeSkuId } from './sku'

export interface CartStoreLike {
  items: CartItem[]
  patchItem: (productId: number, skuId: number | undefined, patch: Partial<CartItem>) => void
}

const normalizeSkuCode = (value: unknown) => String(value || '').trim().toUpperCase()

const normalizeOptionalLimitNumber = (value: unknown): number | undefined => {
  const n = Number(value)
  if (!Number.isFinite(n)) return undefined
  const i = Math.floor(n)
  return i > 0 ? i : undefined
}

/** Max quantity per purchase (null = unlimited). */
export const cartItemPurchaseLimit = (item: Pick<CartItem, 'maxPurchaseQuantity'>): number | null =>
  normalizeOptionalLimitNumber(item.maxPurchaseQuantity) ?? null

/** Min quantity per purchase (default 1). */
export const cartItemPurchaseMin = (item: Pick<CartItem, 'minPurchaseQuantity'>): number =>
  normalizeOptionalLimitNumber(item.minPurchaseQuantity) ?? 1

/**
 * Stock available for a cart line from its snapshot, `null` when not enforced
 * (unlimited / hidden quantity).
 */
export const cartItemAvailableStock = (item: CartItem): number | null => {
  if (item.skuStockStatus === 'out_of_stock') return 0
  if (!item.skuStockEnforced) {
    return item.skuStockStatus === 'out_of_stock' ? 0 : null
  }
  if (item.fulfillmentType === 'auto') return item.skuAutoStockAvailable === undefined ? null : normalizeStockNumber(item.skuAutoStockAvailable)
  if (item.fulfillmentType === 'upstream') {
    if (item.skuUpstreamStock === undefined) return null
    return item.skuUpstreamStock === -1 ? null : normalizeStockNumber(item.skuUpstreamStock)
  }
  if (item.skuManualStockTotal === undefined) return null
  const total = normalizeManualStockTotal(item.skuManualStockTotal)
  return total === -1 ? null : total
}

const resolveMatchedSku = (item: CartItem, skus: ProductSku[]): ProductSku | null => {
  const skuId = normalizeSkuId(item.skuId)
  if (skuId > 0) {
    const byId = skus.find((sku) => normalizeSkuId(sku.id) === skuId)
    if (byId) return byId
  }
  const code = normalizeSkuCode(item.skuCode)
  if (code) {
    const byCode = skus.find((sku) => normalizeSkuCode(sku.sku_code) === code)
    if (byCode) return byCode
  }
  return skus.length === 1 ? skus[0] : null
}

export const shouldEnforceSkuStock = (product: Product | null | undefined, sku: ProductSku | null | undefined): boolean => {
  if (!product || !sku) return false
  if (sku.stock_quantity_hidden === true || product.stock_quantity_hidden === true) return false
  const type = String(product.fulfillment_type || '').trim()
  if (type === 'auto' || type === 'upstream') return true
  if (type !== 'manual') return false
  return normalizeManualStockTotal(sku.manual_stock_total) !== -1
}

/** Builds the stock/pricing patch for a cart line from fresh product data. */
export const buildCartStockPatch = (item: CartItem, product: Product, sku: ProductSku): Partial<CartItem> => {
  const rangeMin = normalizeStockNumber(sku.stock_range_min)
  const rangeMax = normalizeStockNumber(sku.stock_range_max)
  return {
    ...resolveCartPricingSnapshot(product, sku),
    skuCode: String(sku.sku_code || item.skuCode || ''),
    skuSpecValues: sku.spec_values && typeof sku.spec_values === 'object' ? sku.spec_values : item.skuSpecValues,
    skuManualStockTotal: normalizeManualStockTotal(sku.manual_stock_total),
    skuManualStockLocked: normalizeStockNumber(sku.manual_stock_locked),
    skuManualStockSold: normalizeStockNumber(sku.manual_stock_sold),
    skuAutoStockAvailable: normalizeStockNumber(sku.auto_stock_available),
    skuUpstreamStock: normalizeManualStockTotal(sku.upstream_stock),
    skuStockStatus: String(sku.stock_status || ''),
    skuStockDisplayMode: String(sku.stock_display_mode || product.stock_display_mode || ''),
    skuStockDisplay: String(sku.stock_display || ''),
    skuStockRangeMin: rangeMin > 0 ? rangeMin : undefined,
    skuStockRangeMax: rangeMax > 0 ? rangeMax : undefined,
    skuStockQuantityHidden: Boolean(sku.stock_quantity_hidden || product.stock_quantity_hidden),
    skuStockEnforced: shouldEnforceSkuStock(product, sku),
    skuStockSnapshotAt: new Date().toISOString(),
    minPurchaseQuantity: normalizeOptionalLimitNumber(product.min_purchase_quantity),
    maxPurchaseQuantity: normalizeOptionalLimitNumber(product.max_purchase_quantity),
  }
}

/** Re-fetches every product in the cart and refreshes stock snapshots. */
export const refreshCartStockSnapshots = async (cart: CartStoreLike): Promise<void> => {
  const items = Array.isArray(cart.items) ? cart.items : []
  const slugs = new Set(items.map((item) => String(item.slug || '').trim()).filter(Boolean))
  if (slugs.size === 0) return
  const products = new Map<string, Product>()
  await Promise.all(
    Array.from(slugs).map(async (slug) => {
      try {
        const res = await productAPI.detail(slug)
        if (res.data) products.set(slug, res.data)
      } catch {
        // 单个商品刷新失败时忽略
      }
    }),
  )
  for (const item of [...items]) {
    const product = products.get(String(item.slug || '').trim())
    if (!product) continue
    const skus = (product.skus || []).filter((sku) => sku.is_active)
    if (skus.length === 0) continue
    const sku = resolveMatchedSku(item, skus)
    if (!sku) continue
    cart.patchItem(item.productId, item.skuId, buildCartStockPatch(item, product, sku))
  }
}
