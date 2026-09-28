import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import type { AdminOrder, AdminProcurementOrder } from '@/api/types'
import { errorMessage } from '@/api/client'
import { downloadBlob } from '@/utils/download'
import { canManualRefund, canRefundToWallet, checkRefundAmount, refundableAmountValue } from './orderUtils'

export type RefundTab = 'wallet' | 'manual'

/** State + actions of the order detail dialog (fetch, refunds, fulfillment download). */
export function useOrderDetail(opts: { maxRefundDays: () => number; onChanged: () => void }) {
  const { t } = useI18n()

  const loading = ref(false)
  const error = ref('')
  const order = ref<AdminOrder | null>(null)
  const procurement = ref<AdminProcurementOrder | null>(null)
  let seq = 0

  const refundTab = ref<RefundTab>('wallet')
  const wallet = reactive({ amount: '', remark: '', submitting: false, error: '', success: '' })
  const manual = reactive({ amount: '', reason: '', paymentFeeRefunded: true, submitting: false, error: '', success: '' })
  const downloading = ref(false)

  const defaultRefundTab = (o: AdminOrder | null): RefundTab => (o?.user_id ? 'wallet' : 'manual')

  const resetForms = () => {
    Object.assign(wallet, { amount: '', remark: '', error: '', success: '' })
    Object.assign(manual, { amount: '', reason: '', paymentFeeRefunded: true, error: '', success: '' })
  }

  const fetchDetail = async (orderId: number, keepTab = false) => {
    const current = ++seq
    loading.value = true
    error.value = ''
    order.value = null
    procurement.value = null
    try {
      const res = await adminAPI.getOrder(orderId)
      if (current !== seq) return
      order.value = res.data
      if (!keepTab || !res.data?.user_id) refundTab.value = defaultRefundTab(res.data)
      try {
        const proc = await adminAPI.getProcurementOrders({ order_no: res.data?.order_no, page_size: 1 })
        if (current === seq && Array.isArray(proc.data) && proc.data.length > 0) procurement.value = proc.data[0] ?? null
      } catch {
        // procurement order may not exist
      }
    } catch (err) {
      if (current === seq) error.value = errorMessage(err, t('admin.orders.detailFetchFailed'))
    } finally {
      if (current === seq) loading.value = false
    }
  }

  const open = (seed: AdminOrder | null) => {
    resetForms()
    refundTab.value = defaultRefundTab(seed)
    if (seed?.id) void fetchDetail(seed.id)
  }

  const close = () => {
    seq++
    order.value = null
    procurement.value = null
    error.value = ''
    loading.value = false
    refundTab.value = 'wallet'
    resetForms()
  }

  const amountError = (raw: string) => {
    const check = checkRefundAmount(raw, refundableAmountValue(order.value))
    if (check === 'invalid') return t('admin.orders.refundInvalidAmount')
    if (check === 'exceeded') return t('admin.orders.refundExceeded')
    return ''
  }

  const submitWallet = async () => {
    const o = order.value
    if (!o) return
    wallet.error = ''
    wallet.success = ''
    if (!canRefundToWallet(o, opts.maxRefundDays())) {
      wallet.error = t('admin.orders.refundNoRemaining')
      return
    }
    const invalid = amountError(wallet.amount)
    if (invalid) {
      wallet.error = invalid
      return
    }
    wallet.submitting = true
    try {
      await adminAPI.refundOrderToWallet(o.id, { amount: wallet.amount.trim(), remark: wallet.remark.trim() || undefined })
      await fetchDetail(o.id, true)
      wallet.success = t('admin.orders.refundSuccess')
      wallet.amount = ''
      wallet.remark = ''
      opts.onChanged()
    } catch (err) {
      wallet.error = errorMessage(err, t('admin.orders.refundFailed'))
    } finally {
      wallet.submitting = false
    }
  }

  const submitManual = async () => {
    const o = order.value
    if (!o) return
    manual.error = ''
    manual.success = ''
    if (!canManualRefund(o, opts.maxRefundDays())) {
      manual.error = t('admin.orders.refundNoRemaining')
      return
    }
    const invalid = amountError(manual.amount)
    if (invalid) {
      manual.error = invalid
      return
    }
    manual.submitting = true
    try {
      await adminAPI.manualRefundOrder(o.id, {
        amount: manual.amount.trim(),
        remark: manual.reason.trim() || undefined,
        payment_fee_refunded: manual.paymentFeeRefunded,
      })
      await fetchDetail(o.id, true)
      manual.success = t('admin.orders.manualRefundSuccess')
      manual.amount = ''
      manual.reason = ''
      manual.paymentFeeRefunded = true
      opts.onChanged()
    } catch (err) {
      manual.error = errorMessage(err, t('admin.orders.refundFailed'))
    } finally {
      manual.submitting = false
    }
  }

  const downloadFulfillment = async (orderId: number, orderNo: string) => {
    if (downloading.value) return
    downloading.value = true
    try {
      const res = await adminAPI.downloadFulfillment(orderId)
      const blob = new Blob([res.data], { type: 'text/plain; charset=utf-8' })
      downloadBlob(blob, `fulfillment-${orderNo}.txt`)
    } catch {
      /* already notified */
    } finally {
      downloading.value = false
    }
  }

  return {
    loading,
    error,
    order,
    procurement,
    refundTab,
    wallet,
    manual,
    downloading,
    open,
    close,
    fetchDetail,
    submitWallet,
    submitManual,
    downloadFulfillment,
  }
}
