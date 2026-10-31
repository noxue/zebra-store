import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { paymentAPI, userOrderAPI } from '@/api/order'
import type { Order } from '@/api/types'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { toast } from '@/composables/useToast'
import { downloadBlob } from '@/utils/format'
import { hasDeliveredOrder, orderExportText } from '@/utils/exportOrder'
import { useOrderDisplayHelpers } from './useOrderDisplayHelpers'
import { useOrderAutoRefresh } from './useOrderAutoRefresh'

/** Logged-in order detail (load / cancel / download fulfillment). */
export function useOrderDetail() {
  const route = useRoute()
  const router = useRouter()
  const { t, locale } = useI18n()
  const { confirm } = useConfirmDialog()
  const loading = ref(true)
  const order = ref<Order | null>(null)
  const hasManualPayment = ref(false)
  const fulfillmentDownloading = ref(false)
  const exportDownloading = ref(false)
  const refundSubmitting = ref(false)
  const helpers = useOrderDisplayHelpers(order)
  const orderNo = () => String(route.params.order_no || '').trim()

  let requestVersion = 0
  let disposed = false
  const loadOrder = async (silent = false) => {
    const no = orderNo()
    const version = ++requestVersion
    const isCurrent = () => !disposed && version === requestVersion && no === orderNo()
    if (!silent) loading.value = true
    try {
      const response = await userOrderAPI.detail(no, { silentBusinessError: silent })
      if (!isCurrent()) return
      order.value = response.data
      hasManualPayment.value = false
      if (order.value?.status === 'pending_payment' || order.value?.status === 'canceled') {
        try {
          const latest = await paymentAPI.latest({ order_no: order.value.order_no })
          if (!isCurrent()) return
          hasManualPayment.value = latest.data?.provider_type === 'manual_qr'
        } catch { /* No active payment attempt. */ }
      }
    } catch {
      if (isCurrent() && !silent) {
        order.value = null
        hasManualPayment.value = false
      }
    } finally {
      if (isCurrent()) loading.value = false
    }
  }

  useOrderAutoRefresh(order, loading, () => loadOrder(true))
  watch(() => route.params.order_no, () => {
    order.value = null
    hasManualPayment.value = false
    void loadOrder()
  })
  onBeforeUnmount(() => { disposed = true })

  const cancelOrder = async () => {
    if (!order.value) return
    const ok = await confirm({
      title: t('orderDetail.cancel'),
      message: t('orderDetail.cancelConfirm'),
      confirmText: t('common.confirm'),
      cancelText: t('common.cancel'),
      variant: 'danger',
    })
    if (!ok) return
    try {
      await userOrderAPI.cancel(order.value.order_no)
      await loadOrder()
    } catch {
      toast.error(t('orderDetail.cancelFailed'))
    }
  }

  const requestRefund = async () => {
    if (!order.value || refundSubmitting.value) return
    const accepted = await confirm({
      title: t('orderDetail.requestRefund'),
      message: t('orderDetail.refundConfirm'),
      confirmText: t('common.confirm'),
      cancelText: t('common.cancel'),
    })
    if (!accepted) return
    refundSubmitting.value = true
    try {
      const response = await userOrderAPI.requestRefund(order.value.order_no)
      toast.success(t(response.data.status === 'succeeded' ? 'orderDetail.refundSucceeded' : 'orderDetail.refundPending'))
      await loadOrder()
    } catch {
      toast.error(t('orderDetail.refundFailed'))
    } finally {
      refundSubmitting.value = false
    }
  }

  const handleDownloadFulfillment = async (no: string) => {
    if (fulfillmentDownloading.value) return
    fulfillmentDownloading.value = true
    try {
      const blob = await userOrderAPI.downloadFulfillment(no)
      downloadBlob(new Blob([blob], { type: 'text/plain; charset=utf-8' }), `fulfillment-${no}.txt`)
    } catch {
      // ignore (same as original)
    } finally {
      fulfillmentDownloading.value = false
    }
  }

  const exportOrder = async () => {
    if (!order.value || exportDownloading.value || !hasDeliveredOrder(order.value)) return
    exportDownloading.value = true
    try {
      const content = await orderExportText(order.value, String(locale.value), {
        orderNo: t('orderDetail.exportOrderNo'),
        keys: t('orderDetail.exportKeys'),
        instructions: t('orderDetail.exportInstructions'),
      }, userOrderAPI.downloadFulfillment)
      downloadBlob(new Blob([content], { type: 'text/plain; charset=utf-8' }), `order-${order.value.order_no}.txt`)
    } catch {
      toast.error(t('orderDetail.exportFailed'))
    } finally {
      exportDownloading.value = false
    }
  }

  onMounted(() => {
    if (!orderNo()) {
      void router.push('/me/orders')
      return
    }
    void loadOrder()
  })

  return { loading, order, hasManualPayment, loadOrder, cancelOrder, refundSubmitting, requestRefund, fulfillmentDownloading, handleDownloadFulfillment, exportDownloading, exportOrder, helpers }
}
