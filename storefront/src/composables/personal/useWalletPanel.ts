import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { walletAPI } from '@/api/wallet'
import type { PaymentChannel, WalletAccountData, WalletTransactionData } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { useAppStore } from '@/stores/app'
import { amountToCents, basisPointsToPercent, centsToAmount, rateToBasisPoints } from '@/utils/money'
import { channelLimit, computeChannelFee, filterRechargeChannels, isChannelOutOfRange } from '@/utils/personal'
import { usePagedList } from './usePagedList'
import { usePanelAlert } from './usePanelAlert'

/** Wallet: balance, recharge form (channel lookup by amount), transactions. */
export function useWalletPanel() {
  const { t, te } = useI18n()
  const route = useRoute()
  const router = useRouter()
  const appStore = useAppStore()
  const { formatPrice } = useLocalized()
  const { alert, show, clear } = usePanelAlert()

  const wallet = ref<WalletAccountData | null>(null)
  const transactions = usePagedList<WalletTransactionData>((p) => walletAPI.transactions(p))
  const recharging = ref(false)
  const form = reactive({ amount: '', channelId: 0, remark: '' })

  const channels = ref<PaymentChannel[]>([])
  const channelLoading = ref(false)
  const resolvedAmount = ref('')
  let fetchSeq = 0
  let fetchTimer: ReturnType<typeof setTimeout> | null = null

  const amountCents = computed(() => amountToCents(form.amount.trim()))
  const validAmount = computed(() => amountCents.value !== null && amountCents.value > 0)

  const hasChannels = computed(() => {
    if (!validAmount.value || channelLoading.value || resolvedAmount.value !== form.amount.trim()) return true
    return channels.value.length > 0
  })
  const selectedChannel = computed(() => channels.value.find((c) => c.id === form.channelId) || null)
  const isSurcharge = computed(() => {
    const policy = String(selectedChannel.value?.fee_policy || '').toLowerCase()
    return policy === 'customer_surcharge' || policy === 'legacy_customer_surcharge'
  })
  const feeRateDisplay = computed(() => {
    const bp = rateToBasisPoints(selectedChannel.value?.fee_rate)
    return `${bp === null ? '0.00' : basisPointsToPercent(bp)}%`
  })
  const fixedFeeDisplay = computed(() => formatPrice(selectedChannel.value?.fixed_fee || '0.00'))
  const feeAmountDisplay = computed(() => formatPrice(computeChannelFee(form.amount.trim(), selectedChannel.value)))

  const amountHint = computed(() => {
    const ch = selectedChannel.value
    if (!ch || !validAmount.value || amountCents.value === null) return ''
    if (!isChannelOutOfRange(ch, amountCents.value)) return ''
    const { minCents, maxCents } = channelLimit(ch)
    const fmt = (c: number) => formatPrice(centsToAmount(c))
    if (minCents !== null && maxCents !== null) return t('payment.channelAmountLimitHint', { min: fmt(minCents), max: fmt(maxCents) })
    if (minCents !== null) return t('payment.channelAmountMinHint', { min: fmt(minCents) })
    if (maxCents !== null) return t('payment.channelAmountMaxHint', { max: fmt(maxCents) })
    return ''
  })

  const channelOptions = computed(() => channels.value.map((c) => ({ value: c.id, label: c.name || c.channel_type })))

  const fetchChannels = async (seq: number, amount: string, cents: number) => {
    try {
      const res = await walletAPI.getPaymentChannels(amount)
      if (seq !== fetchSeq) return
      channels.value = filterRechargeChannels(Array.isArray(res.data) ? res.data : [], cents, appStore.config?.wallet_recharge_channel_ids)
    } catch {
      if (seq !== fetchSeq) return
      channels.value = []
    } finally {
      if (seq === fetchSeq) {
        resolvedAmount.value = amount
        channelLoading.value = false
      }
    }
  }

  const scheduleChannels = () => {
    const amount = form.amount.trim()
    const cents = amountToCents(amount)
    fetchSeq += 1
    if (fetchTimer) clearTimeout(fetchTimer)
    fetchTimer = null
    if (!amount || cents === null || cents <= 0) {
      channelLoading.value = false
      resolvedAmount.value = ''
      channels.value = []
      return
    }
    const seq = fetchSeq
    channelLoading.value = true
    fetchTimer = setTimeout(() => {
      fetchTimer = null
      void fetchChannels(seq, amount, cents)
    }, 300)
  }

  watch(() => form.amount, scheduleChannels)
  watch(channels, (list) => {
    if (list.length === 0) form.channelId = 0
    else if (!list.some((c) => c.id === form.channelId)) form.channelId = list[0]?.id ?? 0
  })

  const balanceDisplay = computed(() => formatPrice(wallet.value?.balance ?? '0.00'))

  const loadWallet = async () => {
    wallet.value = (await walletAPI.account()).data
  }

  const submitRecharge = async () => {
    clear()
    const amount = form.amount.trim()
    if (!validAmount.value) return show('warning', t('personalCenter.wallet.errors.invalidAmount'))
    if (!form.channelId) return show('warning', t('personalCenter.wallet.errors.channelRequired'))
    if (amountHint.value) return show('warning', amountHint.value)
    recharging.value = true
    try {
      const res = await walletAPI.recharge({ amount, channel_id: form.channelId, remark: form.remark.trim() || undefined })
      const rechargeNo = res.data?.recharge?.recharge_no || res.data?.recharge_no || ''
      form.amount = ''
      form.remark = ''
      if (rechargeNo) void router.push(`/recharge-orders/${encodeURIComponent(rechargeNo)}`)
      else show('success', t('personalCenter.wallet.createPaymentSuccess'))
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.wallet.errors.rechargeFailed')))
    } finally {
      recharging.value = false
    }
  }

  const refresh = async () => {
    try {
      await Promise.all([loadWallet(), transactions.load(transactions.pagination.page)])
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.wallet.errors.loadFailed')))
    }
  }

  const typeLabel = (type?: string) => {
    const key = `personalCenter.wallet.types.${type || ''}`
    return type && te(key) ? t(key) : type || '-'
  }
  const directionLabel = (d?: string) =>
    d === 'in' ? t('personalCenter.wallet.directionIn') : d === 'out' ? t('personalCenter.wallet.directionOut') : d || '-'

  /** Payment gateways may return to /me/wallet?recharge_no=… → go to the recharge detail. */
  const redirectRechargeReturn = () => {
    const rechargeNo = String(route.query.recharge_no || '').trim()
    const orderNo = String(route.query.order_no || '').trim()
    const target = rechargeNo || (/^WR/i.test(orderNo) ? orderNo : '')
    if (target) void router.replace(`/recharge-orders/${encodeURIComponent(target)}${window.location.search}`)
  }

  onMounted(async () => {
    try {
      await Promise.all([loadWallet(), transactions.load(1)])
      redirectRechargeReturn()
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.wallet.errors.loadFailed')))
    }
  })
  onBeforeUnmount(() => {
    fetchSeq += 1
    if (fetchTimer) clearTimeout(fetchTimer)
  })

  return {
    alert,
    wallet,
    balanceDisplay,
    transactions,
    form,
    recharging,
    channelOptions,
    channelLoading,
    hasChannels,
    selectedChannel,
    isSurcharge,
    feeRateDisplay,
    fixedFeeDisplay,
    feeAmountDisplay,
    amountHint,
    submitRecharge,
    refresh,
    typeLabel,
    directionLabel,
  }
}
