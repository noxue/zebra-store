import { computed, reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminOrder } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useAdminAuthStore } from '@/stores/auth'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { canUpdateStatus, normalizeMaxRefundDays, orderActionFlags, parseSortValue } from './orderUtils'

export function useOrders() {
  const auth = useAdminAuthStore()
  const allowed = computed(() => orderActionFlags((p) => auth.hasPermission(p)))
  const filters = reactive({
    userId: '',
    userKeyword: '',
    orderNo: '',
    guestEmail: '',
    productKeyword: '',
    createdFrom: '',
    createdTo: '',
    status: '__all__',
    sortBy: '__all__',
  })

  const statusEdits = reactive<Record<number, string>>({})
  const maxRefundDays = ref(30)

  const list = useListPage<AdminOrder>({
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getOrders(
        cleanParams({
          page,
          page_size: pageSize,
          status: filters.status,
          user_id: filters.userId.trim(),
          user_keyword: filters.userKeyword.trim(),
          order_no: filters.orderNo.trim(),
          guest_email: filters.guestEmail.trim(),
          product_keyword: filters.productKeyword.trim(),
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
          ...parseSortValue(filters.sortBy),
        }),
      )
      ;(res.data || []).forEach((o) => {
        statusEdits[o.id] = o.status
      })
      return res
    },
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const fetchRefundConfig = async () => {
    // Roles without settings access keep the default instead of getting a 403 toast.
    if (!allowed.value.readSettings) {
      maxRefundDays.value = 30
      return
    }
    try {
      const res = await adminAPI.getSettings<{ max_refund_days?: unknown }>({ key: 'order_config' })
      maxRefundDays.value = normalizeMaxRefundDays(res.data?.max_refund_days)
    } catch {
      maxRefundDays.value = 30
    }
  }

  const updateStatus = async (order: AdminOrder) => {
    if (!allowed.value.updateStatus || !canUpdateStatus(order)) return
    const status = statusEdits[order.id]
    if (!status || status === order.status) return
    try {
      await adminAPI.updateOrderStatus(order.id, { status })
      await list.refresh()
    } catch {
      /* already notified */
    }
  }

  const markCompleted = async (order: AdminOrder) => {
    if (!allowed.value.updateStatus || order.status !== 'delivered') return
    try {
      await adminAPI.updateOrderStatus(order.id, { status: 'completed' })
      await list.refresh()
    } catch {
      /* already notified */
    }
  }

  // ---- dialogs ----
  const showDetail = ref(false)
  const showFulfillment = ref(false)
  const selectedOrder = ref<AdminOrder | null>(null)
  const fulfillmentParentId = ref<number | null>(null)

  const openDetail = (order: AdminOrder) => {
    showFulfillment.value = false
    selectedOrder.value = order
    showDetail.value = true
  }

  const openDetailById = (orderId: number) => {
    if (!orderId || orderId <= 0) return
    openDetail({ id: orderId } as AdminOrder)
  }

  const openFulfillment = (order: AdminOrder, parentId?: number) => {
    showDetail.value = false
    fulfillmentParentId.value = parentId || null
    selectedOrder.value = order
    showFulfillment.value = true
  }

  const onDetailToggle = (value: boolean) => {
    showDetail.value = value
    if (!value && !showFulfillment.value) selectedOrder.value = null
  }

  const onFulfillmentToggle = (value: boolean) => {
    showFulfillment.value = value
    if (!value) {
      selectedOrder.value = null
      fulfillmentParentId.value = null
    }
  }

  const onFulfillmentSuccess = (parentId?: number | null) => {
    showFulfillment.value = false
    fulfillmentParentId.value = null
    if (parentId) openDetailById(parentId)
    else selectedOrder.value = null
    void list.refresh()
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    statusEdits,
    allowed,
    maxRefundDays,
    fetchRefundConfig,
    updateStatus,
    markCompleted,
    showDetail,
    showFulfillment,
    selectedOrder,
    fulfillmentParentId,
    openDetail,
    openDetailById,
    openFulfillment,
    onDetailToggle,
    onFulfillmentToggle,
    onFulfillmentSuccess,
  }
}
