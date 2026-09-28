import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { guestOrderAPI, userOrderAPI } from '@/api/order'
import type { CreateAndPayPayload, OrderItem, OrderItemInput, OrderPreview, PaymentChannel } from '@/api/types'
import { walletAPI } from '@/api/wallet'
import { useCaptcha } from '@/composables/useCaptcha'
import { useLocalized } from '@/composables/useLocalized'
import { useAppStore } from '@/stores/app'
import { useBuyNowStore } from '@/stores/buyNow'
import { normalizeSkuId, useCartStore, type CartItem } from '@/stores/cart'
import { useUserAuthStore } from '@/stores/userAuth'
import { getAffiliateCode, getAffiliateVisitorKey } from '@/utils/affiliate'
import { cartItemPurchaseMin, refreshCartStockSnapshots } from '@/utils/cartStock'
import {
  checkoutItemMinNotMet,
  checkoutItemStockExceeded,
  checkoutItemStockHint,
} from '@/utils/checkoutStock'
import { debounce } from '@/utils/debounce'
import { saveGuestOrderAuth } from '@/utils/guestOrderAuth'
import { getImageUrl } from '@/utils/image'
import {
  buildManualFormDataPayload,
  buildManualFormProducts,
  isValidEmail,
  manualFieldErrorKey,
  syncManualFormValues,
  validateManualForm,
  type ManualFormValues,
} from '@/utils/manualForm'
import { amountToCents, centsToAmount, parseInteger } from '@/utils/money'
import {
  channelLimitMeta,
  filterSupportedChannels,
  formatFeeRate,
  intersectAllowedChannelIds,
  isChannelDisabledForAmount as channelDisabled,
  restrictChannels,
  splitWalletPayment,
} from '@/utils/orderPayment'
import { resolveWholesalePriceAmount } from '@/utils/productPricing'
import { buildSkuDisplayText } from '@/utils/sku'

/** Minimum guest order password length (original checkout). */
const GUEST_PASSWORD_MIN_LENGTH = 6

export type CheckoutAlert = { level: 'error' | 'warning'; message: string }

/** Checkout page logic (port of the original useCheckout). */
export function useCheckout() {
  const router = useRouter()
  const route = useRoute()
  const cartStore = useCartStore()
  const buyNowStore = useBuyNowStore()
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  const { t } = useI18n()
  const { getLocalizedText, siteCurrency, formatPrice } = useLocalized()
  const captcha = useCaptcha('guest_create_order')

  const isBuyNowMode = computed(() => route.query.mode === 'buynow')
  const cartItems = computed<CartItem[]>(() => {
    if (isBuyNowMode.value) return buyNowStore.item ? [buyNowStore.item] : []
    return cartStore.items
  })
  const totalItems = computed(() => cartItems.value.reduce((sum, item) => sum + item.quantity, 0))

  const couponCode = ref('')
  const normalizedCoupon = computed(() => couponCode.value.trim())
  const submitting = ref(false)
  const error = ref('')
  const preview = ref<OrderPreview | null>(null)
  const previewLoading = ref(false)
  const previewError = ref('')
  let previewRequestId = 0
  const couponRefreshing = ref(false)
  const syncingStock = ref(false)
  const orderPaymentChannels = ref<PaymentChannel[]>([])
  let channelsRequestId = 0

  const selectedChannelId = ref<number | null>(null)
  const useBalance = ref(false)
  const walletLoading = ref(false)
  const walletBalance = ref('0')

  const checkoutMode = ref<'guest' | 'member'>('guest')
  const guestEmail = ref('')
  const guestPassword = ref('')
  const submitAttempted = ref(false)
  const manualFormData = ref<ManualFormValues>({})

  const isResellerTenant = computed(() => appStore.isResellerTenant)
  const walletOnlyPayment = computed(() => !!appStore.config?.wallet_only_payment)
  const showBalanceOption = computed(() => auth.isAuthenticated)
  const isGuestCheckout = computed(() => !auth.isAuthenticated && checkoutMode.value === 'guest')
  const guestEmailValid = computed(() => !isGuestCheckout.value || isValidEmail(guestEmail.value))
  const guestPasswordValid = computed(() => guestPassword.value.trim().length >= GUEST_PASSWORD_MIN_LENGTH)

  // ------------------------------------------------------------ amounts
  const totalAmount = computed(() =>
    centsToAmount(
      cartItems.value.reduce((sum, item) => {
        const cents = amountToCents(item.priceAmount)
        const qty = parseInteger(item.quantity)
        return cents === null || qty === null ? sum : sum + cents * qty
      }, 0),
    ),
  )
  const previewCurrency = computed(() => preview.value?.currency || siteCurrency.value || 'CNY')
  const previewOriginal = computed(() => preview.value?.original_amount ?? totalAmount.value)
  const previewCoupon = computed(() => preview.value?.discount_amount ?? '0')
  const previewPromotion = computed(() => preview.value?.promotion_discount_amount ?? '0')
  const previewWholesale = computed(() => preview.value?.wholesale_discount_amount ?? '0')
  const previewMemberDiscount = computed(() => preview.value?.member_discount_amount ?? '0')
  const previewTotal = computed(() => preview.value?.total_amount ?? totalAmount.value)

  const walletSplit = computed(() => splitWalletPayment(previewTotal.value, walletBalance.value, showBalanceOption.value && useBalance.value))
  const expectedOnlinePayCents = computed(() => walletSplit.value.onlineCents)
  const expectedWalletPaidDisplay = computed(() => formatPrice(centsToAmount(walletSplit.value.walletCents), previewCurrency.value))
  const expectedOnlinePayDisplay = computed(() => formatPrice(centsToAmount(walletSplit.value.onlineCents), previewCurrency.value))
  const requiresOnlineChannel = computed(() => !auth.isAuthenticated || !useBalance.value || expectedOnlinePayCents.value > 0)

  // ------------------------------------------------------------ channels
  const paymentChannels = computed<PaymentChannel[]>(() => {
    const source: PaymentChannel[] = auth.isAuthenticated ? orderPaymentChannels.value : (appStore.config?.payment_channels ?? [])
    const allowed = intersectAllowedChannelIds(cartItems.value.map((item) => item.paymentChannelIds))
    return restrictChannels(filterSupportedChannels(source), cartItems.value.length > 0 ? allowed : null)
  })

  const isChannelDisabledForAmount = (channel: PaymentChannel) =>
    channelDisabled(channel, expectedOnlinePayCents.value, requiresOnlineChannel.value)

  const channelAmountLimitHint = (channel: PaymentChannel) => {
    const meta = channelLimitMeta(channel)
    const fmt = (cents: number | null) => formatPrice(centsToAmount(cents ?? 0), previewCurrency.value)
    if (meta.hasMin && meta.hasMax) return t('checkout.channelAmountLimitHint', { min: fmt(meta.minCents), max: fmt(meta.maxCents) })
    if (meta.hasMin) return t('checkout.channelAmountMinHint', { min: fmt(meta.minCents) })
    if (meta.hasMax) return t('checkout.channelAmountMaxHint', { max: fmt(meta.maxCents) })
    return ''
  }

  const selectChannel = (channel: PaymentChannel) => {
    if (isChannelDisabledForAmount(channel)) return
    selectedChannelId.value = Number(channel.id) || null
  }

  const selectedChannelAmountHint = computed(() => {
    const channel = paymentChannels.value.find((c) => Number(c.id) === Number(selectedChannelId.value))
    if (!channel || !isChannelDisabledForAmount(channel)) return ''
    return channelAmountLimitHint(channel)
  })

  const formatChannelFeeRate = (channel: PaymentChannel) => formatFeeRate(channel.fee_rate)
  const formatChannelFixedFee = (channel: PaymentChannel) => {
    const fixed = channel.fixed_fee
    return formatPrice(fixed && Number(fixed) !== 0 ? String(fixed) : '0.00', previewCurrency.value)
  }

  // ------------------------------------------------------------ manual form
  const manualFormProducts = computed(() =>
    buildManualFormProducts(
      cartItems.value.map((item) => ({
        productId: item.productId,
        title: item.title,
        fulfillmentType: item.fulfillmentType,
        manualFormSchema: item.manualFormSchema,
      })),
    ),
  )
  watch(
    manualFormProducts,
    (products) => {
      manualFormData.value = syncManualFormValues(products, manualFormData.value)
    },
    { immediate: true, deep: true },
  )
  const manualValidation = computed(() =>
    validateManualForm(manualFormProducts.value, manualFormData.value, (key, params) => t(key, params), appStore.locale),
  )
  const manualFieldError = (itemKey: string, fieldKey: string) =>
    manualValidation.value.errors[manualFieldErrorKey(itemKey, fieldKey)] || ''
  const manualFingerprint = computed(() => JSON.stringify(manualFormData.value))

  // ------------------------------------------------------------ submit rules
  const canSubmit = computed(() => {
    if (syncingStock.value || submitting.value) return false
    if (cartItems.value.length === 0) return false
    if (!manualValidation.value.valid) return false
    if (cartItems.value.some(checkoutItemStockExceeded)) return false
    if (cartItems.value.some(checkoutItemMinNotMet)) return false
    if (walletOnlyPayment.value && expectedOnlinePayCents.value > 0) return false
    if (!walletOnlyPayment.value && requiresOnlineChannel.value && !selectedChannelId.value) return false
    if (requiresOnlineChannel.value && selectedChannelAmountHint.value) return false
    if (auth.isAuthenticated) return true
    if (checkoutMode.value !== 'guest') return false
    if (!guestEmail.value.trim() || !guestPassword.value.trim() || !guestEmailValid.value || !guestPasswordValid.value) return false
    return captcha.isComplete()
  })

  const submitBlockedReason = computed(() => {
    if (syncingStock.value) return t('checkout.stockSyncing')
    if (cartItems.value.length === 0) return t('checkout.errors.emptyCart')
    if (!manualValidation.value.valid) return manualValidation.value.firstError || t('checkout.errors.manualFormInvalid')
    const stockBlocked = cartItems.value.find(checkoutItemStockExceeded)
    if (stockBlocked) return checkoutItemStockHint(stockBlocked, t) || t('cart.stockOut')
    const minBlocked = cartItems.value.find(checkoutItemMinNotMet)
    if (minBlocked) return t('cart.minPurchaseNotMet', { count: cartItemPurchaseMin(minBlocked) })
    if (walletOnlyPayment.value && expectedOnlinePayCents.value > 0) return t('payment.walletInsufficientHint')
    if (!walletOnlyPayment.value && requiresOnlineChannel.value && !selectedChannelId.value) return t('checkout.errors.selectPayment')
    if (requiresOnlineChannel.value && selectedChannelAmountHint.value) return selectedChannelAmountHint.value
    if (auth.isAuthenticated) return ''
    if (checkoutMode.value !== 'guest') return t('checkout.errors.loginOrGuest')
    if (!guestEmail.value.trim() || !guestPassword.value.trim()) return t('checkout.errors.missingGuest')
    if (!guestEmailValid.value) return t('error.email_invalid')
    if (!guestPasswordValid.value) return t('checkout.errors.guestPasswordTooShort')
    if (!captcha.isComplete()) return t('auth.common.captchaRequired')
    return ''
  })

  const previewStatusText = computed(() => (couponRefreshing.value ? t('checkout.couponRefreshing') : t('checkout.previewLoading')))

  const checkoutAlert = computed<CheckoutAlert | null>(() => {
    if (error.value) return { level: 'error', message: error.value }
    if (previewError.value) return { level: 'error', message: previewError.value }
    if (!canSubmit.value && submitBlockedReason.value) return { level: 'warning', message: submitBlockedReason.value }
    return null
  })

  // ------------------------------------------------------------ requests
  const buildItemsPayload = (): OrderItemInput[] =>
    cartItems.value.map((item) => ({
      product_id: item.productId,
      sku_id: normalizeSkuId(item.skuId) || undefined,
      quantity: item.quantity,
      fulfillment_type: item.fulfillmentType || undefined,
    }))

  const buildOrderPayload = () => ({
    coupon_code: normalizedCoupon.value || undefined,
    affiliate_code: getAffiliateCode() || undefined,
    affiliate_visitor_key: getAffiliateVisitorKey() || undefined,
    items: buildItemsPayload(),
    manual_form_data: buildManualFormDataPayload(manualFormProducts.value, manualFormData.value),
  })

  const loadOrderPaymentChannels = async () => {
    if (!auth.isAuthenticated || !requiresOnlineChannel.value || cartItems.value.length === 0 || !preview.value) {
      orderPaymentChannels.value = []
      return
    }
    const id = ++channelsRequestId
    try {
      const res = await userOrderAPI.getPaymentChannels({ amount: centsToAmount(expectedOnlinePayCents.value), items: buildItemsPayload() })
      if (id !== channelsRequestId) return
      orderPaymentChannels.value = Array.isArray(res.data) ? res.data : []
    } catch {
      if (id !== channelsRequestId) return
      orderPaymentChannels.value = Array.isArray(preview.value?.payment_channels) ? preview.value.payment_channels : []
    }
  }
  const debouncedLoadChannels = debounce(() => void loadOrderPaymentChannels(), 250)

  const clearPreview = () => {
    preview.value = null
    orderPaymentChannels.value = []
    previewError.value = ''
    couponRefreshing.value = false
  }

  const loadPreview = async () => {
    if (syncingStock.value || cartItems.value.length === 0) return clearPreview()
    if (isGuestCheckout.value && (!guestEmail.value.trim() || !guestPassword.value.trim() || !guestEmailValid.value)) return clearPreview()
    if (cartItems.value.some(checkoutItemStockExceeded) || cartItems.value.some(checkoutItemMinNotMet)) return clearPreview()
    const id = ++previewRequestId
    previewLoading.value = true
    previewError.value = ''
    try {
      const payload = buildOrderPayload()
      const res = auth.isAuthenticated
        ? await userOrderAPI.preview(payload)
        : await guestOrderAPI.preview({ ...payload, email: guestEmail.value.trim(), order_password: guestPassword.value })
      if (id !== previewRequestId) return
      preview.value = res.data
      if (auth.isAuthenticated) debouncedLoadChannels()
      else orderPaymentChannels.value = []
    } catch (err) {
      if (id !== previewRequestId) return
      preview.value = null
      orderPaymentChannels.value = []
      previewError.value = errorMessage(err, t('checkout.previewFailed'))
    } finally {
      if (id === previewRequestId) {
        previewLoading.value = false
        couponRefreshing.value = false
      }
    }
  }
  const debouncedLoadPreview = debounce(() => void loadPreview(), 300)

  const handleSubmit = async () => {
    submitAttempted.value = true
    error.value = ''
    previewError.value = ''
    if (!canSubmit.value) {
      error.value = submitBlockedReason.value || t('checkout.errors.submitFailed')
      return
    }
    submitting.value = true
    try {
      debouncedLoadPreview.cancel()
      debouncedLoadChannels.cancel()
      await loadPreview()
      if (previewError.value) {
        error.value = previewError.value
        return
      }
      const payload: CreateAndPayPayload = {
        ...buildOrderPayload(),
        channel_id: requiresOnlineChannel.value ? selectedChannelId.value || undefined : undefined,
        use_balance: useBalance.value,
      }
      let orderNo = ''
      if (auth.isAuthenticated) {
        const res = await userOrderAPI.createAndPay(payload)
        orderNo = String(res.data?.order_no || res.data?.order?.order_no || '')
      } else {
        const res = await guestOrderAPI.createAndPay({
          ...payload,
          email: guestEmail.value.trim(),
          order_password: guestPassword.value,
          captcha_payload: captcha.build(),
        })
        saveGuestOrderAuth({ email: guestEmail.value.trim(), order_password: guestPassword.value })
        orderNo = String(res.data?.order_no || res.data?.order?.order_no || '')
      }
      if (!orderNo) throw new Error(t('checkout.errors.submitFailed'))
      if (isBuyNowMode.value) buyNowStore.clear()
      else cartStore.clear()
      await router.push({ path: '/pay', query: auth.isAuthenticated ? { order_no: orderNo } : { guest: '1', order_no: orderNo } })
    } catch (err) {
      error.value = errorMessage(err, t('checkout.errors.submitFailed'))
      if (captcha.enabled.value) captcha.reset()
    } finally {
      submitting.value = false
    }
  }

  // ------------------------------------------------------------ watchers
  watch(
    () => [cartItems.value, manualFingerprint.value, normalizedCoupon.value, checkoutMode.value, guestEmail.value, guestPassword.value, auth.isAuthenticated],
    () => debouncedLoadPreview(),
    { deep: true },
  )
  watch(
    walletOnlyPayment,
    (v) => {
      if (v) useBalance.value = true
    },
    { immediate: true },
  )
  watch(normalizedCoupon, (value, previous) => {
    if (value === previous) return
    couponRefreshing.value = true
    error.value = ''
    previewError.value = ''
  })
  watch(
    () => [auth.isAuthenticated, requiresOnlineChannel.value, expectedOnlinePayCents.value, preview.value?.total_amount],
    () => debouncedLoadChannels(),
  )
  watch(
    () => [paymentChannels.value, expectedOnlinePayCents.value, requiresOnlineChannel.value],
    () => {
      if (!selectedChannelId.value) return
      const selected = paymentChannels.value.find((c) => Number(c.id) === Number(selectedChannelId.value))
      if (!selected || isChannelDisabledForAmount(selected)) selectedChannelId.value = null
    },
    { deep: true },
  )

  const loadWalletBalance = async () => {
    if (!auth.isAuthenticated) return
    walletLoading.value = true
    try {
      const res = await walletAPI.account()
      walletBalance.value = String(res.data?.balance || '0')
    } catch {
      walletBalance.value = '0'
    } finally {
      walletLoading.value = false
    }
  }

  onMounted(async () => {
    if (!appStore.config) await appStore.loadConfig()
    if (!isBuyNowMode.value && !syncingStock.value) {
      syncingStock.value = true
      try {
        await refreshCartStockSnapshots(cartStore)
      } finally {
        syncingStock.value = false
      }
    }
    debouncedLoadPreview()
    void loadWalletBalance()
  })
  onUnmounted(() => {
    debouncedLoadPreview.cancel()
    debouncedLoadChannels.cancel()
  })

  // ------------------------------------------------------------ item display
  const cartItemKey = (item: CartItem) => `${item.productId}:${normalizeSkuId(item.skuId)}`
  const itemSkuDisplay = (item: CartItem) =>
    buildSkuDisplayText({ skuCode: item.skuCode, specValues: item.skuSpecValues, fallback: t('productDetail.skuFallback'), locale: appStore.locale })
  const itemImage = (item: CartItem) => getImageUrl(String(item.image || '').trim())

  const previewItemsByKey = computed(() => {
    const map = new Map<string, OrderItem>()
    for (const item of preview.value?.items ?? []) map.set(`${item.product_id}:${normalizeSkuId(item.sku_id)}`, item)
    return map
  })
  const productQuantities = computed(() => {
    const map = new Map<number, number>()
    for (const item of cartItems.value) {
      const qty = parseInteger(item.quantity)
      if (item.productId > 0 && qty !== null && qty > 0) map.set(item.productId, (map.get(item.productId) || 0) + qty)
    }
    return map
  })
  const itemSubtotalCents = (item: CartItem) => {
    const cents = amountToCents(item.priceAmount)
    const qty = parseInteger(item.quantity)
    return cents === null || qty === null ? null : cents * qty
  }
  const itemWholesaleSubtotalCents = (item: CartItem) => {
    const qty = parseInteger(item.quantity)
    if (qty === null || qty <= 0 || !Array.isArray(item.wholesalePrices)) return null
    const match = productQuantities.value.get(item.productId) || qty
    const unit = amountToCents(
      resolveWholesalePriceAmount({ wholesale_prices: item.wholesalePrices }, item.priceAmount, match, item.skuId, item.skuCode, qty),
    )
    return unit === null ? null : unit * qty
  }
  const itemOriginalCents = (item: CartItem) => {
    const fromPreview = amountToCents(previewItemsByKey.value.get(cartItemKey(item))?.original_total_price)
    return fromPreview !== null ? fromPreview : itemSubtotalCents(item)
  }
  const itemPayableCents = (item: CartItem) => {
    const previewItem = previewItemsByKey.value.get(cartItemKey(item))
    const payable = amountToCents(previewItem?.total_price)
    if (payable !== null) return Math.max(0, payable - (amountToCents(previewItem?.coupon_discount_amount) || 0))
    const wholesale = itemWholesaleSubtotalCents(item)
    return wholesale !== null ? wholesale : itemOriginalCents(item)
  }
  const itemPayableAmount = (item: CartItem) => {
    const c = itemPayableCents(item)
    return c === null ? '' : centsToAmount(Math.max(0, c))
  }
  const itemOriginalAmount = (item: CartItem) => {
    const c = itemOriginalCents(item)
    return c === null ? '' : centsToAmount(Math.max(0, c))
  }
  const itemHasDiscount = (item: CartItem) => {
    const o = itemOriginalCents(item)
    const p = itemPayableCents(item)
    return o !== null && p !== null && o > p
  }

  const itemStockHint = (item: CartItem) => checkoutItemStockHint(item, t)

  return {
    auth,
    captcha,
    getLocalizedText,
    formatPrice,
    isBuyNowMode,
    cartItems,
    totalItems,
    cartItemKey,
    itemImage,
    itemSkuDisplay,
    itemStockExceeded: checkoutItemStockExceeded,
    itemStockHint,
    itemPayableAmount,
    itemOriginalAmount,
    itemHasDiscount,
    manualFormProducts,
    manualFormData,
    submitAttempted,
    manualFieldError,
    couponCode,
    isResellerTenant,
    checkoutMode,
    guestEmail,
    guestPassword,
    guestEmailValid,
    previewCurrency,
    previewOriginal,
    previewCoupon,
    previewPromotion,
    previewWholesale,
    previewMemberDiscount,
    previewTotal,
    previewLoading,
    couponRefreshing,
    previewStatusText,
    checkoutAlert,
    showBalanceOption,
    walletLoading,
    walletBalance,
    useBalance,
    walletOnlyPayment,
    expectedWalletPaidDisplay,
    expectedOnlinePayDisplay,
    expectedOnlinePayCents,
    requiresOnlineChannel,
    paymentChannels,
    selectedChannelId,
    isChannelDisabledForAmount,
    channelAmountLimitHint,
    selectChannel,
    formatChannelFeeRate,
    formatChannelFixedFee,
    submitting,
    canSubmit,
    handleSubmit,
  }
}
