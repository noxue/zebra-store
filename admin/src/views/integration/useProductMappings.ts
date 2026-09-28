import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { api, errorMessage } from '@/api/client'
import type { AdminCategory, AdminProductMapping, AdminProductSKU, AdminSiteConnection, LocalizedText, Money } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, getLocalizedText } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifyError, notifySuccess } from '@/utils/notify'
import { isRecord, priceRange, type SkuMapping } from './integrationUtils'

/** A mapping row as returned by `GET product-mappings` (with the local product embedded). */
export type MappingRow = AdminProductMapping & {
  product?: { id: number; title?: LocalizedText; price_amount?: Money; fulfillment_type?: string; skus?: AdminProductSKU[] }
}

export interface MappingDetail {
  mapping: AdminProductMapping
  sku_mappings: SkuMapping[]
}

const parseDetail = (data: unknown): MappingDetail | null => {
  if (!isRecord(data)) return null
  return {
    mapping: (isRecord(data.mapping) ? data.mapping : data) as AdminProductMapping,
    sku_mappings: Array.isArray(data.sku_mappings) ? (data.sku_mappings as SkuMapping[]) : [],
  }
}

const batchSuccessCount = (data: unknown) => (isRecord(data) ? Number(data.success_count ?? 0) || 0 : 0)

export const localProductTitle = (m: MappingRow) => (m.product ? getLocalizedText(m.product.title) : `#${m.local_product_id}`)
export const localPriceRange = (m: MappingRow) => {
  const p = m.product
  if (!p) return '-'
  return priceRange((p.skus || []).map((s) => s.price_amount), p.price_amount)
}

/** Page logic for 商品映射 (list, expand detail, row + batch actions). */
export function useProductMappings() {
  const t = i18n.global.t
  const connections = ref<AdminSiteConnection[]>([])
  const categories = ref<AdminCategory[]>([])
  const filters = reactive({ connection_id: '__all__' as string | number, upstream_status: '__all__', product_status: '__all__', search: '' })
  const syncingId = ref<number | null>(null)
  const expandedId = ref<number | null>(null)
  const detailLoading = ref(false)
  const detail = ref<MappingDetail | null>(null)
  const selected = ref<Set<number>>(new Set())
  const batchOperating = ref(false)

  const list = useListPage<MappingRow>({
    fetchFn: (page, pageSize) => {
      expandedId.value = null
      detail.value = null
      selected.value = new Set()
      return adminAPI.getProductMappings(
        cleanParams({
          page,
          page_size: pageSize,
          connection_id: filters.connection_id,
          upstream_status: filters.upstream_status,
          product_status: filters.product_status,
          search: filters.search.trim(),
        }),
      )
    },
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const fetchConnections = async () => {
    try {
      connections.value = (await adminAPI.getSiteConnections({ page_size: 100 })).data ?? []
    } catch {
      connections.value = []
    }
  }
  const fetchCategories = async () => {
    try {
      categories.value = (await adminAPI.getCategories()).data ?? []
    } catch {
      categories.value = []
    }
  }

  const connectionName = (id: number) => connections.value.find((c) => c.id === id)?.name || `#${id}`
  const exchangeRate = (id: number) => {
    const rate = Number(connections.value.find((c) => c.id === id)?.exchange_rate)
    return Number.isFinite(rate) && rate > 0 ? rate : 1
  }

  const skuMappingByLocalId = computed(() => {
    const map = new Map<number, SkuMapping>()
    for (const sm of detail.value?.sku_mappings ?? []) map.set(sm.local_sku_id, sm)
    return map
  })

  const toggleExpand = async (m: MappingRow) => {
    if (expandedId.value === m.id) {
      expandedId.value = null
      detail.value = null
      return
    }
    expandedId.value = m.id
    detailLoading.value = true
    detail.value = null
    try {
      const res = await adminAPI.getProductMapping(m.id)
      if (expandedId.value === m.id) detail.value = parseDetail(res.data)
    } catch {
      detail.value = null
    } finally {
      detailLoading.value = false
    }
  }

  // --- selection ---
  const allSelected = computed(() => list.items.value.length > 0 && list.items.value.every((m) => selected.value.has(m.id)))
  const someSelected = computed(() => selected.value.size > 0 && !allSelected.value)
  const toggleSelect = (id: number) => {
    const next = new Set(selected.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selected.value = next
  }
  const toggleAll = () => {
    selected.value = allSelected.value ? new Set() : new Set(list.items.value.map((m) => m.id))
  }
  const clearSelection = () => (selected.value = new Set())

  const runBatch = async (fn: (ids: number[]) => Promise<{ data?: unknown }>, messageKey: string) => {
    const ids = Array.from(selected.value)
    if (ids.length === 0) return
    batchOperating.value = true
    try {
      const res = await fn(ids)
      notifySuccess(t(messageKey, { success: batchSuccessCount(res.data), total: ids.length }))
      selected.value = new Set()
      void list.refresh()
    } catch {
      /* already notified */
    } finally {
      batchOperating.value = false
    }
  }
  const batchSync = () => runBatch((ids) => adminAPI.batchSyncProductMappings(ids), 'productMappings.batch.syncResult')
  const batchStatus = (active: boolean) =>
    runBatch((ids) => adminAPI.batchUpdateProductMappingStatus(ids, active), 'productMappings.batch.statusResult')
  const batchDelete = async () => {
    if (selected.value.size === 0) return
    const ok = await confirmAction({
      description: t('productMappings.batch.deleteConfirm', { count: selected.value.size }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    await runBatch((ids) => adminAPI.batchDeleteProductMappings(ids), 'productMappings.batch.deleteResult')
  }

  // --- row actions ---
  const sync = async (m: MappingRow) => {
    syncingId.value = m.id
    try {
      await api.post(`/admin/product-mappings/${m.id}/sync`, undefined, { silent: true, timeout: 60_000 })
      notifySuccess(t('productMappings.sync.success'))
      void list.refresh()
    } catch (err) {
      notifyError(`${t('productMappings.sync.failed')}: ${errorMessage(err)}`)
    } finally {
      syncingId.value = null
    }
  }

  const toggleStatus = async (m: MappingRow) => {
    try {
      await adminAPI.updateProductMappingStatus(m.id, { is_active: !m.is_active })
      notifySuccess()
      void list.refresh()
    } catch {
      /* already notified */
    }
  }

  const remove = async (m: MappingRow) => {
    const ok = await confirmAction({
      description: t('productMappings.delete.confirm', { id: m.id }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteProductMapping(m.id)
      notifySuccess()
      void list.refresh()
    } catch {
      /* already notified */
    }
  }

  const init = () => {
    void fetchConnections()
    void fetchCategories()
    void list.fetchData(1)
  }

  return {
    list,
    filters,
    connections,
    categories,
    refreshing,
    refresh,
    syncingId,
    expandedId,
    detailLoading,
    detail,
    skuMappingByLocalId,
    selected,
    allSelected,
    someSelected,
    batchOperating,
    connectionName,
    exchangeRate,
    toggleExpand,
    toggleSelect,
    toggleAll,
    clearSelection,
    batchSync,
    batchStatus,
    batchDelete,
    sync,
    toggleStatus,
    remove,
    fetchCategories,
    init,
  }
}
