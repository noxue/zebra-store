import { beforeEach, describe, expect, it } from 'vitest'
import type { Product, ProductSku } from '@/api/types'
import { useAnnouncement, todayStr } from '@/composables/useAnnouncement'
import { cartLineMaxQuantity, cartLineSubtotal, cartLineUnitPrice, cartTotal } from '@/composables/useCart'
import { groupProductsByCategory } from '@/composables/useProductList'
import { buildCartItem, effectivePurchaseLimit, isSkuPurchasable, skuAvailableStock } from '@/composables/useProductPurchase'
import type { CartItem } from '@/stores/cart'
import { createCategoryMap } from '@/utils/category'

const sku = (over: Partial<ProductSku> = {}): ProductSku => ({ id: 1, sku_code: 'A', price_amount: '10.00', is_active: true, ...over })
const product = (over: Partial<Product> = {}): Product => ({
  id: 7,
  slug: 'p',
  title: { 'zh-CN': 'P' },
  price_amount: '10.00',
  purchase_type: 'guest',
  fulfillment_type: 'auto',
  stock_display_mode: 'exact',
  ...over,
})
const line = (over: Partial<CartItem> = {}): CartItem => ({ productId: 1, skuId: 1, slug: 'p', title: {}, priceAmount: '10.00', quantity: 1, ...over })

describe('useAnnouncement', () => {
  beforeEach(() => {
    localStorage.clear()
    sessionStorage.clear()
  })
  const a = { version: 'v1', title: { 'zh-CN': 'hi' } }

  it('shows a versioned announcement until dismissed', () => {
    const { shouldShow, closeForSession } = useAnnouncement()
    expect(shouldShow(a)).toBe(true)
    closeForSession('v1')
    expect(shouldShow(a)).toBe(false)
    expect(sessionStorage.getItem('announcement_closed')).toBe('v1')
  })

  it('today / forever dismissal is per version', () => {
    const { shouldShow, dismissToday, dismissForever } = useAnnouncement()
    dismissToday('v1')
    expect(JSON.parse(localStorage.getItem('announcement_dismiss') || '{}')).toEqual({ version: 'v1', mode: 'today', date: todayStr() })
    expect(shouldShow(a)).toBe(false)
    expect(shouldShow({ ...a, version: 'v2' })).toBe(true)
    dismissForever('v2')
    expect(shouldShow({ ...a, version: 'v2' })).toBe(false)
  })

  it('ignores announcements without version', () => {
    expect(useAnnouncement().shouldShow({ title: { 'zh-CN': 'x' } })).toBe(false)
  })
})

describe('purchase stock rules', () => {
  it('auto fulfillment uses auto_stock_available', () => {
    const p = product()
    expect(skuAvailableStock(p, sku({ auto_stock_available: 3 }))).toBe(3)
    expect(isSkuPurchasable(p, sku({ auto_stock_available: 0 }))).toBe(false)
  })

  it('manual with -1 total is unlimited', () => {
    const p = product({ fulfillment_type: 'manual' })
    expect(skuAvailableStock(p, sku({ manual_stock_total: -1 }))).toBeNull()
  })

  it('limit is min(max_purchase_quantity, stock)', () => {
    expect(effectivePurchaseLimit(product({ max_purchase_quantity: 10 }), sku({ auto_stock_available: 3 }))).toBe(3)
    expect(effectivePurchaseLimit(product({ max_purchase_quantity: 2 }), sku({ auto_stock_available: 3 }))).toBe(2)
    expect(effectivePurchaseLimit(product({ fulfillment_type: 'manual' }), sku({ manual_stock_total: -1 }))).toBeNull()
  })

  it('buildCartItem snapshots sku fields', () => {
    const item = buildCartItem(product({ min_purchase_quantity: 0, max_purchase_quantity: 5 }), sku({ id: 4, auto_stock_available: 9 }), 2)
    expect(item).toMatchObject({ productId: 7, skuId: 4, skuCode: 'A', priceAmount: '10.00', quantity: 2, skuAutoStockAvailable: 9, skuStockEnforced: true, maxPurchaseQuantity: 5 })
    expect(item.minPurchaseQuantity).toBeUndefined()
  })
})

describe('cart line pricing', () => {
  it('applies wholesale tier when reached', () => {
    const item = line({ quantity: 5, wholesalePrices: [{ min_quantity: 5, unit_price: '8.00' }] })
    expect(cartLineUnitPrice(item)).toBe('8.00')
    expect(cartLineSubtotal(item)).toBe('40.00')
    expect(cartLineUnitPrice({ ...item, quantity: 4 })).toBe('10.00')
  })

  it('totals lines exactly in cents', () => {
    expect(cartTotal([line({ priceAmount: '0.10', quantity: 3 }), line({ productId: 2, priceAmount: '0.20' })])).toBe('0.50')
  })

  it('max quantity combines stock snapshot and purchase limit', () => {
    expect(cartLineMaxQuantity(line({ fulfillmentType: 'auto', skuAutoStockAvailable: 4, maxPurchaseQuantity: 10 }))).toBe(4)
    expect(cartLineMaxQuantity(line({ fulfillmentType: 'auto', skuAutoStockAvailable: 40, maxPurchaseQuantity: 10 }))).toBe(10)
    expect(cartLineMaxQuantity(line({ fulfillmentType: 'manual' }))).toBe(Number.MAX_SAFE_INTEGER)
  })
})

describe('groupProductsByCategory', () => {
  it('groups child categories under their parent in first-seen order', () => {
    const cats = [
      { id: 1, slug: 'a', name: { 'zh-CN': 'A' }, parent_id: 0 },
      { id: 2, slug: 'b', name: { 'zh-CN': 'B' }, parent_id: 1 },
      { id: 3, slug: 'c', name: { 'zh-CN': 'C' }, parent_id: 0 },
    ]
    const map = createCategoryMap(cats)
    const ps = [
      product({ id: 1, category: { id: 3, slug: 'c', name: { 'zh-CN': 'C' } } }),
      product({ id: 2, category: { id: 2, slug: 'b', name: { 'zh-CN': 'B' } } }),
      product({ id: 3, category: null }),
    ]
    const groups = groupProductsByCategory(ps, map, (v) => v?.['zh-CN'] || '', 'All')
    expect(groups.map((g) => [g.categoryId, g.categoryName, g.products.length])).toEqual([
      [3, 'C', 1],
      [1, 'A', 1],
      [null, 'All', 1],
    ])
  })
})
