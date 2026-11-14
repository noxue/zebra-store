import { computed, onBeforeUnmount, ref, watch, type ComputedRef, type Ref } from 'vue'
import { useRoute, useRouter, type LocationQueryRaw } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { categoryAPI, productAPI } from '@/api/catalog'
import type { Category, Product, ProductListParams } from '@/api/types'
import { buildCategoryGroups, createCategoryMap, normalizeCategoryParentId, type PublicCategory } from '@/utils/category'
import { debounce } from '@/utils/debounce'
import { CatalogResponseError, catalogReadError, type CatalogReadError } from '@/utils/catalogReadError'
import { parseFacetQuery, withoutFacetQuery } from '@/utils/catalogFacets'
import { useCatalogFacets } from './useCatalogFacets'
import { useLocalized } from './useLocalized'

export interface UseProductListOptions {
  pageSize?: number
  homeRouteName?: string
  categoryRouteName?: string
}

/** Search debounce delay in ms (original: 300). */
const SEARCH_DEBOUNCE_MS = 300

/** Product listing with category tree, search, pagination and route sync. */
export function useProductList(options: UseProductListOptions = {}) {
  const { pageSize: defaultPageSize = 20, homeRouteName = 'home', categoryRouteName = 'category-products' } = options
  const router = useRouter()
  const route = useRoute()

  const loading = ref(true)
  const ready = ref(false)
  const products = ref<Product[]>([])
  const categories = ref<Category[]>([])
  const selectedCategory = ref<number | null>(null)
  const searchQuery = ref('')
  const facets = useCatalogFacets(selectedCategory, searchQuery)
  const error = ref<CatalogReadError | null>(null)
  const categoryError = ref(false)
  const routeQueryError = ref(false)
  const currentPage = ref(1)
  const pageSize = ref(defaultPageSize)
  const totalPages = ref(0)
  const total = ref(0)
  const showFilterDrawer = ref(false)
  const expandedParentIds = ref<number[]>([])

  const categoryGroups = computed(() => buildCategoryGroups(categories.value as PublicCategory[]))
  const categoryMap = computed(() => createCategoryMap(categories.value as PublicCategory[]))
  const hasFilters = computed(() => selectedCategory.value !== null || searchQuery.value.trim() !== '' || facets.hasSelection.value || facets.routeError.value || routeQueryError.value)
  const blocked = computed(() => facets.blocked.value || routeQueryError.value)

  let initializing = true
  let applyingRoute = false
  let disposed = false
  let listingVersion = 0
  let intentVersion = 0

  const isParentExpanded = (id: number) => expandedParentIds.value.includes(id)
  const expandParent = (id: number) => {
    if (!id || isParentExpanded(id)) return
    expandedParentIds.value = [...expandedParentIds.value, id]
  }
  const toggleParentCategory = (id: number) => {
    if (isParentExpanded(id)) expandedParentIds.value = expandedParentIds.value.filter((x) => x !== id)
    else expandParent(id)
  }
  const syncExpanded = () => {
    if (!selectedCategory.value) return
    const matched = categoryMap.value.get(selectedCategory.value)
    if (!matched) return
    const parentId = normalizeCategoryParentId(matched.parent_id)
    if (parentId > 0) {
      expandParent(parentId)
      return
    }
    const group = categoryGroups.value.find((g) => g.id === matched.id)
    if (group?.children.length) expandParent(group.id)
  }

  const selectCategory = (id: number | null, closeDrawer = false) => {
    selectedCategory.value = id
    if (closeDrawer) showFilterDrawer.value = false
  }

  const loadProducts = async () => {
    const version = ++listingVersion
    if (disposed) return
    loading.value = true
    error.value = null
    if (blocked.value) {
      products.value = []
      total.value = 0
      totalPages.value = 0
      loading.value = false
      return
    }
    try {
      const params: ProductListParams = { page: currentPage.value, page_size: pageSize.value, ...facets.filters.value }
      if (selectedCategory.value) params.category_id = selectedCategory.value
      const keyword = searchQuery.value.trim()
      if (keyword) params.search = keyword
      const res = await productAPI.list(params)
      if (disposed || version !== listingVersion) return
      if (!Array.isArray(res.data) || !res.pagination || !Number.isSafeInteger(res.pagination.total) || res.pagination.total < 0 || !Number.isSafeInteger(res.pagination.total_page) || res.pagination.total_page < 0) throw new CatalogResponseError()
      products.value = res.data
      totalPages.value = res.pagination.total_page
      total.value = res.pagination.total
    } catch (failure) {
      if (!disposed && version === listingVersion) {
        error.value = catalogReadError(failure)
        total.value = 0
        totalPages.value = 0
      }
    } finally {
      if (!disposed && version === listingVersion) loading.value = false
    }
  }

  const loadCategories = async () => {
    categoryError.value = false
    try {
      const res = await categoryAPI.list()
      if (disposed) return
      if (!Array.isArray(res.data)) throw new CatalogResponseError()
      categories.value = res.data
    } catch {
      if (!disposed) categoryError.value = true
    }
  }

  const listingRoute = () => route.name === 'home' || route.name === 'products' || route.name === categoryRouteName
  const syncUrl = async () => {
    if (disposed || !listingRoute()) return
    const query = withoutFacetQuery(route.query) as LocationQueryRaw
    delete query.search
    delete query.page
    if (searchQuery.value.trim()) query.search = searchQuery.value.trim()
    if (currentPage.value > 1) query.page = String(currentPage.value)
    Object.assign(query, facets.filters.value)
    const matched = selectedCategory.value ? categories.value.find((c) => c.id === selectedCategory.value) : undefined
    const target = matched?.slug
      ? { name: categoryRouteName, params: { slug: matched.slug }, query }
      : { name: route.name === 'products' ? 'products' : homeRouteName, query }
    if (router.resolve(target).fullPath !== route.fullPath) await router.replace(target)
  }
  const runLoad = async (writeUrl: boolean) => {
    const version = intentVersion
    try {
      if (writeUrl) await syncUrl()
      if (!disposed && version === intentVersion) await loadProducts()
    } catch (failure) {
      if (!disposed && version === intentVersion) { error.value = catalogReadError(failure); loading.value = false }
    }
  }
  const debouncedLoad = debounce((writeUrl: boolean) => void runLoad(writeUrl), SEARCH_DEBOUNCE_MS)
  const schedule = (resetPage: boolean, writeUrl: boolean) => {
    listingVersion += 1
    intentVersion += 1
    error.value = null
    loading.value = true
    totalPages.value = 0
    if (resetPage) currentPage.value = 1
    debouncedLoad(writeUrl)
  }

  const changePage = (page: number) => {
    if (page < 1 || page > totalPages.value) return
    currentPage.value = page
    schedule(false, true)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const clearSearch = () => {
    if (!searchQuery.value) return
    searchQuery.value = ''
  }

  const clearFilters = () => {
    applyingRoute = true
    routeQueryError.value = false
    searchQuery.value = ''
    selectedCategory.value = null
    facets.clear()
    currentPage.value = 1
    applyingRoute = false
    schedule(false, true)
  }
  const canonicalFilters = (value: object) => JSON.stringify(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0))
  const syncFromRoute = () => {
    let category: number | null = null
    let invalid = false
    if (route.name === categoryRouteName) {
      const slug = route.params.slug
      const matched = typeof slug === 'string' ? categories.value.find((c) => c.slug === slug) : undefined
      if (!matched) invalid = true
      else category = matched.id
    }
    const rawSearch = route.query.search
    if (rawSearch != null && typeof rawSearch !== 'string') invalid = true
    const keyword = typeof rawSearch === 'string' ? rawSearch : ''
    const rawPage = route.query.page
    const pageNumber = typeof rawPage === 'string' && /^[1-9]\d*$/.test(rawPage) ? Number(rawPage) : 1
    if (rawPage != null && (typeof rawPage !== 'string' || !/^[1-9]\d*$/.test(rawPage) || !Number.isSafeInteger(pageNumber))) invalid = true
    const page = Number.isSafeInteger(pageNumber) && pageNumber > 0 ? pageNumber : 1
    const parsed = parseFacetQuery(route.query)
    const changed = selectedCategory.value !== category || searchQuery.value !== keyword || currentPage.value !== page || routeQueryError.value !== invalid || facets.routeError.value !== !parsed.ok || canonicalFilters(facets.filters.value) !== canonicalFilters(parsed.ok ? parsed.filters : {})
    if (!changed) return false
    applyingRoute = true
    selectedCategory.value = category
    searchQuery.value = keyword
    currentPage.value = page
    routeQueryError.value = invalid
    facets.restore(route.query)
    applyingRoute = false
    syncExpanded()
    return true
  }
  watch(selectedCategory, () => {
    if (initializing || applyingRoute) return
    facets.clear()
    routeQueryError.value = false
    syncExpanded()
    schedule(true, true)
  }, { flush: 'sync' })
  watch([searchQuery, facets.filters], () => {
    if (initializing || applyingRoute) return
    schedule(true, true)
  }, { deep: true, flush: 'sync' })
  watch(() => route.fullPath, () => {
    if (initializing) return
    if (!listingRoute()) { intentVersion += 1; listingVersion += 1; debouncedLoad.cancel(); return }
    if (syncFromRoute()) schedule(false, false)
  })
  const initialize = async () => {
    await loadCategories()
    if (disposed) return
    syncFromRoute()
    syncExpanded()
    await facets.initialize(false)
    if (disposed) return
    initializing = false
    ready.value = true
    await loadProducts()
  }
  const retryFilters = async () => {
    await facets.retryCapabilities()
    if (!disposed) await loadProducts()
  }
  const retryCategories = async () => {
    await loadCategories()
    if (disposed) return
    syncFromRoute()
    await facets.reload()
    await loadProducts()
  }
  onBeforeUnmount(() => {
    disposed = true
    intentVersion += 1
    listingVersion += 1
    debouncedLoad.cancel()
  })

  return {
    loading,
    ready,
    error,
    categoryError,
    routeQueryError,
    blocked,
    facets,
    retryCategories,
    retryFilters,
    products,
    categories,
    selectedCategory,
    searchQuery,
    currentPage,
    pageSize,
    totalPages,
    total,
    showFilterDrawer,
    expandedParentIds,
    categoryGroups,
    categoryMap,
    hasFilters,
    isParentExpanded,
    toggleParentCategory,
    selectCategory,
    loadProducts,
    changePage,
    clearSearch,
    clearFilters,
    initialize,
  }
}

export interface ProductGroup {
  categoryId: number | null
  categoryName: string
  categoryIcon: string | null
  products: Product[]
}

/** Groups products by top-level category, keeping first-seen order. */
export const groupProductsByCategory = (
  products: Product[],
  categoryMap: Map<number, PublicCategory>,
  localize: (v: Category['name'] | undefined) => string,
  allLabel: string,
): ProductGroup[] => {
  const groups = new Map<number | null, ProductGroup>()
  const order: Array<number | null> = []
  for (const product of products) {
    const directId = product.category?.id ?? null
    let groupId = directId
    let name = directId !== null ? localize(product.category?.name) : allLabel
    let icon: string | null = product.category?.icon ?? null
    if (directId !== null) {
      const entry = categoryMap.get(directId)
      const parentId = entry ? normalizeCategoryParentId(entry.parent_id) : 0
      const parent = parentId > 0 ? categoryMap.get(parentId) : undefined
      if (parent) {
        groupId = parent.id
        name = localize(parent.name as Category['name'])
        icon = parent.icon ?? null
      }
    }
    let group = groups.get(groupId)
    if (!group) {
      group = { categoryId: groupId, categoryName: name, categoryIcon: icon, products: [] }
      groups.set(groupId, group)
      order.push(groupId)
    }
    group.products.push(product)
  }
  return order.map((k) => groups.get(k) as ProductGroup)
}

export function useProductListGroups(products: Ref<Product[]>, categoryMap: ComputedRef<Map<number, PublicCategory>>) {
  const { getLocalizedText } = useLocalized()
  const { t } = useI18n()
  return computed(() => groupProductsByCategory(products.value, categoryMap.value, (v) => getLocalizedText(v), t('products.allCategories')))
}
