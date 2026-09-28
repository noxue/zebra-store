import { computed, nextTick, reactive, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCategory, AdminProduct } from '@/api/types'
import { cleanParams, formatMoney, getLocalizedText } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import {
  buildAdminCategoryPath,
  createAdminCategoryChildCountMap,
  createAdminCategoryMap,
  flattenAdminCategories,
  isAdminProductCategorySelectable,
} from '@/utils/category'
import { formatWholesaleTierScopeLabel, wholesaleTierScopeValue } from '@/utils/wholesalePricing'
import { useListPage } from '@/composables/useListPage'
import { useSelection } from '@/composables/useSelection'
import { parseWholesaleQuery, resolveManualStockMetrics, statusQueryValue, toSafeInt, wholesaleQueryValue } from './productUtils'

export const PRODUCT_PAGE_SIZE_OPTIONS = [10, 20, 50, 100]

/** Page logic for 商品管理 list: filters, inline edits, batch actions and deep links. */
export function useProducts() {
  const t = i18n.global.t
  const route = useRoute()
  const router = useRouter()

  const filters = reactive({ search: '', stockStatus: 'all', category: 'all' as string | number, status: 'all', wholesale: 'all' })
  const siteCurrency = ref('CNY')
  const categories = ref<AdminCategory[]>([])
  const categoryMap = computed(() => createAdminCategoryMap(categories.value))
  const childCountMap = computed(() => createAdminCategoryChildCountMap(categories.value))
  const orderedCategories = computed(() => flattenAdminCategories(categories.value).map((item) => item.category))

  /** Category <select> options; parents with children cannot hold products (disabled). */
  const categoryOptions = computed(() =>
    flattenAdminCategories(categories.value).map((item) => ({
      value: item.category.id,
      label: item.depth > 0 ? `\u3000${getLocalizedText(item.category.name)}` : categoryLabel(item.category),
      disabled: !isAdminProductCategorySelectable(item.category, childCountMap.value),
    })),
  )
  const categoryLabel = (category?: AdminCategory) =>
    category ? buildAdminCategoryPath(category, categoryMap.value, (item) => getLocalizedText(item.name)) : ''

  const list = useListPage<AdminProduct>({
    pageSize: 10,
    fetchFn: (page, pageSize) => {
      selection.clear()
      return adminAPI.getProducts(
        cleanParams({
          page,
          page_size: pageSize,
          search: filters.search,
          stock_status: filters.stockStatus,
          is_active: statusQueryValue(filters.status),
          wholesale: filters.wholesale,
          category_id: filters.category === 'all' ? undefined : filters.category,
        }),
      )
    },
  })
  const selection = useSelection(list.items, (p) => p.id)
  const rows = list.items

  async function fetchCategories() {
    try {
      categories.value = (await adminAPI.getCategories({ type: 'product' })).data ?? []
    } catch {
      categories.value = []
    }
  }

  async function fetchSiteCurrency() {
    try {
      const data = (await adminAPI.getSettings<Record<string, unknown>>({ key: 'site_config' })).data
      const raw = String(data?.currency || 'CNY')
        .trim()
        .toUpperCase()
      siteCurrency.value = /^[A-Z]{3}$/.test(raw) ? raw : 'CNY'
    } catch {
      siteCurrency.value = 'CNY'
    }
  }

  // ---- filters ----
  const handleWholesaleChange = () => {
    const current = Array.isArray(route.query.wholesale) ? route.query.wholesale[0] : route.query.wholesale
    const next = wholesaleQueryValue(filters.wholesale)
    if ((current || undefined) !== next) {
      void router.replace({ query: { ...route.query, wholesale: next } })
      return
    }
    void list.fetchData(1)
  }

  const resetFilters = () => {
    Object.assign(filters, { search: '', stockStatus: 'all', category: 'all', status: 'all', wholesale: 'all' })
    if (route.query.wholesale !== undefined) void router.replace({ query: { ...route.query, wholesale: undefined } })
    else void list.fetchData(1)
    void nextTick(() => (document.getElementById('admin-products-search') as HTMLInputElement | null)?.focus())
  }

  watch(
    () => route.query.wholesale,
    (value) => {
      filters.wholesale = parseWholesaleQuery(value)
      void list.fetchData(1)
    },
  )

  // ---- modal / deep links ----
  const showModal = ref(false)
  const editingProductId = ref<number | null>(null)
  const openCreate = () => {
    editingProductId.value = null
    showModal.value = true
  }
  const openEditById = (raw: unknown) => {
    const id = Number(raw)
    if (!Number.isFinite(id) || id <= 0) return
    editingProductId.value = id
    showModal.value = true
  }

  const init = () => {
    filters.wholesale = parseWholesaleQuery(route.query.wholesale)
    void list.fetchData(1)
    void fetchCategories()
    void fetchSiteCurrency()
    if (route.query.action === 'create') {
      openCreate()
      void router.replace({ query: { ...route.query, action: undefined } })
    }
    if (route.query.product_id) openEditById(route.query.product_id)
  }
  watch(
    () => route.query.product_id,
    (value) => value && openEditById(value),
  )

  // ---- inline edits ----
  const touch = () => (rows.value = [...rows.value])

  const toggleStatus = async (product: AdminProduct) => {
    const next = !product.is_active
    product.is_active = next
    touch()
    try {
      await adminAPI.patchProduct(product.id, { is_active: next })
      // 上架状态筛选生效时，该行已不满足筛选条件，需要重新拉取
      if (filters.status !== 'all') void list.refresh()
    } catch {
      product.is_active = !next
      touch()
    }
  }

  const editingSortId = ref<number | null>(null)
  const editingSortValue = ref<string | number>('')
  const startEditSort = (product: AdminProduct) => {
    editingSortId.value = product.id
    editingSortValue.value = product.sort_order || 0
    void nextTick(() => {
      const input = document.getElementById(`sort-input-${product.id}`) as HTMLInputElement | null
      input?.focus()
      input?.select()
    })
  }
  const cancelEditSort = () => (editingSortId.value = null)
  const saveSort = async (product: AdminProduct) => {
    if (editingSortId.value !== product.id) return
    const newValue = Math.max(Math.floor(Number(editingSortValue.value) || 0), 0)
    editingSortId.value = null
    const oldValue = product.sort_order || 0
    if (newValue === oldValue) return
    product.sort_order = newValue
    touch()
    try {
      await adminAPI.patchProduct(product.id, { sort_order: newValue })
    } catch {
      product.sort_order = oldValue
      touch()
    }
  }

  const editingCategoryId = ref<number | null>(null)
  const startEditCategory = (product: AdminProduct) => (editingCategoryId.value = product.id)
  const cancelEditCategory = () => (editingCategoryId.value = null)
  const saveCategory = async (product: AdminProduct, raw: unknown) => {
    editingCategoryId.value = null
    const numId = Number(raw)
    if (!numId || numId === product.category_id) return
    const oldId = product.category_id
    const oldCategory = product.category
    product.category_id = numId
    product.category = categories.value.find((c) => c.id === numId)
    touch()
    try {
      await adminAPI.patchProduct(product.id, { category_id: numId })
    } catch {
      product.category_id = oldId
      product.category = oldCategory
      touch()
    }
  }

  const remove = async (product: AdminProduct) => {
    const ok = await confirmAction({
      description: t('admin.products.confirmDelete', { name: getLocalizedText(product.title) }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteProduct(product.id)
      void list.refresh()
    } catch {
      /* already notified */
    }
  }

  // ---- batch ----
  const batchOperating = ref(false)
  const batchCategoryId = ref<string | number>('')
  const runBatch = async (fn: (ids: number[]) => Promise<{ data: { success_count?: number } | null }>, resultKey: string) => {
    const ids = selection.selectedIds.value
    if (!ids.length) return
    batchOperating.value = true
    try {
      const res = await fn(ids)
      notifySuccess(t(resultKey, { success: res.data?.success_count || 0, total: ids.length }))
      selection.clear()
      void list.refresh()
    } catch {
      /* already notified */
    } finally {
      batchOperating.value = false
    }
  }
  const batchStatus = (isActive: boolean) => runBatch((ids) => adminAPI.batchUpdateProductStatus(ids, isActive), 'admin.products.batch.statusResult')
  const batchCategory = async () => {
    if (!batchCategoryId.value) return
    await runBatch((ids) => adminAPI.batchUpdateProductCategory(ids, Number(batchCategoryId.value)), 'admin.products.batch.categoryResult')
    batchCategoryId.value = ''
  }
  const batchDelete = async () => {
    const count = selection.selectedIds.value.length
    if (!count) return
    const ok = await confirmAction({
      description: t('admin.products.batch.deleteConfirm', { count }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    await runBatch((ids) => adminAPI.batchDeleteProducts(ids), 'admin.products.batch.deleteResult')
  }

  // ---- display helpers ----
  const formatPrice = (amount: AdminProduct['price_amount']) => formatMoney(amount, siteCurrency.value)

  const formatWholesaleSummary = (product: AdminProduct) => {
    const tiers = Array.isArray(product.wholesale_prices) ? product.wholesale_prices : []
    if (!tiers.length) return ''
    return tiers
      .slice()
      .sort((a, b) => {
        const sa = wholesaleTierScopeValue(a)
        const sb = wholesaleTierScopeValue(b)
        if (sa !== sb) return sa.localeCompare(sb)
        return Number(a.min_quantity || 0) - Number(b.min_quantity || 0)
      })
      .map((tier) => {
        const scope = formatWholesaleTierScopeLabel(product, tier, String(i18n.global.locale.value || 'zh-CN'), t('admin.wholesalePrices.modal.skuAll'))
        return `${scope} ≥${Number(tier.min_quantity || 0)} ${formatPrice(tier.unit_price)}`
      })
      .join(' / ')
  }

  const manualStockSummary = (product: AdminProduct) => {
    const m = resolveManualStockMetrics(product)
    if (m.total === -1) return { text: t('admin.products.stock.unlimited'), tone: 'neutral' as const }
    return {
      text: t('admin.products.stock.manualSummary', { remaining: Math.max(m.total, 0), locked: m.locked, sold: m.sold }),
      tone: m.total <= 0 ? ('danger' as const) : ('success' as const),
    }
  }
  const autoStockSummary = (product: AdminProduct) => {
    const available = toSafeInt(product.auto_stock_available)
    return {
      text: t('admin.products.stock.summary', {
        total: toSafeInt(product.auto_stock_total),
        locked: toSafeInt(product.auto_stock_locked),
        sold: toSafeInt(product.auto_stock_sold),
        available,
      }),
      tone: available <= 0 ? ('danger' as const) : ('success' as const),
    }
  }

  return {
    filters,
    list,
    rows,
    selection,
    siteCurrency,
    categories,
    orderedCategories,
    categoryOptions,
    categoryLabel,
    handleWholesaleChange,
    resetFilters,
    showModal,
    editingProductId,
    openCreate,
    openEditById,
    init,
    toggleStatus,
    editingSortId,
    editingSortValue,
    startEditSort,
    cancelEditSort,
    saveSort,
    editingCategoryId,
    startEditCategory,
    cancelEditCategory,
    saveCategory,
    remove,
    batchOperating,
    batchCategoryId,
    batchStatus,
    batchCategory,
    batchDelete,
    formatPrice,
    formatWholesaleSummary,
    manualStockSummary,
    autoStockSummary,
  }
}
