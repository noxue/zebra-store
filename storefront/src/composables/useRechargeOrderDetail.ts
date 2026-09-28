import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { walletAPI } from '@/api/wallet'
import type { RechargeDetailData, RechargePaymentData, WalletRechargeOrderData } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { usePaymentLink } from '@/composables/usePaymentLink'
import { toast } from '@/composables/useToast'
import { formatDateTime } from '@/utils/format'
import { isCustomerSurchargePayment, shouldAutoOpenPaymentLink } from '@/utils/paymentResumePolicy'
import type { BadgeTone } from '@/utils/status'

/** Poll interval while the recharge is pending (original: 5s). */
const POLL_INTERVAL_MS = 5000
const FINAL_STATUSES = new Set(['success', 'failed', 'expired'])

/** Extracts recharge + payment from a detail / capture payload. */
export const splitRechargePayload = (payload: RechargeDetailData | null | undefined) => {
  const recharge = payload?.recharge ?? null
  let payment: RechargePaymentData | null = payload?.payment ?? null
  if (!payment && payload && payload.payment_id != null) {
    payment = {
      id: payload.payment_id,
      payment_id: payload.payment_id,
      provider_type: payload.provider_type,
      channel_type: payload.channel_type,
      interaction_mode: payload.interaction_mode,
      pay_url: payload.pay_url,
      qr_code: payload.qr_code,
      wallet_address: payload.wallet_address,
      chain_amount: payload.chain_amount,
      chain: payload.chain,
      token_id: payload.token_id,
      expires_at: payload.expires_at,
      status: payload.status,
      fee_policy: payload.fee_policy,
    }
  }
  return { recharge, payment }
}

/** Recharge (wallet top-up) order detail with payment polling. */
export function useRechargeOrderDetail() {
  const { t, te } = useI18n()
  const route = useRoute()
  const { formatPrice } = useLocalized()
  const loading = ref(true)
  const checkingPayment = ref(false)
  const recharge = ref<WalletRechargeOrderData | null>(null)
  const payment = ref<RechargePaymentData | null>(null)
  let pollTimer: number | null = null

  const rechargeNo = computed(() => String(route.params.recharge_no || '').trim())
  const isPending = computed(() => ['pending', 'initiated'].includes(String(recharge.value?.status || '').toLowerCase()))
  const link = usePaymentLink(payment)
  const customerFeeApplied = computed(() => isCustomerSurchargePayment(payment.value) && Number(recharge.value?.fee_amount || 0) > 0)

  const statusText = (status?: string) => {
    const s = String(status || '').toLowerCase()
    const key = `personalCenter.wallet.rechargeStatus.${s}`
    return te(key) ? t(key) : s || '-'
  }
  const statusTone = (status?: string): BadgeTone => {
    const s = String(status || '').toLowerCase()
    if (s === 'success') return 'success'
    if (s === 'failed' || s === 'expired') return 'danger'
    return 'warning'
  }

  const sync = (payload: RechargeDetailData | null | undefined) => {
    const next = splitRechargePayload(payload)
    if (next.recharge) recharge.value = next.recharge
    if (next.payment) payment.value = next.payment
  }

  const stopPolling = () => {
    if (pollTimer !== null) window.clearInterval(pollTimer)
    pollTimer = null
  }
  const startPolling = () => {
    if (!isPending.value || pollTimer !== null) return
    pollTimer = window.setInterval(() => void refreshStatus(), POLL_INTERVAL_MS)
  }

  const loadDetail = async () => {
    if (!rechargeNo.value) return
    loading.value = true
    try {
      sync((await walletAPI.rechargeDetail(rechargeNo.value)).data)
    } catch {
      recharge.value = null
    } finally {
      loading.value = false
    }
  }

  async function refreshStatus() {
    if (!rechargeNo.value) return
    try {
      sync((await walletAPI.rechargeDetail(rechargeNo.value)).data)
      if (FINAL_STATUSES.has(String(recharge.value?.status || '').toLowerCase())) stopPolling()
      else startPolling()
    } catch {
      // keep polling silently
    }
  }

  const checkPayment = async () => {
    const id = Number(payment.value?.id || payment.value?.payment_id || 0)
    if (!Number.isFinite(id) || id <= 0) return
    checkingPayment.value = true
    try {
      const res = await walletAPI.captureRechargePayment(id)
      sync(res.data as RechargeDetailData)
      await refreshStatus()
    } catch {
      toast.error(t('personalCenter.wallet.errors.checkPayStatusFailed'))
    } finally {
      checkingPayment.value = false
    }
  }

  onMounted(async () => {
    await loadDetail()
    if (isPending.value) {
      startPolling()
      if (shouldAutoOpenPaymentLink(payment.value)) link.openPayLink(true)
    }
  })
  onUnmounted(stopPolling)

  return {
    loading,
    checkingPayment,
    recharge,
    payment,
    isPending,
    link,
    customerFeeApplied,
    statusText,
    statusTone,
    money: (amount: string | undefined, currency?: string) => formatPrice(amount, currency ?? null),
    formatDate: formatDateTime,
    loadDetail,
    checkPayment,
  }
}
