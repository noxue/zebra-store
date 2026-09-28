import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCardSecret, AdminCardSecretBatch, AdminCardSecretQueryPayload, Pagination } from '@/api/types'
import { confirmAction } from '@/utils/confirm'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { useListRefresh } from '@/composables/useListRefresh'
import { useProductSkuPicker } from './useProductSkuPicker'
import { normalizeIds, type CardSecretStatus } from './cardSecretUtils'

const emptyPagination = (pageSize: number): Pagination => ({ page: 1, page_size: pageSize, total: 0, total_page: 1 })

interface FetchOptions {
  preserveRows?: boolean
}

/** Page logic for 卡密库存: product/SKU picker, stats, batch navigator, list, scoped batch actions. */
export function useCardSecrets() {
  const t = i18n.global.t
  const picker = useProductSkuPicker({ autoSelectSingleSku: true })
  const { refreshing, refreshList } = useListRefresh()

  const stats = ref<{ available: number; reserved: number; used: number; total: number } | null>(null)
  const statsLoading = ref(false)

  const batches = ref<AdminCardSecretBatch[]>([])
  const batchesLoading = ref(false)
  const batchPagination = ref<Pagination>(emptyPagination(10))

  const secrets = ref<AdminCardSecret[]>([])
  const secretsLoading = ref(false)
  const secretPagination = ref<Pagination>(emptyPagination(20))

  const filters = reactive({ status: '__all__' as string, secret: '', batchNo: '' })
  const currentBatch = ref<AdminCardSecretBatch | null>(null)
  const selectedIds = ref<number[]>([])
  const operationScope = ref<'selected' | 'filtered'>('selected')
  const batchStatusTarget = ref<CardSecretStatus>('available')
  const actionLoading = ref(false)
  const actionError = ref('')
  const actionSuccess = ref('')

  const showEditModal = ref(false)
  const editingSecret = ref<AdminCardSecret | null>(null)

  const currentBatchId = computed(() => {
    const raw = Number(currentBatch.value?.id || 0)
    return Number.isFinite(raw) && raw > 0 ? Math.floor(raw) : 0
  })
  const productHint = computed(() =>
    picker.productId.value ? t('admin.cardSecrets.productHintCurrent', { id: picker.productId.value }) : t('admin.cardSecrets.productHintEmpty'),
  )
  const batchFilterText = computed(() =>
    currentBatch.value
      ? t('admin.cardSecrets.batchFilterCurrent', { id: currentBatch.value.id, batchNo: currentBatch.value.batch_no || '-' })
      : t('admin.cardSecrets.batchFilterAll'),
  )

  const selectedCount = computed(() => normalizeIds(selectedIds.value).length)
  const filteredCount = computed(() => Number(secretPagination.value.total || 0))
  const scopeCount = computed(() => (operationScope.value === 'selected' ? selectedCount.value : filteredCount.value))
  const scopeLabel = computed(() =>
    operationScope.value === 'selected' ? t('admin.cardSecrets.batch.scopeSelected') : t('admin.cardSecrets.batch.scopeFiltered'),
  )
  const hasActionableScope = computed(() => scopeCount.value > 0)
  const scopeHint = computed(() =>
    operationScope.value === 'selected'
      ? t('admin.cardSecrets.batch.scopeHintSelected', { count: selectedCount.value })
      : t('admin.cardSecrets.batch.scopeHintFiltered', { count: filteredCount.value }),
  )

  const queryFilter = computed<AdminCardSecretQueryPayload>(() => {
    const payload: AdminCardSecretQueryPayload = {
      status: filters.status === '__all__' ? undefined : filters.status || undefined,
      secret: filters.secret.trim() || undefined,
      batch_no: filters.batchNo.trim() || undefined,
    }
    if (picker.productId.value) {
      payload.product_id = picker.productId.value
      payload.sku_id = picker.skuId.value || undefined
    }
    if (currentBatchId.value) payload.batch_id = currentBatchId.value
    return payload
  })

  // ---- selection ----
  const allPageSelected = computed(() => secrets.value.length > 0 && secrets.value.every((s) => selectedIds.value.includes(s.id)))
  const somePageSelected = computed(() => !allPageSelected.value && secrets.value.some((s) => selectedIds.value.includes(s.id)))
  const toggleSelectAll = () => {
    selectedIds.value = allPageSelected.value ? [] : secrets.value.map((s) => s.id).filter((id) => id > 0)
  }
  const toggleSelected = (row: AdminCardSecret) => {
    selectedIds.value = selectedIds.value.includes(row.id) ? selectedIds.value.filter((id) => id !== row.id) : [...selectedIds.value, row.id]
  }

  const clearMessages = () => {
    actionError.value = ''
    actionSuccess.value = ''
  }

  // ---- loaders ----
  const refreshStats = async () => {
    const productId = picker.productId.value
    if (!productId) {
      stats.value = null
      return
    }
    statsLoading.value = true
    try {
      stats.value = (await adminAPI.getCardSecretStats({ product_id: productId, sku_id: picker.skuId.value || undefined })).data
    } catch {
      stats.value = null
    } finally {
      statsLoading.value = false
    }
  }

  const fetchBatches = async (page = 1, options: FetchOptions = {}) => {
    const productId = picker.productId.value
    if (!productId) {
      batches.value = []
      batchPagination.value = emptyPagination(batchPagination.value.page_size)
      return
    }
    if (!options.preserveRows) batchesLoading.value = true
    try {
      const res = await adminAPI.getCardSecretBatches({
        product_id: productId,
        sku_id: picker.skuId.value || undefined,
        page,
        page_size: batchPagination.value.page_size,
      })
      batches.value = res.data || []
      if (res.pagination) batchPagination.value = res.pagination
    } catch {
      if (!options.preserveRows) batches.value = []
    } finally {
      if (!options.preserveRows) batchesLoading.value = false
    }
  }

  const fetchSecrets = async (page = 1, options: FetchOptions = {}): Promise<void> => {
    if (!options.preserveRows) secretsLoading.value = true
    try {
      const res = await adminAPI.getCardSecrets({ ...queryFilter.value, page, page_size: secretPagination.value.page_size })
      const next = res.pagination || secretPagination.value
      const totalPage = Number(next.total_page || 1)
      if (page > totalPage && totalPage > 0) {
        await fetchSecrets(totalPage, options)
        return
      }
      secrets.value = res.data || []
      secretPagination.value = { ...next, total_page: Math.max(totalPage, 1) }
      selectedIds.value = []
    } catch {
      if (!options.preserveRows) {
        secrets.value = []
        selectedIds.value = []
      }
    } finally {
      if (!options.preserveRows) secretsLoading.value = false
    }
  }

  const refreshAll = async (options: FetchOptions = {}) => {
    clearMessages()
    if (picker.productId.value) {
      await picker.loadProductInfo()
      await Promise.all([refreshStats(), fetchBatches(1, options), fetchSecrets(1, options)])
      return
    }
    picker.productInfo.value = null
    stats.value = null
    currentBatch.value = null
    batches.value = []
    batchPagination.value = emptyPagination(batchPagination.value.page_size)
    picker.skuValue.value = '__all__'
    await fetchSecrets(1, options)
  }

  const refreshAfterMutations = async () => {
    await fetchSecrets(secretPagination.value.page)
    if (picker.productId.value) await Promise.all([refreshStats(), fetchBatches(batchPagination.value.page)])
  }

  const init = async () => {
    await picker.loadOptions()
    await fetchSecrets(1)
  }

  // ---- picker / filters ----
  const onProductChange = async () => {
    picker.skuValue.value = '__all__'
    currentBatch.value = null
    await refreshAll()
  }
  const onSkuChange = async () => {
    currentBatch.value = null
    await refreshAll()
  }
  const applyFilters = async () => {
    clearMessages()
    await fetchSecrets(1, { preserveRows: true })
  }
  const resetFilters = async () => {
    Object.assign(filters, { status: '__all__', secret: '', batchNo: '' })
    currentBatch.value = null
    clearMessages()
    await fetchSecrets(1, { preserveRows: true })
  }
  const refreshCurrent = () => refreshList(() => refreshAll({ preserveRows: true }))
  const refreshBatches = () => refreshList(() => fetchBatches(1, { preserveRows: true }))

  const changeBatchPage = (page: number) => {
    if (page < 1 || page > batchPagination.value.total_page) return
    void fetchBatches(page)
  }
  const changeBatchPageSize = (size: number) => {
    if (size === batchPagination.value.page_size) return
    batchPagination.value = { ...batchPagination.value, page_size: size }
    void fetchBatches(1)
  }
  const changeSecretPage = (page: number) => {
    if (page < 1 || page > secretPagination.value.total_page) return
    void fetchSecrets(page)
  }
  const changeSecretPageSize = (size: number) => {
    if (size === secretPagination.value.page_size) return
    secretPagination.value = { ...secretPagination.value, page_size: size }
    void fetchSecrets(1)
  }

  const filterByBatch = async (batch: AdminCardSecretBatch | null) => {
    currentBatch.value = batch
    clearMessages()
    await fetchSecrets(1)
  }
  const filterByBatchId = async (raw: number) => {
    const id = Math.floor(Number(raw))
    if (!Number.isFinite(id) || id <= 0) return
    const matched =
      (currentBatch.value && currentBatch.value.id === id ? currentBatch.value : null) || batches.value.find((b) => Number(b.id || 0) === id) || null
    await filterByBatch(
      matched || {
        id,
        product_id: picker.productId.value || 0,
        sku_id: picker.skuId.value || 0,
        name: '',
        batch_no: '',
        source: '',
        note: '',
        total_count: 0,
        available_count: 0,
        reserved_count: 0,
        used_count: 0,
        created_at: '',
      },
    )
  }
  const clearBatchFilter = async () => {
    currentBatch.value = null
    await fetchSecrets(1)
  }

  // ---- scoped batch actions ----
  const buildActionPayload = (): { ids: number[] } | { filter: AdminCardSecretQueryPayload } | null => {
    if (operationScope.value === 'selected') {
      const ids = normalizeIds(selectedIds.value)
      if (!ids.length) {
        actionError.value = t('admin.cardSecrets.errors.selectRequired')
        return null
      }
      return { ids }
    }
    if (filteredCount.value <= 0) {
      actionError.value = t('admin.cardSecrets.errors.scopeEmpty')
      return null
    }
    return { filter: queryFilter.value }
  }

  const dangerDescription = (suffix: string) => [
    { text: `${t('admin.cardSecrets.batch.confirmCountPrefix')} ` },
    { text: String(scopeCount.value), tone: 'danger' as const, strong: true },
    { text: ` ${suffix}` },
  ]

  const statusLabel = (status: string) => (['available', 'reserved', 'used'].includes(status) ? t(`admin.cardSecrets.status.${status}`) : status)

  const errorText = (err: unknown, fallbackKey: string) => (err instanceof Error && err.message ? err.message : t(fallbackKey))

  const applyBatchStatus = async () => {
    clearMessages()
    const payload = buildActionPayload()
    if (!payload) return
    const ok = await confirmAction({
      description: dangerDescription(t('admin.cardSecrets.batch.confirmStatusSuffix', { scope: scopeLabel.value, status: statusLabel(batchStatusTarget.value) })),
    })
    if (!ok) return
    actionLoading.value = true
    try {
      const res = await adminAPI.batchUpdateCardSecretStatus({ ...payload, status: batchStatusTarget.value })
      const affected = Number(res.data?.affected || scopeCount.value)
      actionSuccess.value = t('admin.cardSecrets.success.batchStatusUpdated', { count: affected })
      await refreshAfterMutations()
    } catch (err) {
      actionError.value = errorText(err, 'admin.cardSecrets.errors.batchStatusFailed')
    } finally {
      actionLoading.value = false
    }
  }

  const deleteInScope = async () => {
    clearMessages()
    const payload = buildActionPayload()
    if (!payload) return
    const ok = await confirmAction({
      description: dangerDescription(t('admin.cardSecrets.batch.confirmDeleteSuffix', { scope: scopeLabel.value })),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    actionLoading.value = true
    try {
      const res = await adminAPI.batchDeleteCardSecrets(payload)
      const affected = Number(res.data?.affected || scopeCount.value)
      actionSuccess.value = t('admin.cardSecrets.success.batchDeleted', { count: affected })
      if (operationScope.value === 'filtered' && currentBatchId.value && affected >= filteredCount.value) currentBatch.value = null
      await refreshAfterMutations()
    } catch (err) {
      actionError.value = errorText(err, 'admin.cardSecrets.errors.batchDeleteFailed')
    } finally {
      actionLoading.value = false
    }
  }

  const exportInScope = async (format: 'txt' | 'csv') => {
    clearMessages()
    const payload = buildActionPayload()
    if (!payload) return
    actionLoading.value = true
    try {
      const res = await adminAPI.exportCardSecrets({ ...payload, format })
      const fallback = `card-secrets-${new Date().toISOString().replace(/[:.]/g, '-')}.${format}`
      const type = format === 'csv' ? 'text/csv;charset=utf-8' : 'text/plain;charset=utf-8'
      downloadBlob(new Blob([res.data], { type }), filenameFromDisposition(res.headers['content-disposition'], fallback))
      actionSuccess.value = t('admin.cardSecrets.success.batchExported', { count: scopeCount.value, format: format.toUpperCase() })
    } catch (err) {
      actionError.value = errorText(err, 'admin.cardSecrets.errors.batchExportFailed')
    } finally {
      actionLoading.value = false
    }
  }

  const openEdit = (secret: AdminCardSecret) => {
    editingSecret.value = secret
    showEditModal.value = true
  }

  return {
    picker,
    refreshing,
    stats,
    statsLoading,
    batches,
    batchesLoading,
    batchPagination,
    secrets,
    secretsLoading,
    secretPagination,
    filters,
    currentBatch,
    currentBatchId,
    productHint,
    batchFilterText,
    selectedIds,
    selectedCount,
    operationScope,
    batchStatusTarget,
    actionLoading,
    actionError,
    actionSuccess,
    scopeCount,
    scopeLabel,
    scopeHint,
    hasActionableScope,
    allPageSelected,
    somePageSelected,
    toggleSelectAll,
    toggleSelected,
    showEditModal,
    editingSecret,
    init,
    onProductChange,
    onSkuChange,
    applyFilters,
    resetFilters,
    refreshCurrent,
    refreshBatches,
    changeBatchPage,
    changeBatchPageSize,
    changeSecretPage,
    changeSecretPageSize,
    filterByBatch,
    filterByBatchId,
    clearBatchFilter,
    applyBatchStatus,
    deleteInScope,
    exportInScope,
    openEdit,
    refreshAfterMutations,
    statusLabel,
  }
}
