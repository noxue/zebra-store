import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import type { AdminOrderRefund } from '@/api/types'
import { errorMessage } from '@/api/client'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'

/** `wallet`/`manual` → i18n key suffix; unknown codes are shown raw. */
export const refundTypeCode = (item: Pick<AdminOrderRefund, 'refund_type_label' | 'type'> | null | undefined) =>
  String(item?.refund_type_label || item?.type || '')
    .trim()
    .toLowerCase()

export function useOrderRefunds() {
  const filters = reactive({
    userId: '',
    userKeyword: '',
    orderNo: '',
    guestEmail: '',
    productKeyword: '',
    createdFrom: '',
    createdTo: '',
  })

  const list = useListPage<AdminOrderRefund>({
    fetchFn: (page, pageSize) =>
      adminAPI.getOrderRefunds(
        cleanParams({
          page,
          page_size: pageSize,
          user_id: filters.userId.trim(),
          user_keyword: filters.userKeyword.trim(),
          order_no: filters.orderNo.trim(),
          guest_email: filters.guestEmail.trim(),
          product_keyword: filters.productKeyword.trim(),
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const showDetail = ref(false)
  const selectedRefundId = ref<number | null>(null)
  const openDetail = (id: number) => {
    if (!id || id <= 0) return
    selectedRefundId.value = id
    showDetail.value = true
  }
  const onDetailToggle = (value: boolean) => {
    showDetail.value = value
    if (!value) selectedRefundId.value = null
  }

  return { filters, list, refreshing, refresh, showDetail, selectedRefundId, openDetail, onDetailToggle }
}

/** Refund detail dialog state, including the payment-fee-refunded toggle for manual refunds. */
export function useOrderRefundDetail(onUpdated: () => void) {
  const { t } = useI18n()
  const loading = ref(false)
  const error = ref('')
  const refund = ref<AdminOrderRefund | null>(null)
  const fee = reactive({ refunded: false, updating: false, error: '', success: '' })
  let seq = 0

  const reset = () => {
    seq++
    loading.value = false
    error.value = ''
    refund.value = null
    Object.assign(fee, { refunded: false, updating: false, error: '', success: '' })
  }

  const load = async (id: number | null | undefined) => {
    reset()
    if (!id || id <= 0) {
      error.value = t('admin.orderRefunds.detailFetchFailed')
      return
    }
    const current = seq
    loading.value = true
    try {
      const res = await adminAPI.getOrderRefund(id)
      if (current !== seq) return
      refund.value = res.data
      fee.refunded = !!res.data?.payment_fee_refunded
    } catch (err) {
      if (current === seq) error.value = errorMessage(err, t('admin.orderRefunds.detailFetchFailed'))
    } finally {
      if (current === seq) loading.value = false
    }
  }

  const updatePaymentFee = async () => {
    const r = refund.value
    if (!r || r.type !== 'manual') return
    fee.updating = true
    fee.error = ''
    fee.success = ''
    try {
      const res = await adminAPI.updateOrderRefundPaymentFee(r.id, { payment_fee_refunded: fee.refunded })
      refund.value = res.data
      fee.refunded = !!res.data?.payment_fee_refunded
      fee.success = t('admin.orderRefunds.paymentFeeUpdateSuccess')
      onUpdated()
    } catch (err) {
      fee.error = errorMessage(err, t('admin.orderRefunds.paymentFeeUpdateFailed'))
    } finally {
      fee.updating = false
    }
  }

  return { loading, error, refund, fee, reset, load, updatePaymentFee }
}
