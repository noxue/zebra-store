import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminReconciliationItem, AdminReconciliationJob, AdminSiteConnection } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { isRecord } from './integrationUtils'

export const RECON_ITEMS_PAGE_SIZE = 20
export const RECON_STATUSES = ['pending', 'running', 'completed', 'failed'] as const
export const RECON_TYPES = ['status', 'amount', 'full'] as const

/** Page logic for 对账中心 (jobs list, new job, job detail + resolve). */
export function useReconciliation() {
  const t = i18n.global.t
  const filters = reactive({ status: '__all__', type: '__all__', connection_id: '__all__' as string | number })
  const connections = ref<AdminSiteConnection[]>([])

  const list = useListPage<AdminReconciliationJob>({
    fetchFn: (page, pageSize) =>
      adminAPI.getReconciliationJobs(cleanParams({ page, page_size: pageSize, status: filters.status, type: filters.type, connection_id: filters.connection_id })),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(() => list.fetchData(1))

  const fetchConnections = async () => {
    try {
      connections.value = (await adminAPI.getSiteConnections({ page: 1, page_size: 200 })).data ?? []
    } catch {
      connections.value = []
    }
  }

  // --- new job ---
  const showNewJob = ref(false)
  const submitting = ref(false)
  const newJob = reactive({ connection_id: '' as number | '', type: 'full', time_range_start: '', time_range_end: '' })
  const canSubmitNewJob = () => !!newJob.connection_id && !!newJob.time_range_start && !!newJob.time_range_end
  const openNewJob = () => {
    if (connections.value.length === 0) void fetchConnections()
    showNewJob.value = true
  }
  const submitNewJob = async () => {
    if (!canSubmitNewJob()) return
    submitting.value = true
    try {
      await adminAPI.runReconciliation({
        connection_id: Number(newJob.connection_id),
        type: newJob.type,
        time_range_start: toRFC3339(newJob.time_range_start) ?? '',
        time_range_end: toRFC3339(newJob.time_range_end) ?? '',
      })
      notifySuccess(t('reconciliation.form.submitSuccess'))
      showNewJob.value = false
      Object.assign(newJob, { connection_id: '', time_range_start: '', time_range_end: '' })
      void list.fetchData(1)
    } catch {
      /* already notified */
    } finally {
      submitting.value = false
    }
  }

  // --- detail ---
  const showDetail = ref(false)
  const detailJob = ref<AdminReconciliationJob | null>(null)
  const detailItems = ref<AdminReconciliationItem[]>([])
  const itemsTotal = ref(0)
  const itemsPage = ref(1)

  const loadItems = async (jobId: number, page: number) => {
    const res = await adminAPI.getReconciliationJob(jobId, { items_page: page, items_page_size: RECON_ITEMS_PAGE_SIZE })
    const data = isRecord(res.data) ? res.data : {}
    detailItems.value = Array.isArray(data.items) ? (data.items as AdminReconciliationItem[]) : []
    itemsTotal.value = Number(data.items_total ?? 0) || 0
    itemsPage.value = page
    return data
  }

  const openDetail = async (job: AdminReconciliationJob) => {
    try {
      const data = await loadItems(job.id, 1)
      detailJob.value = isRecord(data.job) ? (data.job as AdminReconciliationJob) : job
    } catch {
      detailJob.value = job
      detailItems.value = []
      itemsTotal.value = 0
    }
    showDetail.value = true
  }

  const changeItemsPage = async (page: number) => {
    if (!detailJob.value) return
    try {
      await loadItems(detailJob.value.id, page)
    } catch {
      /* keep current items */
    }
  }

  // --- resolve ---
  const showResolve = ref(false)
  const resolveItemId = ref<number | null>(null)
  const resolveRemark = ref('')
  const resolving = ref(false)
  const openResolve = (item: AdminReconciliationItem) => {
    resolveItemId.value = item.id
    resolveRemark.value = ''
    showResolve.value = true
  }
  const submitResolve = async () => {
    if (!resolveItemId.value) return
    resolving.value = true
    try {
      await adminAPI.resolveReconciliationItem(resolveItemId.value, { remark: resolveRemark.value })
      notifySuccess(t('reconciliation.items.resolveSuccess'))
      showResolve.value = false
      await changeItemsPage(itemsPage.value)
    } catch {
      /* already notified */
    } finally {
      resolving.value = false
    }
  }

  const init = () => {
    void fetchConnections()
    void list.fetchData(1)
  }

  return {
    filters,
    connections,
    list,
    refreshing,
    refresh,
    showNewJob,
    submitting,
    newJob,
    canSubmitNewJob,
    openNewJob,
    submitNewJob,
    showDetail,
    detailJob,
    detailItems,
    itemsTotal,
    itemsPage,
    openDetail,
    changeItemsPage,
    showResolve,
    resolveRemark,
    resolving,
    openResolve,
    submitResolve,
    init,
  }
}
