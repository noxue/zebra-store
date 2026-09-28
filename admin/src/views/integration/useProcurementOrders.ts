import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminSiteConnection, Money } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { normalizeUpstreamRefundRecords, parseProcurementStats, type ProcurementLocalOrder, type ProcurementStats } from './integrationUtils'

/** Procurement order as rendered by the page (list row / detail). */
export interface ProcurementRow {
  id: number
  connection_id: number
  local_order_id: number
  local_order_no: string
  parent_order_no?: string
  upstream_order_no?: string
  status: string
  upstream_amount: Money
  upstream_currency?: string
  local_sell_amount: Money
  currency?: string
  error_message?: string
  retry_count: number
  next_retry_at?: string
  trace_id: string
  upstream_payload?: string
  upstream_payload_line_count?: number
  upstream_refund_records?: unknown
  upstream_refunded_amount?: Money
  created_at: string
  updated_at: string
  connection?: { id?: number; name?: string; exchange_rate?: Money }
  local_order?: ProcurementLocalOrder
}

/** Page logic for 采购单管理. */
export function useProcurementOrders() {
  const t = i18n.global.t
  const connections = ref<AdminSiteConnection[]>([])
  const filters = reactive({
    status: '__all__' as string,
    connection_id: '__all__' as string | number,
    order_no: '',
    upstream_order_no: '',
    created_from: '',
    created_to: '',
  })
  const stats = ref<ProcurementStats>({ total: 0, pending: 0, failed: 0, rejected: 0, fulfilled: 0, other: 0 })
  const showDetail = ref(false)
  const detail = ref<ProcurementRow | null>(null)
  const detailLoading = ref(false)
  const retryingId = ref<number | null>(null)
  const cancelingId = ref<number | null>(null)
  const downloading = ref(false)

  /** Filters shared by list + stats (stats ignore the status filter). */
  const baseParams = () => ({
    connection_id: filters.connection_id,
    order_no: filters.order_no.trim(),
    upstream_order_no: filters.upstream_order_no.trim(),
    created_from: toRFC3339(filters.created_from),
    created_to: toRFC3339(filters.created_to),
  })

  const list = useListPage<ProcurementRow>({
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getProcurementOrders(cleanParams({ page, page_size: pageSize, status: filters.status, ...baseParams() }))
      return { items: (res.data ?? []) as unknown as ProcurementRow[], pagination: res.pagination }
    },
  })
  const { refreshing, refreshList } = useListRefresh()

  const fetchStats = async () => {
    try {
      stats.value = parseProcurementStats((await adminAPI.getProcurementOrderStats(cleanParams(baseParams()))).data)
    } catch {
      /* keep previous values */
    }
  }
  const fetchConnections = async () => {
    try {
      connections.value = (await adminAPI.getSiteConnections({ page: 1, page_size: 100 })).data ?? []
    } catch {
      /* ignore */
    }
  }

  const search = () => {
    void list.fetchData(1)
    void fetchStats()
  }
  let timer: ReturnType<typeof setTimeout> | undefined
  const debouncedSearch = () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(search, 300)
  }
  const selectStatus = (status: string) => {
    filters.status = filters.status === status ? '__all__' : status
    void list.fetchData(1)
  }
  const refresh = () => refreshList(list.refresh)

  const openDetail = async (order: ProcurementRow) => {
    detail.value = order
    showDetail.value = true
    detailLoading.value = true
    try {
      const res = await adminAPI.getProcurementOrder(order.id)
      if (res.data && detail.value?.id === order.id) detail.value = res.data as unknown as ProcurementRow
    } catch {
      /* keep list row */
    } finally {
      detailLoading.value = false
    }
  }

  const afterAction = (order: ProcurementRow) => {
    void list.refresh()
    void fetchStats()
    if (showDetail.value && detail.value?.id === order.id) void openDetail(order)
  }

  const retry = async (order: ProcurementRow) => {
    const ok = await confirmAction({ description: t('procurement.actions.retryConfirm', { id: order.id }), confirmText: t('procurement.actions.retry') })
    if (!ok) return
    retryingId.value = order.id
    try {
      await adminAPI.retryProcurementOrder(order.id)
      notifySuccess(t('procurement.actions.retrySuccess'))
      afterAction(order)
    } catch {
      /* already notified */
    } finally {
      retryingId.value = null
    }
  }

  const cancel = async (order: ProcurementRow) => {
    const ok = await confirmAction({
      description: t('procurement.actions.cancelConfirm', { id: order.id }),
      confirmText: t('procurement.actions.cancelOrder'),
      variant: 'destructive',
    })
    if (!ok) return
    cancelingId.value = order.id
    try {
      await adminAPI.cancelProcurementOrder(order.id)
      notifySuccess(t('procurement.actions.cancelSuccess'))
      afterAction(order)
    } catch {
      /* already notified */
    } finally {
      cancelingId.value = null
    }
  }

  const downloadPayload = async (id: number) => {
    if (downloading.value) return
    downloading.value = true
    try {
      const res = await adminAPI.downloadProcurementUpstreamPayload(id)
      const blob = new Blob([res.data], { type: 'text/plain; charset=utf-8' })
      downloadBlob(blob, filenameFromDisposition(res.headers['content-disposition'], `upstream-payload-${id}.txt`))
    } catch {
      /* already notified */
    } finally {
      downloading.value = false
    }
  }

  const refundRecords = computed(() => normalizeUpstreamRefundRecords(detail.value?.upstream_refund_records))

  const init = () => {
    void fetchConnections()
    void list.fetchData(1)
    void fetchStats()
  }

  return {
    list,
    filters,
    connections,
    stats,
    refreshing,
    refresh,
    search,
    debouncedSearch,
    selectStatus,
    showDetail,
    detail,
    detailLoading,
    retryingId,
    cancelingId,
    downloading,
    refundRecords,
    openDetail,
    retry,
    cancel,
    downloadPayload,
    init,
  }
}
