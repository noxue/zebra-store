import { computed, ref, watch, type Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { Product, ProductSku, PromotionRule, WholesalePrice } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { useBuyNowStore } from '@/stores/buyNow'
import { useCartStore, type CartItem } from '@/stores/cart'
import { useUserAuthStore } from '@/stores/userAuth'
import { shouldEnforceSkuStock } from '@/utils/cartStock'
import { getImageUrl } from '@/utils/image'
import { amountToCents } from '@/utils/money'
import {
  getWholesalePrices,
  hasPromotionPrice,
  resolveMemberPriceAmount,
  resolveWholesalePriceAmount,
} from '@/utils/productPricing'
import {
  normalizeManualStockTotal,
  normalizeStockNumber,
  resolveSkuAvailableStock,
  resolveSkuStockDisplay,
  type PublicStockDisplay,
} from '@/utils/publicStock'
import { buildSkuDisplayText, normalizeSkuId } from '@/utils/sku'
import type { BadgeTone } from '@/utils/status'
import { useLocalized } from './useLocalized'
import { useMemberPricing } from './useMemberPricing'
import { toast } from './useToast'

const optLimit = (value: unknown): number | null => {
  const n = Number(value)
  if (!Number.isFinite(n)) return null
  const i = Math.floor(n)
  return i > 0 ? i : null
}

const cents = (v: string | null | undefined) => amountToCents(v) ?? 0

/** Available stock for a sku, `null` when unlimited / not enforced. */
export const skuAvailableStock = (product: Product | null, sku: ProductSku | null): number | null => {
  if (!sku) return 0
  if (!shouldEnforceSkuStock(product, sku) && !sku.stock_quantity_hidden) return null
  return resolveSkuAvailableStock(product, sku)
}

export const isSkuPurchasable = (product: Product | null, sku: ProductSku | null) => {
  const available = skuAvailableStock(product, sku)
  return available === null || available > 0
}

/** min(product max_purchase_quantity, available stock); null = unlimited. */
export const effectivePurchaseLimit = (product: Product | null, sku: ProductSku | null): number | null => {
  const available = skuAvailableStock(product, sku)
  const productLimit = optLimit(product?.max_purchase_quantity)
  if (available === null) return productLimit
  return productLimit === null ? available : Math.min(productLimit, available)
}

/** Builds the cart/buy-now line from product + sku. */
export const buildCartItem = (product: Product, sku: ProductSku | null, quantity: number): CartItem => ({
  productId: product.id,
  skuId: normalizeSkuId(sku?.id),
  skuCode: String(sku?.sku_code || ''),
  skuSpecValues: sku?.spec_values && typeof sku.spec_values === 'object' ? sku.spec_values : undefined,
  skuManualStockTotal: normalizeManualStockTotal(sku?.manual_stock_total),
  skuManualStockLocked: normalizeStockNumber(sku?.manual_stock_locked),
  skuManualStockSold: normalizeStockNumber(sku?.manual_stock_sold),
  skuAutoStockAvailable: normalizeStockNumber(sku?.auto_stock_available),
  skuUpstreamStock: normalizeManualStockTotal(sku?.upstream_stock),
  skuStockStatus: String(sku?.stock_status || ''),
  skuStockDisplayMode: String(sku?.stock_display_mode || product.stock_display_mode || ''),
  skuStockDisplay: String(sku?.stock_display || ''),
  skuStockRangeMin: normalizeStockNumber(sku?.stock_range_min) || undefined,
  skuStockRangeMax: normalizeStockNumber(sku?.stock_range_max) || undefined,
  skuStockQuantityHidden: Boolean(sku?.stock_quantity_hidden || product.stock_quantity_hidden),
  skuStockEnforced: shouldEnforceSkuStock(product, sku),
  slug: product.slug,
  title: product.title,
  priceAmount: String(sku?.price_amount || product.price_amount || '0.00'),
  wholesalePrices: Array.isArray(product.wholesale_prices) ? product.wholesale_prices : undefined,
  image: product.images?.[0] ? getImageUrl(product.images[0]) : product.category?.icon ? getImageUrl(product.category.icon) : '',
  minPurchaseQuantity: optLimit(product.min_purchase_quantity) ?? undefined,
  maxPurchaseQuantity: optLimit(product.max_purchase_quantity) ?? undefined,
  purchaseType: product.purchase_type,
  fulfillmentType: product.fulfillment_type,
  manualFormSchema: product.manual_form_schema || {},
  paymentChannelIds: Array.isArray(product.payment_channel_ids) && product.payment_channel_ids.length > 0 ? product.payment_channel_ids : undefined,
  quantity,
})

/**
 * SKU selection, pricing (promotion/member/wholesale), stock limits and the
 * add-to-cart / buy-now actions shared by ProductDetail and ProductQuickBuy.
 */
export function useProductPurchase(product: Ref<Product | null>) {
  const route = useRoute()
  const router = useRouter()
  const { t } = useI18n()
  const appStore = useAppStore()
  const cartStore = useCartStore()
  const buyNowStore = useBuyNowStore()
  const auth = useUserAuthStore()
  const { formatPrice } = useLocalized()
  const member = useMemberPricing()

  const selectedSkuId = ref(0)
  const quantity = ref(1)
  const purchaseWarning = ref('')

  const activeSkus = computed<ProductSku[]>(() => (product.value?.skus || []).filter((s) => Boolean(s?.is_active)))
  const selectedSku = computed<ProductSku | null>(() =>
    selectedSkuId.value > 0 ? activeSkus.value.find((s) => normalizeSkuId(s.id) === selectedSkuId.value) || null : null,
  )

  const memberPrice = (basePrice: string | null | undefined): string | null => {
    if (!selectedSku.value || basePrice === null || basePrice === undefined) return null
    return resolveMemberPriceAmount(product.value, normalizeSkuId(selectedSku.value.id), basePrice, member.memberLevelId.value, member.discountRate.value)
  }

  const skuHasPromotion = computed(() => hasPromotionPrice(selectedSku.value))
  const skuMemberPrice = computed(() => memberPrice(selectedSku.value?.price_amount))
  const hasMemberPrice = computed(() => skuMemberPrice.value !== null && cents(skuMemberPrice.value) < cents(selectedSku.value?.price_amount))

  const wholesaleRules = computed<WholesalePrice[]>(() =>
    product.value && selectedSku.value ? getWholesalePrices(product.value, normalizeSkuId(selectedSku.value.id), selectedSku.value.sku_code) : [],
  )
  const wholesalePrice = computed(() =>
    product.value && selectedSku.value
      ? resolveWholesalePriceAmount(product.value, selectedSku.value.price_amount, quantity.value, normalizeSkuId(selectedSku.value.id), selectedSku.value.sku_code, quantity.value)
      : null,
  )
  const hasWholesalePrice = computed(() => {
    if (!selectedSku.value || !wholesalePrice.value) return false
    const compare = skuHasPromotion.value ? selectedSku.value.promotion_price_amount : selectedSku.value.price_amount
    return cents(wholesalePrice.value) < cents(compare)
  })
  const wholesaleMemberPrice = computed(() => (hasWholesalePrice.value ? memberPrice(wholesalePrice.value) : null))
  const promotionPrice = computed(() => (skuHasPromotion.value ? selectedSku.value?.promotion_price_amount ?? null : null))
  const promotionMemberPrice = computed(() => (promotionPrice.value !== null ? memberPrice(promotionPrice.value) : null))

  /**
   * Final displayed price for the selected sku, the struck-through original
   * (if any), and which tag applies.
   */
  const priceDisplay = computed(() => {
    const sku = selectedSku.value
    const p = product.value
    if (!sku) {
      if (p && hasPromotionPrice(p)) return { price: p.promotion_price_amount ?? p.price_amount, original: p.price_amount as string | null, tag: 'promotion' as const, isMember: false }
      return { price: p?.price_amount ?? '', original: null, tag: null, isMember: false }
    }
    if (hasWholesalePrice.value && wholesalePrice.value) {
      const isMember = wholesaleMemberPrice.value !== null
      return { price: wholesaleMemberPrice.value ?? wholesalePrice.value, original: sku.price_amount as string | null, tag: 'wholesale' as const, isMember }
    }
    if (promotionPrice.value !== null) {
      const isMember = promotionMemberPrice.value !== null
      return { price: promotionMemberPrice.value ?? promotionPrice.value, original: sku.price_amount as string | null, tag: 'promotion' as const, isMember }
    }
    if (hasMemberPrice.value && skuMemberPrice.value) {
      return { price: skuMemberPrice.value, original: sku.price_amount as string | null, tag: 'member' as const, isMember: true }
    }
    return { price: sku.price_amount, original: null, tag: null, isMember: false }
  })

  const saveAmount = computed(() => {
    const { price, original } = priceDisplay.value
    if (!original) return ''
    const diff = cents(original) - cents(price)
    return diff > 0 ? formatPrice((diff / 100).toFixed(2)) : ''
  })

  const promotionRules = computed<PromotionRule[]>(() => product.value?.promotion_rules ?? [])

  const formatPromotionRule = (rule: PromotionRule) => {
    const amount = formatPrice(rule.min_amount)
    const value = rule.type === 'percent' ? String(rule.value) : formatPrice(rule.value)
    const hasMin = cents(rule.min_amount) > 0
    switch (rule.type) {
      case 'percent':
        return hasMin ? t('products.promotionHintPercent', { amount, value }) : t('products.promotionHintPercentNoMin', { value })
      case 'fixed':
        return hasMin ? t('products.promotionHintFixed', { amount, value }) : t('products.promotionHintFixedNoMin', { value })
      case 'special_price':
        return hasMin ? t('products.promotionHintSpecial', { amount, value }) : t('products.promotionHintSpecialNoMin', { value })
      default:
        return rule.name || ''
    }
  }

  const formatWholesaleTier = (tier: WholesalePrice) =>
    t('products.wholesaleTier', { count: Number(tier.min_quantity || 0), price: formatPrice(tier.unit_price) })

  const formatStockDisplay = (d: PublicStockDisplay) => {
    switch (d.kind) {
      case 'unlimited':
        return t('productDetail.skuStockUnlimited')
      case 'out':
        return t('productDetail.skuStockOut')
      case 'remaining':
        return t('productDetail.skuStockRemaining', { count: d.count })
      case 'low_stock':
        return t('productDetail.skuStockLow')
      case 'hidden':
        return t('productDetail.skuStockHidden')
      case 'range':
        return t('productDetail.skuStockRange', { min: d.min, max: d.max })
      case 'range_plus':
        return t('productDetail.skuStockRangePlus', { min: d.min })
      default:
        return t('productDetail.skuStockInStock')
    }
  }
  const skuStockText = (sku: ProductSku) => formatStockDisplay(resolveSkuStockDisplay(product.value, sku))
  const skuStockTone = (sku: ProductSku): BadgeTone => {
    const d = resolveSkuStockDisplay(product.value, sku)
    if (d.kind === 'unlimited' || d.kind === 'hidden') return 'neutral'
    if (d.kind === 'out') return 'danger'
    if (d.kind === 'low_stock' || (d.kind === 'range' && d.max <= 5)) return 'warning'
    return 'success'
  }
  const skuDisplayText = (sku: ProductSku) =>
    buildSkuDisplayText({ skuCode: sku.sku_code, specValues: sku.spec_values, fallback: t('productDetail.skuFallback'), locale: appStore.locale })
  const skuPurchasable = (sku: ProductSku) => isSkuPurchasable(product.value, sku)

  const quantityLimit = computed(() => effectivePurchaseLimit(product.value, selectedSku.value))
  const quantityMin = computed(() => optLimit(product.value?.min_purchase_quantity) ?? 1)

  const purchaseType = computed(() => product.value?.purchase_type || 'member')
  const requiresLogin = computed(() => purchaseType.value === 'member' && !auth.isAuthenticated)
  const requiresSkuSelection = computed(() => activeSkus.value.length > 1 && !selectedSku.value)
  const stockBelowMin = computed(() => quantityLimit.value !== null && quantityLimit.value < quantityMin.value)

  const canPurchase = computed(() => {
    const p = product.value
    if (!p) return false
    if (activeSkus.value.length === 0) return false
    if (p.is_sold_out) return false
    if (requiresSkuSelection.value) return false
    if (p.stock_status === 'out_of_stock') return false
    if (selectedSku.value && !skuPurchasable(selectedSku.value)) return false
    if (stockBelowMin.value) return false
    return true
  })

  const cannotPurchaseReason = computed(() => {
    if (!product.value || requiresLogin.value) return ''
    if (requiresSkuSelection.value) return t('productDetail.skuRequired')
    if (stockBelowMin.value) return t('productDetail.stockBelowMinPurchase', { count: quantityMin.value })
    if (canPurchase.value) return ''
    return t('productDetail.stockUnavailable')
  })

  const syncSelectedSku = (keepCurrent = true) => {
    const rows = activeSkus.value
    if (rows.length === 0) {
      selectedSkuId.value = 0
      return
    }
    if (rows.length === 1) {
      selectedSkuId.value = normalizeSkuId(rows[0].id)
      return
    }
    if (keepCurrent && rows.some((s) => normalizeSkuId(s.id) === selectedSkuId.value)) return
    const first = rows.find((s) => skuPurchasable(s))
    selectedSkuId.value = normalizeSkuId((first || rows[0]).id)
  }

  const cartQuantityOfSelected = () => {
    if (!product.value || !selectedSku.value) return 0
    const skuId = normalizeSkuId(selectedSku.value.id)
    const matched = cartStore.items.find((i) => i.productId === product.value?.id && normalizeSkuId(i.skuId) === skuId)
    return Number(matched?.quantity || 0)
  }

  const goLogin = () => {
    void router.push(`/auth/login?redirect=${encodeURIComponent(route.fullPath)}`)
  }

  /** Returns true when added. */
  const addToCart = (): boolean => {
    const p = product.value
    if (!p || !canPurchase.value) return false
    purchaseWarning.value = ''
    if (requiresLogin.value) {
      goLogin()
      return false
    }
    const sku = selectedSku.value
    const available = skuAvailableStock(p, sku)
    const inCart = cartQuantityOfSelected()
    const next = inCart + quantity.value
    const productLimit = optLimit(p.max_purchase_quantity)
    const limit = quantityLimit.value
    if (limit !== null && next > limit) {
      if (available !== null && limit === available && (productLimit === null || available <= productLimit)) {
        purchaseWarning.value =
          available > 0
            ? inCart > 0
              ? t('productDetail.addCartStockExceededWithCart', { count: available, cartCount: inCart })
              : t('productDetail.addCartStockExceeded', { count: available })
            : t('productDetail.stockUnavailable')
      } else {
        purchaseWarning.value =
          inCart > 0 ? t('productDetail.addCartLimitExceededWithCart', { count: limit, cartCount: inCart }) : t('productDetail.addCartLimitExceeded', { count: limit })
      }
      return false
    }
    cartStore.addItem(buildCartItem(p, sku, quantity.value), quantity.value)
    toast.success(t('toast.addedToCart'))
    return true
  }

  const buyNow = (): boolean => {
    const p = product.value
    purchaseWarning.value = ''
    if (!p || !canPurchase.value) return false
    if (requiresLogin.value) {
      goLogin()
      return false
    }
    const sku = selectedSku.value
    const available = skuAvailableStock(p, sku)
    const limit = quantityLimit.value
    if (limit !== null && quantity.value > limit) {
      purchaseWarning.value =
        available !== null && limit === available
          ? available > 0
            ? t('productDetail.addCartStockExceeded', { count: available })
            : t('productDetail.stockUnavailable')
          : t('productDetail.addCartLimitExceeded', { count: limit })
      return false
    }
    buyNowStore.setItem(buildCartItem(p, sku, quantity.value))
    void router.push('/checkout?mode=buynow')
    return true
  }

  /** Stepper hit a bound: surface the reason like the original input handler. */
  const onQuantityLimit = (kind: 'min' | 'max') => {
    if (kind === 'min') toast.info(t('cart.minPurchaseNotMet', { count: quantityMin.value }))
    else if (quantityLimit.value !== null) toast.info(t('productDetail.addCartLimitExceeded', { count: quantityLimit.value }))
  }

  watch(selectedSkuId, () => {
    purchaseWarning.value = ''
    quantity.value = quantityMin.value
  })
  watch(quantityMin, (min) => {
    if (min > quantity.value) quantity.value = min
  })
  watch(quantityLimit, (limit) => {
    if (limit !== null && quantity.value > limit) quantity.value = Math.max(quantityMin.value, limit)
  })

  const reset = () => {
    purchaseWarning.value = ''
    quantity.value = quantityMin.value
    syncSelectedSku(false)
    void member.ensure()
  }

  return {
    selectedSkuId,
    selectedSku,
    activeSkus,
    quantity,
    quantityMin,
    quantityLimit,
    purchaseWarning,
    priceDisplay,
    saveAmount,
    hasMemberPrice,
    wholesaleRules,
    hasWholesalePrice,
    promotionRules,
    formatPromotionRule,
    formatWholesaleTier,
    skuStockText,
    skuStockTone,
    skuDisplayText,
    skuPurchasable,
    purchaseType,
    requiresLogin,
    requiresSkuSelection,
    canPurchase,
    cannotPurchaseReason,
    syncSelectedSku,
    addToCart,
    buyNow,
    goLogin,
    onQuantityLimit,
    reset,
    ensureMember: member.ensure,
  }
}
