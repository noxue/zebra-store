import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { paymentAPI, userOrderAPI } from '@/api/order'
import type { Order } from '@/api/types'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { toast } from '@/composables/useToast'
import { downloadBlob } from '@/utils/format'
import { hasDeliveredOrder, orderExportText } from '@/utils/exportOrder'
import { useOrderDisplayHelpers } from './useOrderDisplayHelpers'

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
  const helpers = useOrderDisplayHelpers(order)
  const orderNo = () => String(route.params.order_no || '').trim()

  const loadOrder = async () => {
    loading.value = true
    try {
      order.value = (await userOrderAPI.detail(orderNo())).data
      hasManualPayment.value = false
      if (order.value?.status === 'pending_payment' || order.value?.status === 'canceled') {
        try {
          const latest = await paymentAPI.latest({ order_no: order.value.order_no })
          hasManualPayment.value = latest.data?.provider_type === 'manual_qr'
        } catch { /* No active payment attempt. */ }
      }
    } catch {
      order.value = null
      hasManualPayment.value = false
    } finally {
      loading.value = false
    }
  }

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

  return { loading, order, hasManualPayment, loadOrder, cancelOrder, fulfillmentDownloading, handleDownloadFulfillment, exportDownloading, exportOrder, helpers }
}
