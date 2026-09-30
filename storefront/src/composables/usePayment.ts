import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { isNavigationFailure, NavigationFailureType, useRoute, useRouter, type RouteLocationRaw } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { guestOrderAPI, paymentAPI, userOrderAPI, type GuestCredentials } from '@/api/order'
import type { CreatePaymentPayload, Order, OrderPaymentChannelsPayload, PaymentChannel, PaymentCreateResult } from '@/api/types'
import { walletAPI } from '@/api/wallet'
import { useLocalized } from '@/composables/useLocalized'
import { usePaymentLink } from '@/composables/usePaymentLink'
import { useAppStore } from '@/stores/app'
import { debounce } from '@/utils/debounce'
import { formatCountdown, formatDateTime } from '@/utils/format'
import { loadGuestOrderAuth, saveGuestOrderAuth } from '@/utils/guestOrderAuth'
import { amountToCents, centsToAmount } from '@/utils/money'
import {
  channelLimitMeta,
  filterSupportedChannels,
  isChannelDisabledForAmount as channelDisabled,
  PAYMENT_RETURN_MARKERS,
  readQueryFlag,
  readQueryValue,
  isRechargeReturn as isRechargeReturnQuery,
  resolveRechargeReturnNo,
  restrictChannels,
  splitWalletPayment,
} from '@/utils/orderPayment'
import {
  getCachedPaymentRestorePolicy,
  getPaymentResetPolicy,
  isCustomerSurchargePayment,
  resolvePaymentInteractionLabelKey,
  resolvePaymentResultTitleKey,
  shouldAutoOpenPaymentLink,
  type PaymentResetReason,
} from '@/utils/paymentResumePolicy'
import { PAID_ORDER_STATUSES } from '@/utils/status'
import { paymentChoiceLabel, paymentChoices, paymentTypeLabel } from '@/utils/paymentMethods'

/** Poll interval while waiting for the payment (original: 5s). */
const POLL_INTERVAL_MS = 5000
/** Delay before redirecting to the order detail once paid (original: 600ms). */
const PAID_REDIRECT_DELAY_MS = 600

export type PaymentAlert = { level: 'success' | 'error'; message: string }

const CHANNEL_TYPE_KEYS: Record<string, string> = {
  wechat: 'wechat',
  wxpay: 'wxpay',
  alipay: 'alipay',
  qqpay: 'qqpay',
  paypal: 'paypal',
  stripe: 'stripe',
  usdt: 'usdt',
  'usdt-trc20': 'usdtTrc20',
  'usdc-trc20': 'usdcTrc20',
  trx: 'trx',
}

/** Payment page logic (port of the original usePayment). */
export function usePayment() {
  const route = useRoute()
  const router = useRouter()
  const appStore = useAppStore()
  const { t } = useI18n()
  const { getLocalizedText, formatPrice } = useLocalized()

  const loading = ref(true)
  const submitting = ref(false)
  const order = ref<Order | null>(null)
  const paymentResult = ref<PaymentCreateResult | null>(null)
  const cachedPayment = ref<PaymentCreateResult | null>(null)
  const error = ref('')
  const selectedChannelId = ref<number | null>(null)
  const selectedChannelType = ref('')
  const paymentMethodExpanded = ref(false)
  const capturing = ref(false)
  const redirecting = ref(false)
  let redirected = false
  let latestLoaded = false
  const guestAuth = ref<GuestCredentials>({ email: '', order_password: '' })
  const guestAuthError = ref('')
  const pollTimer = ref<number | null>(null)
  let countdownTimer: number | null = null
  let redirectTimer: number | null = null
  const now = ref(appStore.getServerTime())
  const walletLoading = ref(false)
  const walletBalance = ref('0')
  const useBalance = ref(false)
  const orderPaymentChannels = ref<PaymentChannel[]>([])
  const orderPaymentChannelsLoaded = ref(false)
  let channelsRequestId = 0

  const query = computed(() => route.query as Record<string, unknown>)
  const isGuest = computed(() => readQueryFlag(query.value, 'guest'))
  const orderNoQuery = computed(() => readQueryValue(query.value, 'order_no') || readQueryValue(query.value, 'out_trade_no'))
  const orderNoResolved = computed(() => order.value?.order_no || orderNoQuery.value || '')
  const backLink = computed(() => (isGuest.value ? '/guest/orders' : '/me/orders'))
  const hasGuestAuth = computed(() => Boolean(guestAuth.value.email && guestAuth.value.order_password))
  const showGuestAuthForm = computed(() => isGuest.value && (!hasGuestAuth.value || !!guestAuthError.value))
  const walletOnlyPayment = computed(() => !!appStore.config?.wallet_only_payment)
  const showBalanceOption = computed(() => !isGuest.value)
  const currency = computed(() => order.value?.currency || appStore.currency)
  const money = (amount: string | number | null | undefined, cur?: string | null) => formatPrice(amount, cur === undefined ? currency.value : cur)

  // ------------------------------------------------------------ channels
  const channels = computed<PaymentChannel[]>(() => {
    const source: PaymentChannel[] =
      !isGuest.value && orderPaymentChannelsLoaded.value ? orderPaymentChannels.value : (appStore.config?.payment_channels ?? [])
    const allowed = order.value?.allowed_payment_channel_ids
    return restrictChannels(filterSupportedChannels(source), Array.isArray(allowed) && allowed.length > 0 ? allowed.map(Number) : null)
  })
  const configReady = computed(() => !appStore.loading && (!!appStore.config || (!isGuest.value && orderPaymentChannelsLoaded.value)))
  const findChannel = (id: unknown) => {
    const target = String(id ?? '').trim()
    if (!target) return null
    return channels.value.find((c) => String(c.id) === target) || null
  }
  const channelTypeLabel = (value?: string) => {
    if (!value) return '-'
    const key = CHANNEL_TYPE_KEYS[value]
    return key ? t(`payment.channelTypes.${key}`) : value
  }
  const methodDisplayName = (channel: PaymentChannel | null, type?: string) => {
    if (!channel || !type) return channel?.name || channelTypeLabel(type)
    const allChoices = paymentChoices(channels.value)
    const choice = allChoices.find((item) => Number(item.channel.id) === Number(channel.id) && item.type === type)
    return choice ? paymentChoiceLabel(choice, allChoices, (key) => t(key)) : paymentTypeLabel(type, (key) => t(key))
  }
  const selectedChannel = computed(() => findChannel(selectedChannelId.value))
  const selectedChannelName = computed(() => methodDisplayName(selectedChannel.value, selectedChannelType.value))
  const checkoutChannelId = computed(() => Number(readQueryValue(query.value, 'channel_id')) || null)
  const checkoutChannelType = computed(() => readQueryValue(query.value, 'channel_type').trim())
  const checkoutSelectionActive = computed(() => readQueryFlag(query.value, 'checkout'))
  const showChannelSelector = computed(() => {
    if (paymentMethodExpanded.value || !checkoutSelectionActive.value) return true
    const channel = findChannel(checkoutChannelId.value)
    const validChoice = channel && paymentChoices([channel]).some((choice) => choice.type === checkoutChannelType.value)
    return !validChoice || isChannelDisabledForAmount(channel)
  })
  const cachedChannelName = computed(() => methodDisplayName(findChannel(cachedPayment.value?.channel_id), cachedPayment.value?.channel_type))
  const resultChannel = computed(() => findChannel(paymentResult.value?.channel_id))
  const resultChannelName = computed(() => methodDisplayName(resultChannel.value, paymentResult.value?.channel_type))

  const currentPaymentId = () => {
    const id = Number(paymentResult.value?.payment_id || 0)
    return Number.isFinite(id) && id > 0 ? id : 0
  }
  const providerType = computed(() => String(paymentResult.value?.provider_type || resultChannel.value?.provider_type || '').toLowerCase())
  const channelType = computed(() => String(paymentResult.value?.channel_type || resultChannel.value?.channel_type || '').toLowerCase())

  const link = usePaymentLink(paymentResult)
  const interactionLabel = computed(() => {
    const mode = link.interactionMode.value
    if (!mode) return '-'
    const key = resolvePaymentInteractionLabelKey(mode)
    return key ? t(key) : mode
  })
  const paymentResultTitle = computed(() => t(resolvePaymentResultTitleKey(link.interactionMode.value)))
  const paymentGuideTitle = computed(() => (link.presentationMode.value === 'redirect' ? t('payment.redirectTitle') : t('payment.qrTitle')))
  const paymentGuideTip = computed(() => (link.presentationMode.value === 'redirect' ? t('payment.redirectTip') : t('payment.qrTip')))
  const payLinkOpenedTip = computed(() => (link.telegramMiniApp ? t('payment.redirectOpenedTelegram') : t('payment.redirectOpened')))

  watch(
    walletOnlyPayment,
    (v) => {
      if (v) useBalance.value = true
    },
    { immediate: true },
  )

  // ------------------------------------------------------------ time state
  const parseTime = (raw: string | null | undefined) => {
    if (!raw) return null
    const ts = new Date(raw).getTime()
    return Number.isNaN(ts) ? null : ts
  }
  const expiresAtMs = computed(() => parseTime(paymentResult.value?.expires_at || order.value?.expires_at))
  const orderExpired = computed(() => {
    const ts = parseTime(order.value?.expires_at)
    return ts !== null && ts <= now.value
  })
  const orderCanceled = computed(() => order.value?.status === 'canceled')
  const remainingMs = computed(() => (expiresAtMs.value === null ? null : expiresAtMs.value - now.value))
  const countdownExpired = computed(() => remainingMs.value !== null && remainingMs.value <= 0)
  const countdownText = computed(() => {
    if (remainingMs.value === null) return '-'
    if (remainingMs.value <= 0) return t('payment.countdownExpired')
    return formatCountdown(remainingMs.value / 1000)
  })
  const showCountdown = computed(() => Boolean(expiresAtMs.value && order.value?.status === 'pending_payment'))
  const showResultView = computed(() =>
    Boolean(paymentResult.value && order.value && order.value.status === 'pending_payment' && !orderExpired.value && !orderCanceled.value),
  )
  const pollingActive = computed(() => pollTimer.value !== null)

  const paymentAlert = computed<PaymentAlert | null>(() => {
    if (redirecting.value) return { level: 'success', message: t('payment.redirecting') }
    if (orderCanceled.value) return { level: 'error', message: t('payment.orderCanceled') }
    if (orderExpired.value) return { level: 'error', message: t('payment.orderExpired') }
    if (error.value) return { level: 'error', message: error.value }
    return null
  })

  // ------------------------------------------------------------ amounts
  const orderItems = computed(() => order.value?.items ?? [])
  const customerFeeApplied = computed(() => isCustomerSurchargePayment(paymentResult.value) && (amountToCents(paymentResult.value?.fee_amount) || 0) > 0)
  const customerFeeAmountDisplay = computed(() => (customerFeeApplied.value ? money(String(paymentResult.value?.fee_amount || '0')) : ''))
  const payableAmountDisplay = computed(() => {
    const r = paymentResult.value
    const cur = String(r?.currency || '').trim() || currency.value
    if (r?.payable_amount) return money(r.payable_amount, cur)
    if (r?.online_pay_amount) return money(r.online_pay_amount, cur)
    return money(order.value?.total_amount ?? '')
  })
  const walletBalanceDisplay = computed(() => money(walletBalance.value))
  const split = computed(() => splitWalletPayment(order.value?.total_amount, walletBalance.value, showBalanceOption.value && useBalance.value))
  const expectedOnlinePayCents = computed(() => split.value.onlineCents)
  const expectedWalletPaidDisplay = computed(() => money(centsToAmount(split.value.walletCents)))
  const expectedOnlinePayDisplay = computed(() => money(centsToAmount(split.value.onlineCents)))
  const requiresOnlineChannel = computed(() => isGuest.value || !useBalance.value || expectedOnlinePayCents.value > 0)
  const paymentWalletPaidDisplay = computed(() => (paymentResult.value?.wallet_paid_amount ? money(paymentResult.value.wallet_paid_amount) : '-'))
  const paymentOnlinePayDisplay = computed(() => (paymentResult.value?.online_pay_amount ? money(paymentResult.value.online_pay_amount) : '-'))

  const isChannelDisabledForAmount = (channel: PaymentChannel) => channelDisabled(channel, expectedOnlinePayCents.value, requiresOnlineChannel.value)
  const channelAmountLimitHint = (channel: PaymentChannel) => {
    const meta = channelLimitMeta(channel)
    const fmt = (c: number | null) => money(centsToAmount(c ?? 0))
    if (meta.hasMin && meta.hasMax) return t('payment.channelAmountLimitHint', { min: fmt(meta.minCents), max: fmt(meta.maxCents) })
    if (meta.hasMin) return t('payment.channelAmountMinHint', { min: fmt(meta.minCents) })
    if (meta.hasMax) return t('payment.channelAmountMaxHint', { max: fmt(meta.maxCents) })
    return ''
  }
  const selectedChannelAmountHint = computed(() => {
    const ch = findChannel(selectedChannelId.value)
    return ch && isChannelDisabledForAmount(ch) ? channelAmountLimitHint(ch) : ''
  })
  const canSubmitPayment = computed(() => {
    if (submitting.value) return false
    if (walletOnlyPayment.value && expectedOnlinePayCents.value > 0) return false
    if (!walletOnlyPayment.value && requiresOnlineChannel.value && !selectedChannelId.value) return false
    if (requiresOnlineChannel.value && selectedChannelAmountHint.value) return false
    return !orderExpired.value && !orderCanceled.value
  })

  // ------------------------------------------------------------ loaders
  const loadOrderPaymentChannels = async () => {
    if (isGuest.value || !orderNoResolved.value) {
      orderPaymentChannels.value = []
      orderPaymentChannelsLoaded.value = false
      return
    }
    const amount = centsToAmount(expectedOnlinePayCents.value)
    if (!requiresOnlineChannel.value || (amountToCents(amount) ?? 0) <= 0) {
      orderPaymentChannels.value = []
      orderPaymentChannelsLoaded.value = true
      return
    }
    const id = ++channelsRequestId
    try {
      const payload: OrderPaymentChannelsPayload = { order_no: orderNoResolved.value, amount }
      const res = await userOrderAPI.getPaymentChannels(payload)
      if (id !== channelsRequestId) return
      orderPaymentChannels.value = Array.isArray(res.data) ? res.data : []
      orderPaymentChannelsLoaded.value = true
    } catch {
      if (id !== channelsRequestId) return
      orderPaymentChannels.value = []
      orderPaymentChannelsLoaded.value = false
    }
  }
  const debouncedLoadChannels = debounce(() => void loadOrderPaymentChannels(), 250)

  const loadWallet = async () => {
    if (isGuest.value) return
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

  const startCountdown = () => {
    if (!expiresAtMs.value || countdownTimer !== null || order.value?.status !== 'pending_payment') return
    now.value = appStore.getServerTime()
    countdownTimer = window.setInterval(() => {
      now.value = appStore.getServerTime()
    }, 1000)
  }
  const stopCountdown = () => {
    if (countdownTimer === null) return
    window.clearInterval(countdownTimer)
    countdownTimer = null
  }

  const guestCreds = (): GuestCredentials => ({ email: guestAuth.value.email, order_password: guestAuth.value.order_password })

  const captureById = async (paymentId: number) => {
    if (isGuest.value) await guestOrderAPI.capturePayment(paymentId, guestCreds())
    else await paymentAPI.capture(paymentId)
  }

  /** Official WeChat needs an active capture poll (original behaviour). */
  const shouldCaptureCurrentPayment = () =>
    !!currentPaymentId() && order.value?.status === 'pending_payment' && providerType.value === 'official' && channelType.value === 'wechat'

  const captureCurrentPayment = async (silent = false) => {
    if (capturing.value || !shouldCaptureCurrentPayment()) return
    capturing.value = true
    if (!silent) error.value = ''
    try {
      if (isGuest.value && !hasGuestAuth.value) {
        if (!silent) guestAuthError.value = t('payment.guestAuthRequired')
        return
      }
      await captureById(currentPaymentId())
    } catch (err) {
      if (!silent) error.value = errorMessage(err, t('payment.captureFailed'))
    } finally {
      capturing.value = false
    }
  }

  const loadLatestPayment = async () => {
    if (!order.value || order.value.status !== 'pending_payment' || paymentResult.value) return
    if (isGuest.value && !hasGuestAuth.value) return
    if (!orderNoResolved.value) return
    try {
      const res = isGuest.value
        ? await guestOrderAPI.latestPayment(guestCreds(), { order_no: orderNoResolved.value })
        : await paymentAPI.latest({ order_no: orderNoResolved.value })
      const data = res.data
      if (data && (data.pay_url || data.qr_code)) {
        cachedPayment.value = data
        paymentResult.value = data
        selectedChannelId.value = data.channel_id || null
        selectedChannelType.value = data.channel_type || ''
        startPolling()
        void captureCurrentPayment(true)
        startCountdown()
        if (shouldAutoOpenPaymentLink(data)) link.openPayLink(true)
      }
    } catch {
      // no previous payment
    }
  }

  const loadOrder = async (silentRequest = false) => {
    const silent = silentRequest && !!order.value
    if (!silent) loading.value = true
    try {
      if (!orderNoQuery.value) {
        order.value = null
        orderPaymentChannels.value = []
        orderPaymentChannelsLoaded.value = false
        return
      }
      if (isGuest.value) {
        if (!hasGuestAuth.value) {
          order.value = null
          guestAuthError.value = t('payment.guestAuthRequired')
          return
        }
        const res = await guestOrderAPI.detail(orderNoQuery.value, guestCreds(), { silentBusinessError: true })
        order.value = res.data
        guestAuthError.value = ''
      } else {
        const res = await userOrderAPI.detail(orderNoQuery.value, { silentBusinessError: true })
        order.value = res.data
      }
    } catch {
      if (!silent) {
        order.value = null
        orderPaymentChannels.value = []
        orderPaymentChannelsLoaded.value = false
        if (isGuest.value) guestAuthError.value = t('payment.guestAuthInvalid')
      }
    } finally {
      try {
        if (order.value) {
          now.value = appStore.getServerTime()
          if (orderCanceled.value) {
            if (!silent) error.value = t('payment.orderCanceled')
            cachedPayment.value = null
          } else if (orderExpired.value) {
            if (!silent) error.value = t('payment.orderExpired')
            cachedPayment.value = null
          } else if (!paymentResult.value && !latestLoaded && order.value.status === 'pending_payment') {
            latestLoaded = true
            await loadLatestPayment()
          }
        }
      } finally {
        if (!silent) loading.value = false
      }
    }
  }
  const debouncedLoadOrder = debounce((silent: boolean) => void loadOrder(silent), 250)

  function startPolling() {
    if (pollTimer.value !== null) return
    pollTimer.value = window.setInterval(async () => {
      await captureCurrentPayment(true)
      await loadOrder(true)
    }, POLL_INTERVAL_MS)
  }
  const stopPolling = () => {
    if (pollTimer.value === null) return
    window.clearInterval(pollTimer.value)
    pollTimer.value = null
  }

  const buildPayRouteQuery = () => {
    const q: Record<string, string> = {}
    const no = String(order.value?.order_no || orderNoQuery.value || '').trim()
    if (no) q.order_no = no
    if (isGuest.value) q.guest = '1'
    if (checkoutSelectionActive.value && checkoutChannelId.value && checkoutChannelType.value) {
      q.checkout = '1'
      q.channel_id = String(checkoutChannelId.value)
      q.channel_type = checkoutChannelType.value
    }
    return q
  }

  /** PayPal (pp_return/token/PayerID) and Stripe (stripe_return/session_id) returns. */
  const captureReturnIfNeeded = async () => {
    if (capturing.value) return
    const paymentId = currentPaymentId()
    if (!paymentId || providerType.value !== 'official') return
    let matched = false
    if (channelType.value === 'paypal') {
      matched =
        readQueryValue(query.value, 'pp_return') === '1' ||
        !!readQueryValue(query.value, 'token') ||
        !!(readQueryValue(query.value, 'payer_id') || readQueryValue(query.value, 'PayerID'))
    } else if (channelType.value === 'stripe') {
      matched = readQueryValue(query.value, 'stripe_return') === '1' || !!readQueryValue(query.value, 'session_id')
    }
    if (!matched || !orderNoResolved.value || order.value?.status !== 'pending_payment') return
    capturing.value = true
    error.value = ''
    try {
      if (isGuest.value && !hasGuestAuth.value) {
        guestAuthError.value = t('payment.guestAuthRequired')
        return
      }
      await captureById(paymentId)
      await loadOrder(true)
      await router.replace({ path: route.path, query: buildPayRouteQuery() })
    } catch (err) {
      error.value = errorMessage(err, t('payment.captureFailed'))
    } finally {
      capturing.value = false
    }
  }

  const syncPaymentReturnIfNeeded = async () => {
    const hasReturn = PAYMENT_RETURN_MARKERS.some((m) => readQueryValue(query.value, m).toLowerCase() === '1')
    if (!hasReturn || !orderNoQuery.value) return
    try {
      await loadOrder(true)
      await loadLatestPayment()
    } finally {
      await router.replace({ path: route.path, query: buildPayRouteQuery() })
    }
  }

  const redirectToOrderDetail = () => {
    if (redirected || redirecting.value) return
    const no = String(order.value?.order_no || orderNoQuery.value || '').trim()
    if (!no) return
    const target: RouteLocationRaw = isGuest.value ? { name: 'guest-order-detail', params: { order_no: no } } : { name: 'order-detail', params: { order_no: no } }
    const fallback = isGuest.value ? `/guest/orders/${encodeURIComponent(no)}` : `/orders/${encodeURIComponent(no)}`
    redirected = true
    redirecting.value = true
    if (redirectTimer !== null) window.clearTimeout(redirectTimer)
    redirectTimer = window.setTimeout(async () => {
      redirectTimer = null
      try {
        const failure = await router.push(target)
        if (failure && !isNavigationFailure(failure, NavigationFailureType.duplicated)) window.location.assign(fallback)
      } catch {
        window.location.assign(fallback)
      }
    }, PAID_REDIRECT_DELAY_MS)
  }

  const activatePayment = (result: PaymentCreateResult) => {
    paymentResult.value = result
    if (result.pay_url || result.qr_code) cachedPayment.value = result
    link.openedPayWindow.value = false
    startPolling()
    void captureCurrentPayment(true)
    startCountdown()
  }

  const performPayment = async () => {
    error.value = ''
    if (!orderNoResolved.value) return void (error.value = t('payment.orderNotFound'))
    if (requiresOnlineChannel.value && (!selectedChannelId.value || !selectedChannelType.value)) return void (error.value = t('payment.selectChannelError'))
    if (requiresOnlineChannel.value && selectedChannelAmountHint.value) return void (error.value = selectedChannelAmountHint.value)
    if (orderCanceled.value) return void (error.value = t('payment.orderCanceled'))
    if (orderExpired.value) return void (error.value = t('payment.orderExpired'))
    if (
      requiresOnlineChannel.value && cachedPayment.value && selectedChannelId.value &&
      selectedChannelId.value === cachedPayment.value.channel_id && selectedChannelType.value === cachedPayment.value.channel_type
    ) {
      activatePayment(cachedPayment.value)
      window.scrollTo({ top: 0, behavior: 'smooth' })
      return
    }
    submitting.value = true
    try {
      if (isGuest.value) {
        if (!hasGuestAuth.value) return void (error.value = t('payment.guestAuthRequired'))
        const res = await guestOrderAPI.createPayment(guestCreds(), {
          order_no: orderNoResolved.value,
          channel_id: selectedChannelId.value || undefined,
          channel_type: selectedChannelType.value,
        })
        activatePayment(res.data)
      } else {
        const payload: CreatePaymentPayload = { order_no: orderNoResolved.value, use_balance: useBalance.value }
        if (requiresOnlineChannel.value && selectedChannelId.value) {
          payload.channel_id = selectedChannelId.value
          payload.channel_type = selectedChannelType.value
        }
        const res = await paymentAPI.create(payload)
        const created = res.data || {}
        if (created.order_paid && !created.payment_id) {
          paymentResult.value = null
          cachedPayment.value = null
          selectedChannelId.value = null
          selectedChannelType.value = ''
          useBalance.value = false
          stopPolling()
          stopCountdown()
          await Promise.all([loadOrder(true), loadWallet()])
          redirectToOrderDetail()
          return
        }
        activatePayment(created)
        await loadWallet()
      }
      window.scrollTo({ top: 0, behavior: 'smooth' })
      if (shouldAutoOpenPaymentLink(paymentResult.value)) link.openPayLink(true)
    } catch (err) {
      error.value = errorMessage(err, t('payment.createFailed'))
    } finally {
      submitting.value = false
    }
  }
  const handlePayment = debounce(() => void performPayment(), 200)

  const resetPayment = (reason: PaymentResetReason = 'generic') => {
    const policy = getPaymentResetPolicy(reason)
    if (policy.stopActivePaymentWatch) {
      stopPolling()
      stopCountdown()
      debouncedLoadOrder.cancel()
    }
    paymentResult.value = null
    error.value = ''
    link.openedPayWindow.value = false
    redirecting.value = false
    redirected = false
    latestLoaded = !policy.resumeLatestPayment
    if (policy.clearSelectedChannel) {
      selectedChannelId.value = null
      selectedChannelType.value = ''
    }
  }

  const restoreCachedPayment = () => {
    if (!cachedPayment.value) return
    const policy = getCachedPaymentRestorePolicy()
    paymentResult.value = cachedPayment.value
    selectedChannelId.value = cachedPayment.value.channel_id || null
    selectedChannelType.value = cachedPayment.value.channel_type || ''
    link.openedPayWindow.value = false
    if (policy.startActivePaymentWatch) {
      startPolling()
      void captureCurrentPayment(true)
      startCountdown()
    }
    if (policy.autoOpenPayLink && shouldAutoOpenPaymentLink(paymentResult.value)) link.openPayLink(true)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const handleChangePaymentMethod = () => {
    paymentMethodExpanded.value = true
    if (paymentResult.value) resetPayment('change_payment_method')
    const q: Record<string, string> = { order_no: orderNoResolved.value }
    if (isGuest.value) q.guest = '1'
    void router.replace({ path: route.path, query: q })
  }

  const handleGuestAuthSubmit = async () => {
    guestAuthError.value = ''
    if (!hasGuestAuth.value) {
      guestAuthError.value = t('payment.guestAuthRequired')
      return
    }
    saveGuestOrderAuth(guestCreds())
    await loadOrder()
  }

  const handleRefresh = async () => {
    await captureCurrentPayment()
    await Promise.all([loadOrder(), loadWallet(), loadOrderPaymentChannels()])
  }

  // ------------------------------------------------------------ lifecycle
  onMounted(() => {
    if (isRechargeReturnQuery(query.value)) {
      const rechargeNo = resolveRechargeReturnNo(query.value)
      if (rechargeNo) {
        const q: Record<string, string> = {}
        for (const key of [...PAYMENT_RETURN_MARKERS, 'token', 'payer_id', 'PayerID', 'session_id']) {
          const v = readQueryValue(query.value, key)
          if (v) q[key] = v
        }
        void router.replace({ path: `/recharge-orders/${encodeURIComponent(rechargeNo)}`, query: q })
        return
      }
    }
    if (!orderNoQuery.value) {
      loading.value = false
      return
    }
    guestAuth.value = loadGuestOrderAuth()
    void loadOrder()
    void loadWallet()
    if (!appStore.config || !Array.isArray(appStore.config.payment_channels)) void appStore.loadConfig(true)
  })

  watch(
    () => order.value?.status,
    (status) => {
      if (!status) return
      if (status === 'canceled') {
        error.value = t('payment.orderCanceled')
        stopPolling()
        stopCountdown()
        return
      }
      if (status === 'pending_payment') {
        startPolling()
        startCountdown()
        return
      }
      stopPolling()
      stopCountdown()
      if (PAID_ORDER_STATUSES.has(status)) redirectToOrderDetail()
    },
  )
  watch(
    () => [isGuest.value, orderNoQuery.value] as const,
    async ([, no], [, prevNo]) => {
      if (!prevNo || no === prevNo) return
      stopPolling()
      stopCountdown()
      resetPayment('route_change')
      cachedPayment.value = null
      selectedChannelId.value = null
      selectedChannelType.value = ''
      order.value = null
      orderPaymentChannels.value = []
      orderPaymentChannelsLoaded.value = false
      if (!no) return
      await loadOrder()
      void loadWallet()
    },
  )
  watch(
    () => [isGuest.value, orderNoResolved.value, requiresOnlineChannel.value, expectedOnlinePayCents.value, order.value?.status],
    () => debouncedLoadChannels(),
    { immediate: true },
  )
  watch(
    () => [currentPaymentId(), providerType.value, channelType.value, route.fullPath, order.value?.status],
    () => {
      void captureReturnIfNeeded()
      void syncPaymentReturnIfNeeded()
    },
    { immediate: true },
  )
  watch(
    () => [channels.value, expectedOnlinePayCents.value, requiresOnlineChannel.value],
    () => {
      const choices = paymentChoices(channels.value)
      if (checkoutSelectionActive.value && !paymentMethodExpanded.value && checkoutChannelId.value && checkoutChannelType.value) {
        const preferred = choices.find((choice) => Number(choice.channel.id) === checkoutChannelId.value && choice.type === checkoutChannelType.value)
        if (preferred && !isChannelDisabledForAmount(preferred.channel)) {
          selectedChannelId.value = checkoutChannelId.value
          selectedChannelType.value = checkoutChannelType.value
        }
      }
      if (choices.length === 1) {
        selectedChannelId.value = Number(choices[0].channel.id) || null
        selectedChannelType.value = choices[0].type
        return
      }
      if (!selectedChannelId.value) return
      const ch = findChannel(selectedChannelId.value)
      const typeAvailable = ch && paymentChoices([ch]).some((choice) => choice.type === selectedChannelType.value)
      if (!ch || isChannelDisabledForAmount(ch) || !typeAvailable) {
        selectedChannelId.value = null
        selectedChannelType.value = ''
      }
    },
    { deep: true },
  )
  watch(expiresAtMs, (value) => {
    stopCountdown()
    if (value && order.value?.status === 'pending_payment') startCountdown()
  })
  watch(remainingMs, (value) => {
    if (value !== null && value <= 0) stopCountdown()
  })

  onUnmounted(() => {
    stopPolling()
    stopCountdown()
    if (redirectTimer !== null) window.clearTimeout(redirectTimer)
    debouncedLoadOrder.cancel()
    debouncedLoadChannels.cancel()
    handlePayment.cancel()
  })

  return {
    loading,
    submitting,
    order,
    paymentResult,
    selectedChannelId,
    selectedChannelType,
    cachedPayment,
    guestAuth,
    guestAuthError,
    walletLoading,
    walletBalance,
    useBalance,
    backLink,
    showGuestAuthForm,
    walletOnlyPayment,
    showBalanceOption,
    configReady,
    channels,
    selectedChannel,
    selectedChannelName,
    showChannelSelector,
    cachedChannelName,
    resultChannelName,
    interactionLabel,
    paymentResultTitle,
    paymentGuideTitle,
    paymentGuideTip,
    payLinkOpenedTip,
    link,
    orderExpired,
    orderCanceled,
    paymentAlert,
    countdownExpired,
    countdownText,
    showCountdown,
    showResultView,
    pollingActive,
    orderItems,
    customerFeeApplied,
    customerFeeAmountDisplay,
    payableAmountDisplay,
    walletBalanceDisplay,
    expectedWalletPaidDisplay,
    expectedOnlinePayDisplay,
    expectedOnlinePayCents,
    requiresOnlineChannel,
    paymentWalletPaidDisplay,
    paymentOnlinePayDisplay,
    isChannelDisabledForAmount,
    channelAmountLimitHint,
    canSubmitPayment,
    isGuest,
    money,
    formatDate: formatDateTime,
    getLocalizedText,
    restoreCachedPayment,
    handleChangePaymentMethod,
    handlePayment,
    handleGuestAuthSubmit,
    handleRefresh,
  }
}
