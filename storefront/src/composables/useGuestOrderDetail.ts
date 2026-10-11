import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { guestOrderAPI, type GuestCredentials } from '@/api/order'
import type { Order } from '@/api/types'
import { downloadBlob } from '@/utils/format'
import { clearGuestOrderAuth, loadGuestOrderAuth, saveGuestOrderAuth } from '@/utils/guestOrderAuth'
import { resolveGuestOrderDetailViewState } from '@/utils/guestOrderDetailState'
import { useOrderDisplayHelpers } from './useOrderDisplayHelpers'

/** Guest order detail: credentials form, then the shared order body. */
export function useGuestOrderDetail() {
  const route = useRoute()
  const router = useRouter()
  const { t } = useI18n()
  const loading = ref(true)
  const order = ref<Order | null>(null)
  const hasManualPayment = ref(false)
  const authError = ref('')
  const auth = ref<GuestCredentials>({ email: '', order_password: '' })
  const fulfillmentDownloading = ref(false)
  const helpers = useOrderDisplayHelpers(order)
  const orderNo = () => String(route.params.order_no || '').trim()

  const hasAuth = computed(() => Boolean(auth.value.email && auth.value.order_password))
  const showAuthForm = computed(() => !hasAuth.value || authError.value !== '')
  const viewState = computed(() => resolveGuestOrderDetailViewState({ loading: loading.value, order: order.value, showAuthForm: showAuthForm.value }))

  const loadOrder = async () => {
    loading.value = true
    try {
      if (!hasAuth.value) {
        order.value = null
        authError.value = t('guestOrderDetail.authRequired')
        return
      }
      order.value = (await guestOrderAPI.detail(orderNo(), { ...auth.value })).data
      hasManualPayment.value = false
      if (order.value?.status === 'pending_payment' || order.value?.status === 'canceled') {
        try {
          const latest = await guestOrderAPI.latestPayment({ ...auth.value }, { order_no: order.value.order_no })
          hasManualPayment.value = latest.data?.provider_type === 'manual_qr'
        } catch { /* No active payment attempt. */ }
      }
      authError.value = ''
    } catch {
      order.value = null
      hasManualPayment.value = false
      authError.value = t('guestOrderDetail.authInvalid')
    } finally {
      loading.value = false
    }
  }

  const handleAuthSubmit = async () => {
    authError.value = ''
    if (!hasAuth.value) {
      authError.value = t('guestOrderDetail.authRequired')
      return
    }
    saveGuestOrderAuth({ ...auth.value })
    await loadOrder()
  }

  const clearAuth = () => {
    clearGuestOrderAuth()
    auth.value = { email: '', order_password: '' }
    order.value = null
    hasManualPayment.value = false
    authError.value = t('guestOrderDetail.authRequired')
  }

  const handleDownloadFulfillment = async (no: string) => {
    if (fulfillmentDownloading.value) return
    fulfillmentDownloading.value = true
    try {
      const blob = await guestOrderAPI.downloadFulfillment(no, { ...auth.value })
      downloadBlob(new Blob([blob], { type: 'text/plain; charset=utf-8' }), `fulfillment-${no}.txt`)
    } catch {
      // ignore (same as original)
    } finally {
      fulfillmentDownloading.value = false
    }
  }

  onMounted(() => {
    if (!orderNo()) {
      void router.push('/guest/orders')
      return
    }
    auth.value = loadGuestOrderAuth()
    void loadOrder()
  })

  return { loading, order, hasManualPayment, auth, authError, viewState, handleAuthSubmit, clearAuth, fulfillmentDownloading, handleDownloadFulfillment, helpers }
}
