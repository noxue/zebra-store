import { computed, onBeforeUnmount, ref, watch, type ComputedRef, type Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { categoryAPI, productAPI } from '@/api/catalog'
import type { Category, Product, ProductListParams } from '@/api/types'
import { buildCategoryGroups, createCategoryMap, normalizeCategoryParentId, type PublicCategory } from '@/utils/category'
import { debounce } from '@/utils/debounce'
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
  const products = ref<Product[]>([])
  const categories = ref<Category[]>([])
  const selectedCategory = ref<number | null>(null)
  const searchQuery = ref('')
  const currentPage = ref(1)
  const pageSize = ref(defaultPageSize)
  const totalPages = ref(0)
  const total = ref(0)
  const showFilterDrawer = ref(false)
  const expandedParentIds = ref<number[]>([])

  const categoryGroups = computed(() => buildCategoryGroups(categories.value as PublicCategory[]))
  const categoryMap = computed(() => createCategoryMap(categories.value as PublicCategory[]))
  const hasFilters = computed(() => selectedCategory.value !== null || searchQuery.value.trim() !== '')

  let initializing = true

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
    loading.value = true
    try {
      const params: ProductListParams = { page: currentPage.value, page_size: pageSize.value }
      if (selectedCategory.value) params.category_id = selectedCategory.value
      const keyword = searchQuery.value.trim()
      if (keyword) params.search = keyword
      const res = await productAPI.list(params)
      products.value = Array.isArray(res.data) ? res.data : []
      totalPages.value = res.pagination?.total_page || 0
      total.value = res.pagination?.total ?? products.value.length
    } catch {
      products.value = []
    } finally {
      loading.value = false
    }
  }

  const loadCategories = async () => {
    try {
      const res = await categoryAPI.list()
      categories.value = Array.isArray(res.data) ? res.data : []
    } catch {
      categories.value = []
    }
  }

  const debouncedLoad = debounce(() => void loadProducts(), SEARCH_DEBOUNCE_MS)

  const changePage = (page: number) => {
    if (page < 1 || page > totalPages.value) return
    currentPage.value = page
    debouncedLoad()
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const clearSearch = () => {
    if (!searchQuery.value) return
    searchQuery.value = ''
  }

  const clearFilters = () => {
    searchQuery.value = ''
    selectCategory(null)
  }

  const syncFromRoute = () => {
    if (route.name !== categoryRouteName) {
      if (selectedCategory.value !== null) selectedCategory.value = null
      return false
    }
    const slug = route.params.slug
    if (typeof slug !== 'string' || !slug || categories.value.length === 0) return false
    const matched = categories.value.find((c) => c.slug === slug)
    if (!matched) return false
    if (selectedCategory.value !== matched.id) selectedCategory.value = matched.id
    return true
  }

  watch(selectedCategory, () => {
    if (initializing) return
    currentPage.value = 1
    syncExpanded()
    debouncedLoad()
    if (selectedCategory.value) {
      const matched = categories.value.find((c) => c.id === selectedCategory.value)
      if (matched?.slug && route.params.slug !== matched.slug) {
        void router.replace({ name: categoryRouteName, params: { slug: matched.slug } })
      }
    } else if (route.name === categoryRouteName) {
      void router.replace({ name: homeRouteName })
    }
  })

  watch(searchQuery, () => {
    if (initializing) return
    currentPage.value = 1
    debouncedLoad()
  })

  watch(
    () => route.params.slug,
    () => {
      if (initializing || categories.value.length === 0) return
      syncFromRoute()
    },
  )

  const initialize = async () => {
    await loadCategories()
    if (syncFromRoute()) syncExpanded()
    await loadProducts()
    initializing = false
  }

  onBeforeUnmount(() => debouncedLoad.cancel())

  return {
    loading,
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
