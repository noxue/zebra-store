import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import { cartIdentity, useCartStore, type CartItem } from '@/stores/cart'
import { cartItemPurchaseLimit, cartItemPurchaseMin, refreshCartStockSnapshots } from '@/utils/cartStock'
import { getImageUrl } from '@/utils/image'
import { amountToCents, centsToAmount } from '@/utils/money'
import { resolveWholesalePriceAmount } from '@/utils/productPricing'
import { normalizeManualStockTotal, normalizeStockNumber } from '@/utils/publicStock'
import { buildSkuDisplayText } from '@/utils/sku'
import { useLocalized } from './useLocalized'
import { usePageTitle } from './usePageTitle'
import { toast } from './useToast'

/** Undo window for removed cart lines (ms). */
const UNDO_DURATION_MS = 5000

const shouldEnforceItemStock = (item: CartItem) => {
  if (item.skuStockQuantityHidden === true) return false
  if (item.fulfillmentType === 'auto' || item.fulfillmentType === 'upstream') return true
  if (item.fulfillmentType !== 'manual') return false
  if (!String(item.skuStockSnapshotAt || '').trim()) return false
  if (normalizeManualStockTotal(item.skuManualStockTotal) === -1) return false
  return item.skuStockEnforced !== false
}

/** Stock known for a cart line (null = unlimited / hidden). */
export const cartLineAvailableStock = (item: CartItem): number | null => {
  if (item.skuStockStatus === 'out_of_stock') return 0
  if (item.skuStockQuantityHidden === true) return item.skuStockStatus === 'out_of_stock' ? 0 : null
  if (!shouldEnforceItemStock(item)) return null
  if (item.fulfillmentType === 'upstream') {
    if (item.skuUpstreamStock === undefined) return null
    const upstream = Number(item.skuUpstreamStock ?? 0)
    return upstream === -1 ? null : Math.max(upstream, 0)
  }
  if (item.fulfillmentType === 'auto') return item.skuAutoStockAvailable === undefined ? null : normalizeStockNumber(item.skuAutoStockAvailable)
  if (item.skuManualStockTotal === undefined) return null
  const total = normalizeManualStockTotal(item.skuManualStockTotal)
  return total === -1 ? null : total
}

/** Max quantity for a line = min(stock, purchase limit). */
export const cartLineMaxQuantity = (item: CartItem): number => {
  const available = cartLineAvailableStock(item)
  const limit = cartItemPurchaseLimit(item)
  if (available === null && limit === null) return Number.MAX_SAFE_INTEGER
  if (available === null) return limit || 0
  if (limit === null) return Math.max(available, 0)
  return Math.max(Math.min(available, limit), 0)
}

/** Unit price for a line after wholesale tiers (catalog price otherwise). */
export const cartLineUnitPrice = (item: CartItem): string => {
  const wholesale = resolveWholesalePriceAmount(
    { wholesale_prices: item.wholesalePrices },
    item.priceAmount,
    item.quantity,
    item.skuId,
    item.skuCode,
    item.quantity,
  )
  return wholesale ?? item.priceAmount
}

export const cartLineSubtotal = (item: CartItem): string =>
  centsToAmount((amountToCents(cartLineUnitPrice(item)) ?? 0) * Math.max(0, Math.floor(item.quantity)))

export const cartTotal = (items: CartItem[]): string =>
  centsToAmount(items.reduce((sum, item) => sum + (amountToCents(cartLineSubtotal(item)) ?? 0), 0))

/** Cart page logic. */
export function useCart() {
  const cartStore = useCartStore()
  const appStore = useAppStore()
  const { t } = useI18n()
  const { getLocalizedText, formatPrice } = useLocalized()
  const warnings = ref<Record<string, string>>({})

  usePageTitle(() => t('cart.title'))

  const items = computed(() => cartStore.items)
  const totalItems = computed(() => cartStore.totalItems)
  const totalAmount = computed(() => cartTotal(cartStore.items))

  const itemImage = (item: CartItem) => getImageUrl(String(item.image || '').trim())
  const itemSku = (item: CartItem) =>
    buildSkuDisplayText({ skuCode: item.skuCode, specValues: item.skuSpecValues, fallback: t('productDetail.skuFallback'), locale: appStore.locale })
  const hasWholesale = (item: CartItem) => cartLineUnitPrice(item) !== item.priceAmount

  const stockHint = (item: CartItem) => {
    const available = cartLineAvailableStock(item)
    const limit = cartItemPurchaseLimit(item)
    if (available === null) return ''
    if (available <= 0) return t('cart.stockOut')
    if (limit !== null && cartLineMaxQuantity(item) === limit && limit < available) return t('cart.maxPurchaseExceeded', { count: limit })
    return t('cart.stockRemaining', { count: available })
  }

  const warning = (item: CartItem) => warnings.value[cartIdentity(item)] || ''

  const updateQty = (item: CartItem, qty: number) => {
    const key = cartIdentity(item)
    warnings.value[key] = ''
    const max = cartLineMaxQuantity(item)
    const available = cartLineAvailableStock(item)
    const limit = cartItemPurchaseLimit(item)
    const min = cartItemPurchaseMin(item)
    if (qty < min) {
      if (min > 1) warnings.value[key] = t('cart.minPurchaseNotMet', { count: min })
      return
    }
    if (qty > max) {
      if (max <= 0) warnings.value[key] = t('cart.stockOut')
      else if (limit !== null && max === limit && (available === null || limit < available)) warnings.value[key] = t('cart.maxPurchaseExceeded', { count: limit })
      else warnings.value[key] = t('cart.stockExceeded', { count: max })
      return
    }
    cartStore.updateQuantity(item.productId, qty, item.skuId)
  }

  const removeWithUndo = (item: CartItem) => {
    const index = cartStore.items.findIndex((i) => cartIdentity(i) === cartIdentity(item))
    const removed = { ...item }
    cartStore.removeItem(item.productId, item.skuId)
    toast.info(t('cart.removed'), {
      duration: UNDO_DURATION_MS,
      action: { label: t('cart.undo'), onClick: () => cartStore.restoreItem(removed, index) },
    })
  }

  onMounted(() => void refreshCartStockSnapshots(cartStore))

  return {
    items,
    totalItems,
    totalAmount,
    getLocalizedText,
    formatPrice,
    itemImage,
    itemSku,
    hasWholesale,
    stockHint,
    warning,
    updateQty,
    removeWithUndo,
    unitPrice: cartLineUnitPrice,
    subtotal: cartLineSubtotal,
    minQty: cartItemPurchaseMin,
    maxQty: (item: CartItem) => {
      const m = cartLineMaxQuantity(item)
      return m >= Number.MAX_SAFE_INTEGER ? null : m
    },
  }
}
