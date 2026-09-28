import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { userOrderAPI } from '@/api/order'
import { walletAPI } from '@/api/wallet'
import type { Order, WalletRechargeOrderData } from '@/api/types'
import type { SelectOption } from '@/components/ui'
import { debounce } from '@/utils/debounce'
import { isPositiveAmount } from '@/utils/money'
import { RECHARGE_STATUSES, rechargeStatusTone } from '@/utils/personal'
import { orderStatusLabel, orderStatusList, orderStatusVariant } from '@/utils/status'
import { usePagedList } from './usePagedList'

export type OrdersTab = 'product' | 'recharge'

/** Orders panel: product orders + wallet recharge orders with filters, stats and paging. */
export function useOrdersPanel() {
  const { t, te } = useI18n()
  const activeTab = ref<OrdersTab>('product')

  // ---------- product orders
  const orderFilters = reactive({ orderNo: '', status: '' })
  const orderStats = ref<Record<string, number>>({})
  const orders = usePagedList<Order, { status: string; order_no: string }>((p) => userOrderAPI.list(p))

  const loadOrderStats = async () => {
    try {
      const res = await userOrderAPI.stats({ order_no: orderFilters.orderNo.trim() || undefined })
      orderStats.value = res.data?.by_status || {}
    } catch {
      orderStats.value = {}
    }
  }
  const loadOrders = async (page = 1) => {
    await orders.load(page, { status: orderFilters.status || undefined, order_no: orderFilters.orderNo.trim() || undefined })
    void loadOrderStats()
  }

  const orderStatusOptions = computed<SelectOption[]>(() => [
    { value: '', label: t('orders.filters.statusAll') },
    ...orderStatusList.map((s) => ({ value: s, label: t(`order.status.${s}`) })),
  ])
  const pendingPaymentCount = computed(() => orderStats.value.pending_payment || 0)
  const finishedCount = computed(
    () =>
      (orderStats.value.delivered || 0) +
      (orderStats.value.completed || 0) +
      (orderStats.value.partially_refunded || 0) +
      (orderStats.value.refunded || 0),
  )
  const hasOrderFilters = computed(() => Boolean(orderFilters.orderNo || orderFilters.status))

  // ---------- recharge orders
  const rechargeFilters = reactive({ rechargeNo: '', status: '' })
  const rechargeStats = ref<Record<string, number>>({})
  const recharges = usePagedList<WalletRechargeOrderData, { status: string; recharge_no: string }>((p) => walletAPI.rechargeOrders(p))

  const loadRechargeStats = async () => {
    try {
      const res = await walletAPI.rechargeStats({ recharge_no: rechargeFilters.rechargeNo.trim() || undefined })
      rechargeStats.value = res.data?.by_status || {}
    } catch {
      rechargeStats.value = {}
    }
  }
  const loadRecharges = async (page = 1) => {
    await recharges.load(page, { status: rechargeFilters.status || undefined, recharge_no: rechargeFilters.rechargeNo.trim() || undefined })
    void loadRechargeStats()
  }
  const rechargeStatusOptions = computed<SelectOption[]>(() => [
    { value: '', label: t('orders.filters.statusAll') },
    ...RECHARGE_STATUSES.map((s) => ({ value: s, label: t(`personalCenter.wallet.rechargeStatus.${s}`) })),
  ])
  const rechargePendingCount = computed(() => rechargeStats.value.pending || 0)

  const rechargeStatusText = (status?: string) => {
    const s = String(status || '').toLowerCase()
    const key = `personalCenter.wallet.rechargeStatus.${s}`
    return s && te(key) ? t(key) : s || '-'
  }

  // ---------- shared
  const debouncedOrders = debounce((page: number) => void loadOrders(page), 300)
  const debouncedRecharges = debounce((page: number) => void loadRecharges(page), 300)

  const switchTab = (tab: string) => {
    const next: OrdersTab = tab === 'recharge' ? 'recharge' : 'product'
    if (activeTab.value === next) return
    activeTab.value = next
    if (next === 'product' && !orders.loaded.value) void loadOrders(1)
    if (next === 'recharge' && !recharges.loaded.value) void loadRecharges(1)
  }

  const onOrderNoInput = (v: string) => {
    orderFilters.orderNo = v
    debouncedOrders(1)
  }
  const onOrderStatus = (v: string | number) => {
    orderFilters.status = String(v)
    void loadOrders(1)
  }
  const resetOrderFilters = () => {
    orderFilters.orderNo = ''
    orderFilters.status = ''
    void loadOrders(1)
  }
  const onRechargeNoInput = (v: string) => {
    rechargeFilters.rechargeNo = v
    debouncedRecharges(1)
  }
  const onRechargeStatus = (v: string | number) => {
    rechargeFilters.status = String(v)
    void loadRecharges(1)
  }
  const resetRechargeFilters = () => {
    rechargeFilters.rechargeNo = ''
    rechargeFilters.status = ''
    void loadRecharges(1)
  }

  const hasDiscount = (order: Order) => isPositiveAmount(order.discount_amount) || isPositiveAmount(order.promotion_discount_amount)

  onMounted(() => {
    void loadOrders(1)
  })
  onBeforeUnmount(() => {
    debouncedOrders.cancel()
    debouncedRecharges.cancel()
  })

  return {
    activeTab,
    switchTab,
    orders,
    orderFilters,
    orderStatusOptions,
    pendingPaymentCount,
    finishedCount,
    hasOrderFilters,
    loadOrders,
    onOrderNoInput,
    onOrderStatus,
    resetOrderFilters,
    recharges,
    rechargeFilters,
    rechargeStatusOptions,
    rechargePendingCount,
    loadRecharges,
    onRechargeNoInput,
    onRechargeStatus,
    resetRechargeFilters,
    rechargeStatusText,
    rechargeStatusTone,
    statusLabel: (s?: string) => orderStatusLabel(t, s),
    statusTone: orderStatusVariant,
    hasDiscount,
  }
}
