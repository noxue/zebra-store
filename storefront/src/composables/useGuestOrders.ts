import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage, type Pagination } from '@/api/client'
import { guestOrderAPI, type GuestCredentials } from '@/api/order'
import type { Order } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { debounce } from '@/utils/debounce'
import { formatDateTime } from '@/utils/format'
import { clearGuestOrderAuth, loadGuestOrderAuth, saveGuestOrderAuth } from '@/utils/guestOrderAuth'
import { hasPositive } from '@/utils/orderPayment'

const PAGE_SIZE = 20
const emptyPagination = (): Pagination => ({ page: 1, page_size: PAGE_SIZE, total: 0, total_page: 1 })

/** Guest order lookup (email + order password [+ order no]). */
export function useGuestOrders() {
  const { t } = useI18n()
  const { formatPrice } = useLocalized()
  const savedAuth = ref<GuestCredentials>({ email: '', order_password: '' })
  const email = ref('')
  const orderPassword = ref('')
  const orderNo = ref('')
  const loading = ref(false)
  const searched = ref(false)
  const error = ref('')
  const orders = ref<Order[]>([])
  const pagination = ref<Pagination>(emptyPagination())

  const hasSavedAuth = computed(() => Boolean(savedAuth.value.email || savedAuth.value.order_password))

  const loadOrders = async (page: number) => {
    loading.value = true
    try {
      const res = await guestOrderAPI.list(
        { email: email.value, order_password: orderPassword.value },
        { order_no: orderNo.value || undefined, page, page_size: pagination.value.page_size },
      )
      orders.value = res.data || []
      pagination.value = res.pagination || pagination.value
      if (orderNo.value && orders.value.length === 0) error.value = t('guestOrders.errors.notFound')
    } catch (err) {
      orders.value = []
      error.value = errorMessage(err, t('guestOrders.errors.searchFailed'))
    } finally {
      loading.value = false
      searched.value = true
    }
  }
  const debouncedLoad = debounce((page: number) => void loadOrders(page), 300)

  const handleSearch = () => {
    error.value = ''
    if (!email.value || !orderPassword.value) {
      error.value = t('guestOrders.errors.missing')
      return
    }
    const payload = { email: email.value, order_password: orderPassword.value }
    saveGuestOrderAuth(payload)
    savedAuth.value = payload
    debouncedLoad(1)
  }

  const clearSaved = () => {
    clearGuestOrderAuth()
    savedAuth.value = { email: '', order_password: '' }
    email.value = ''
    orderPassword.value = ''
    orderNo.value = ''
    orders.value = []
    pagination.value = emptyPagination()
    error.value = ''
    searched.value = false
  }

  const changePage = (page: number) => {
    if (page < 1 || page > pagination.value.total_page) return
    debouncedLoad(page)
  }

  const emptyMessage = computed(() => (orderNo.value ? t('guestOrders.emptyOrderNo') : t('guestOrders.empty')))

  onMounted(() => {
    savedAuth.value = loadGuestOrderAuth()
    email.value = savedAuth.value.email
    orderPassword.value = savedAuth.value.order_password
    if (hasSavedAuth.value) debouncedLoad(1)
  })
  onUnmounted(() => debouncedLoad.cancel())

  return {
    savedAuth,
    email,
    orderPassword,
    orderNo,
    loading,
    searched,
    error,
    orders,
    pagination,
    hasSavedAuth,
    clearSaved,
    handleSearch,
    emptyMessage,
    changePage,
    money: (amount: string | undefined, currency?: string) => formatPrice(amount, currency ?? null),
    discountMoney: (amount: string | undefined, currency?: string) => `-${formatPrice(amount, currency ?? null)}`,
    hasPositive,
    hasDiscount: (o: Order) => hasPositive(o.discount_amount) || hasPositive(o.promotion_discount_amount),
    formatDate: formatDateTime,
  }
}
