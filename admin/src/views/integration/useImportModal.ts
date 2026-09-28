import { computed, ref, watch, type Ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { api, ApiError, errorMessage } from '@/api/client'
import type { AdminCategory } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import {
  buildAdminCategoryPath,
  createAdminCategoryChildCountMap,
  createAdminCategoryMap,
  flattenAdminCategories,
  isAdminProductCategorySelectable,
} from '@/utils/category'
import { buildCategoryDisplayList, chunk, groupProductsByCategory, isRecord, parseUpstreamResult, type UpstreamCategory, type UpstreamProduct } from './integrationUtils'

export const UPSTREAM_PAGE_SIZE = 50
export const IMPORT_BATCH_SIZE = 3
export const NO_CATEGORY = '__none__'

interface ImportResult {
  upstream_product_id: number
  success: boolean
  error?: string
}

export interface UseImportModalOptions {
  categories: Ref<AdminCategory[]>
  /** Called after products were imported (refresh mapping list / categories). */
  onImported: (opts: { categoriesChanged: boolean }) => void
  onClose: () => void
}

/** State + logic of the "import from upstream" dialog (flat list / by-category, load more, batch import with fallbacks). */
export function useImportModal(options: UseImportModalOptions) {
  const t = i18n.global.t
  const connectionId = ref<number | ''>('')
  const categoryId = ref<number | string>(NO_CATEGORY)
  const products = ref<UpstreamProduct[]>([])
  const mappedIds = ref<Set<number>>(new Set())
  const loading = ref(false)
  const loadingMore = ref(false)
  const page = ref(1)
  const total = ref(0)
  const selectedIds = ref<Set<number>>(new Set())
  const expandedIds = ref<Set<number>>(new Set())
  const importing = ref(false)
  const progress = ref({ done: 0, total: 0, success: 0 })

  const viewMode = ref<'category' | 'flat'>('category')
  const upstreamCategories = ref<UpstreamCategory[]>([])
  const categoriesSupported = ref(false)
  const loadingCategories = ref(false)
  const expandedCategoryIds = ref<Set<number>>(new Set())
  const autoCreateCategory = ref(false)
  const categoryImporting = ref(false)

  // --- local category options (leaf categories are selectable) ---
  const categoryOptions = computed(() => {
    const map = createAdminCategoryMap(options.categories.value)
    const childCount = createAdminCategoryChildCountMap(options.categories.value)
    return [
      { label: t('productMappings.import.noCategory'), value: NO_CATEGORY as number | string },
      ...flattenAdminCategories(options.categories.value).map((item) => ({
        label:
          item.depth > 0
            ? `\u3000└ ${getLocalizedText(item.category.name)}`
            : buildAdminCategoryPath(item.category, map, (c) => getLocalizedText(c.name)),
        value: item.category.id as number | string,
        disabled: !isAdminProductCategorySelectable(item.category, childCount),
      })),
    ]
  })
  const localCategoryId = () => (categoryId.value !== NO_CATEGORY ? Number(categoryId.value) || 0 : 0)

  // --- derived ---
  const hasMore = computed(() => products.value.length < total.value)
  const remaining = computed(() => Math.max(0, total.value - products.value.length))
  const selectable = computed(() => products.value.filter((p) => !mappedIds.value.has(p.id)))
  const allSelected = computed(() => selectable.value.length > 0 && selectable.value.every((p) => selectedIds.value.has(p.id)))
  const byCategory = computed(() => groupProductsByCategory(products.value))
  const categoryDisplayList = computed(() =>
    buildCategoryDisplayList(upstreamCategories.value, byCategory.value, mappedIds.value, (n) => getLocalizedText(n), t('productMappings.import.uncategorized')),
  )
  const showCategoryView = computed(() => viewMode.value === 'category' && categoriesSupported.value)

  // --- selection ---
  const toggleSelectAll = () => {
    selectedIds.value = allSelected.value ? new Set() : new Set(selectable.value.map((p) => p.id))
  }
  const toggleProduct = (id: number) => {
    if (mappedIds.value.has(id)) return
    const next = new Set(selectedIds.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selectedIds.value = next
  }
  const toggleSet = (target: Ref<Set<number>>, id: number) => {
    const next = new Set(target.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    target.value = next
  }
  const toggleSkuExpand = (id: number) => toggleSet(expandedIds, id)
  const toggleCategoryExpand = (id: number) => toggleSet(expandedCategoryIds, id)

  const nonMappedIn = (catId: number) => (byCategory.value.get(catId) || []).filter((p) => !mappedIds.value.has(p.id))
  const isCategoryAllSelected = (catId: number) => {
    const list = nonMappedIn(catId)
    return list.length > 0 && list.every((p) => selectedIds.value.has(p.id))
  }
  const selectAllInCategory = (catId: number, value: boolean) => {
    const next = new Set(selectedIds.value)
    for (const p of nonMappedIn(catId)) {
      if (value) next.add(p.id)
      else next.delete(p.id)
    }
    selectedIds.value = next
  }

  // --- fetch ---
  const fetchProducts = async (connId: number | '') => {
    if (!connId) {
      products.value = []
      total.value = 0
      mappedIds.value = new Set()
      return
    }
    loading.value = true
    page.value = 1
    try {
      const res = await adminAPI.getUpstreamProducts({ connection_id: connId, page: 1, page_size: UPSTREAM_PAGE_SIZE })
      const parsed = parseUpstreamResult(res.data)
      products.value = parsed.items
      total.value = parsed.total
      if (parsed.mappedIds) mappedIds.value = new Set(parsed.mappedIds)
    } catch {
      products.value = []
      total.value = 0
      mappedIds.value = new Set()
    } finally {
      loading.value = false
    }
  }

  const loadMore = async () => {
    if (!connectionId.value || loadingMore.value || !hasMore.value) return
    loadingMore.value = true
    try {
      const next = page.value + 1
      const res = await adminAPI.getUpstreamProducts({ connection_id: connectionId.value, page: next, page_size: UPSTREAM_PAGE_SIZE })
      const parsed = parseUpstreamResult(res.data)
      products.value = [...products.value, ...parsed.items]
      total.value = parsed.total
      page.value = next
    } catch {
      /* already notified */
    } finally {
      loadingMore.value = false
    }
  }

  const fetchUpstreamCategories = async (connId: number | '') => {
    if (!connId) {
      upstreamCategories.value = []
      categoriesSupported.value = false
      return
    }
    loadingCategories.value = true
    try {
      const res = await adminAPI.getUpstreamCategories({ connection_id: connId })
      const data = isRecord(res.data) ? res.data : {}
      upstreamCategories.value = Array.isArray(data.categories) ? (data.categories as UpstreamCategory[]) : []
      categoriesSupported.value = data.supported === true
    } catch {
      upstreamCategories.value = []
      categoriesSupported.value = false
    } finally {
      // Upstreams without category support fall back to the flat list.
      viewMode.value = categoriesSupported.value ? 'category' : 'flat'
      loadingCategories.value = false
    }
  }

  watch(connectionId, (value) => {
    selectedIds.value = new Set()
    expandedIds.value = new Set()
    expandedCategoryIds.value = new Set()
    autoCreateCategory.value = false
    void fetchProducts(value)
    void fetchUpstreamCategories(value)
  })

  const reset = () => {
    connectionId.value = ''
    categoryId.value = NO_CATEGORY
    products.value = []
    total.value = 0
    page.value = 1
    mappedIds.value = new Set()
    selectedIds.value = new Set()
    expandedIds.value = new Set()
  }

  // --- whole-category import ---
  const importCategory = async (upstreamCategoryId: number) => {
    if (!connectionId.value) return
    const localCatId = localCategoryId()
    if (!autoCreateCategory.value && localCatId === 0) {
      notifyError(t('productMappings.import.selectCategoryFirst'))
      return
    }
    categoryImporting.value = true
    try {
      const res = await adminAPI.batchImportByCategory({
        connection_id: Number(connectionId.value),
        upstream_category_id: upstreamCategoryId,
        auto_create_category: autoCreateCategory.value,
        local_category_id: localCatId || undefined,
      })
      const data = isRecord(res.data) ? res.data : {}
      const totalCount = Number(data.total ?? 0) || 0
      const success = Number(data.success_count ?? 0) || 0
      notifySuccess(t(success === totalCount ? 'productMappings.import.categoryImportSuccess' : 'productMappings.import.categoryImportPartial', { success, total: totalCount }))
      void fetchProducts(connectionId.value)
      options.onImported({ categoriesChanged: true })
    } catch {
      /* already notified */
    } finally {
      categoryImporting.value = false
    }
  }

  // --- batch import (chunks of 3; falls back to single import when batch endpoint is missing) ---
  const importOne = async (connId: number, id: number, catId: number): Promise<ImportResult> => {
    try {
      await api.post('/admin/product-mappings/import', { connection_id: connId, upstream_product_id: id, category_id: catId || undefined }, { silent: true, timeout: 60_000 })
      return { upstream_product_id: id, success: true }
    } catch (err) {
      return { upstream_product_id: id, success: false, error: errorMessage(err) }
    }
  }

  const batchImport = async () => {
    const ids = Array.from(selectedIds.value)
    if (ids.length === 0 || !connectionId.value) return
    const connId = Number(connectionId.value)
    const catId = localCategoryId()
    importing.value = true
    progress.value = { done: 0, total: ids.length, success: 0 }
    const results: ImportResult[] = []
    let successCount = 0
    try {
      for (const batch of chunk(ids, IMPORT_BATCH_SIZE)) {
        try {
          const res = await api.post<unknown>(
            '/admin/product-mappings/batch-import',
            { connection_id: connId, upstream_product_ids: batch, category_id: catId || undefined },
            { silent: true, timeout: 120_000 },
          )
          const data = isRecord(res.data) ? res.data : {}
          if (Array.isArray(data.results)) results.push(...(data.results as ImportResult[]))
          successCount += Number(data.success_count ?? 0) || 0
        } catch (err) {
          if (err instanceof ApiError && err.httpStatus === 404) {
            for (const id of batch) {
              const r = await importOne(connId, id, catId)
              results.push(r)
              if (r.success) successCount++
            }
          } else {
            for (const id of batch) results.push({ upstream_product_id: id, success: false, error: errorMessage(err) })
          }
        }
        progress.value = { done: Math.min(progress.value.done + batch.length, ids.length), total: ids.length, success: successCount }
      }

      if (successCount === ids.length) {
        notifySuccess(t('productMappings.import.batchSuccess', { count: successCount }))
        options.onClose()
      } else {
        const failedDetails = results
          .filter((r) => !r.success)
          .map((r) => {
            const prod = products.value.find((p) => p.id === r.upstream_product_id)
            const name = prod ? getLocalizedText(prod.title) : `#${r.upstream_product_id}`
            return `${name}: ${r.error || t('productMappings.import.unknownError')}`
          })
          .join('\n')
        if (successCount > 0) notifySuccess(t('productMappings.import.batchPartial', { success: successCount, total: ids.length }))
        notifyError(failedDetails)
        const okIds = new Set(results.filter((r) => r.success).map((r) => r.upstream_product_id))
        selectedIds.value = new Set([...selectedIds.value].filter((id) => !okIds.has(id)))
      }
      const nextMapped = new Set(mappedIds.value)
      for (const r of results) if (r.success) nextMapped.add(r.upstream_product_id)
      mappedIds.value = nextMapped
      options.onImported({ categoriesChanged: false })
    } finally {
      importing.value = false
    }
  }

  return {
    connectionId,
    categoryId,
    categoryOptions,
    products,
    mappedIds,
    loading,
    loadingMore,
    total,
    remaining,
    hasMore,
    selectedIds,
    expandedIds,
    importing,
    progress,
    viewMode,
    upstreamCategories,
    categoriesSupported,
    loadingCategories,
    expandedCategoryIds,
    autoCreateCategory,
    categoryImporting,
    allSelected,
    byCategory,
    categoryDisplayList,
    showCategoryView,
    toggleSelectAll,
    toggleProduct,
    toggleSkuExpand,
    toggleCategoryExpand,
    isCategoryAllSelected,
    selectAllInCategory,
    loadMore,
    reset,
    importCategory,
    batchImport,
  }
}

export type ImportModalState = ReturnType<typeof useImportModal>
