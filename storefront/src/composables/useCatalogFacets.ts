import { computed, onBeforeUnmount, reactive, ref, watch, type Ref } from 'vue'
import { ApiError } from '@/api/client'
import { catalogFacetAPI } from '@/api/catalog'
import type { CatalogFacetCapabilities, CatalogFacetFilters, CatalogFacetKey, CatalogFacetOption, CatalogFacetOptionsParams } from '@/api/types'
import { CATALOG_FACET_KEYS, facetParam, parseFacetQuery, selectFacet, validFacetId } from '@/utils/catalogFacets'
import { debounce } from '@/utils/debounce'
import { CatalogResponseError, catalogReadError, type CatalogReadError } from '@/utils/catalogReadError'

export interface FacetOptionsState {
  items: CatalogFacetOption[]
  selected: CatalogFacetOption | null
  missingSelection: boolean
  search: string
  page: number
  total: number
  totalPages: number
  loading: boolean
  error: CatalogReadError | null
}
const state = (): FacetOptionsState => ({ items: [], selected: null, missingSelection: false, search: '', page: 0, total: 0, totalPages: 0, loading: false, error: null })
const OPTION_PAGE_SIZE = 20
const OPTION_DEBOUNCE_MS = 300

const LABEL_LOCALES = new Set(['zh-CN', 'zh-TW', 'en-US'])
function validOption(item: CatalogFacetOption): boolean {
  const label = item.label
  if (!validFacetId(item.value_id) || !label || typeof label !== 'object' || Array.isArray(label) || !Number.isSafeInteger(item.product_count) || item.product_count < 1) return false
  const entries = Object.entries(label)
  return entries.some(([, text]) => typeof text === 'string' && text.trim()) && entries.every(([locale, text]) => LABEL_LOCALES.has(locale) && typeof text === 'string' && [...text].length <= 160 && !/[\p{Cc}]/u.test(text))
}

/** Server-paged options with exact URL identity resolution and stale-response protection. */
export function useCatalogFacets(category: Ref<number | null>, search: Ref<string>) {
  const filters = ref<CatalogFacetFilters>({})
  const capabilities = ref<CatalogFacetCapabilities | null>(null)
  const capabilityLoading = ref(false)
  const capabilityError = ref<CatalogReadError | null>(null)
  const routeError = ref(false)
  const initialized = ref(false)
  const states = reactive(Object.fromEntries(CATALOG_FACET_KEYS.map((key) => [key, state()])) as Record<CatalogFacetKey, FacetOptionsState>)
  const versions = Object.fromEntries(CATALOG_FACET_KEYS.map((key) => [key, 0])) as Record<CatalogFacetKey, number>
  const supported = computed(() => capabilities.value?.schema_version === 1 && capabilities.value.supports_product_filters === true)
  const keys = computed(() => supported.value ? CATALOG_FACET_KEYS.filter((key) => capabilities.value?.dimensions.includes(key)) : [])
  const hasSelection = computed(() => Object.keys(filters.value).length > 0)
  const blocked = computed(() => routeError.value || (hasSelection.value && (!supported.value || Object.keys(filters.value).some((name) => !keys.value.some((key) => facetParam(key) === name)))))
  const visibleKeys = computed(() => keys.value.filter((key) => states[key].loading || states[key].error || states[key].total > 0 || Boolean(states[key].search) || Boolean(filters.value[facetParam(key)])))
  let generation = 0
  let capabilityVersion = 0
  let disposed = false

  const invalidate = () => {
    generation += 1
    for (const key of CATALOG_FACET_KEYS) {
      versions[key] += 1
      states[key].selected = null
      states[key].missingSelection = false
      states[key].items = []
      states[key].page = 0
      states[key].total = 0
      states[key].totalPages = 0
      states[key].error = null
      states[key].loading = !disposed && initialized.value && supported.value && !routeError.value && keys.value.includes(key)
    }
  }
  const scope = (key: CatalogFacetKey) => {
    // The customer may change a parent directly; its old descendants must not
    // hide otherwise valid parent options. Product listing still uses all filters.
    const allowed = new Set(CATALOG_FACET_KEYS.slice(0, CATALOG_FACET_KEYS.indexOf(key) + 1).map(facetParam))
    const parents = Object.fromEntries(Object.entries(filters.value).filter(([name]) => allowed.has(name as `facet_${CatalogFacetKey}`)))
    return { category_id: category.value ?? undefined, search: search.value.trim() || undefined, ...parents }
  }
  const fetchOptions = async (key: CatalogFacetKey, append = false) => {
    if (!supported.value || !keys.value.includes(key) || disposed) return
    const currentGeneration = generation
    const version = ++versions[key]
    const current = () => !disposed && currentGeneration === generation && version === versions[key]
    const target = states[key]
    const page = append ? target.page + 1 : 1
    const selectedId = filters.value[facetParam(key)]
    const params: CatalogFacetOptionsParams = { ...scope(key), dimension: key, page, page_size: OPTION_PAGE_SIZE, option_search: target.search || undefined }
    target.loading = true
    target.error = null
    target.missingSelection = false
    try {
      const response = await catalogFacetAPI.values(params)
      if (!current()) return
      if (!Array.isArray(response.data) || !response.pagination) throw new CatalogResponseError()
      const pagination = response.pagination
      if (pagination.page !== page || pagination.page_size !== OPTION_PAGE_SIZE || !Number.isSafeInteger(pagination.total) || pagination.total < 0 || !Number.isSafeInteger(pagination.total_page) || pagination.total_page < 0) throw new CatalogResponseError()
      const items = response.data
      if (items.some((item) => !validOption(item))) throw new CatalogResponseError()
      let selected = selectedId ? items.find((item) => item.value_id === selectedId) ?? null : null
      let missing = false
      if (selectedId && !selected) {
        // Search text and pagination must not hide the customer's exact selected value.
        const exact = await catalogFacetAPI.values({ ...scope(key), dimension: key, value_id: selectedId, page: 1, page_size: 1 })
        if (!current()) return
        if (!Array.isArray(exact.data) || exact.data.length > 1 || exact.data.some((item) => item.value_id !== selectedId || !validOption(item))) throw new CatalogResponseError()
        selected = exact.data[0] ?? null
        missing = selected === null
      }
      if (!current()) return
      const merged = new Map((append ? target.items : []).map((item) => [item.value_id, item]))
      for (const item of items) merged.set(item.value_id, item)
      target.items = [...merged.values()]
      target.selected = selected
      target.missingSelection = missing
      target.page = response.pagination.page
      target.total = response.pagination.total
      target.totalPages = response.pagination.total_page
    } catch (error) {
      if (current()) target.error = catalogReadError(error)
    } finally {
      if (current()) target.loading = false
    }
  }
  const searches = Object.fromEntries(CATALOG_FACET_KEYS.map((key) => [key, debounce(() => void fetchOptions(key), OPTION_DEBOUNCE_MS)])) as Record<CatalogFacetKey, ReturnType<typeof debounce>>
  const reload = async () => {
    if (!supported.value || disposed || routeError.value) return
    for (const key of CATALOG_FACET_KEYS) searches[key].cancel()
    await Promise.all(keys.value.map((key) => fetchOptions(key)))
  }
  const debouncedReload = debounce(() => void reload(), OPTION_DEBOUNCE_MS)

  const loadCapabilities = async () => {
    if (disposed) return
    const version = ++capabilityVersion
    capabilityLoading.value = true
    capabilityError.value = null
    try {
      const response = await catalogFacetAPI.capabilities()
      if (disposed || version !== capabilityVersion) return
      const value = response.data
      if (!value || value.schema_version !== 1 || value.revision_format !== 'decimal_string' || !Array.isArray(value.dimensions) || value.supports_product_filters !== true || value.supports_value_search !== true || value.dimensions.some((key) => typeof key !== 'string')) throw new CatalogResponseError()
      capabilities.value = value
    } catch (error) {
      if (disposed || version !== capabilityVersion) return
      capabilities.value = null
      // A genuinely older Store has no optional browse API. A failed request is not absence.
      capabilityError.value = error instanceof ApiError && (error.httpStatus === 404 || error.statusCode === 404) ? null : catalogReadError(error)
    } finally {
      if (!disposed && version === capabilityVersion) capabilityLoading.value = false
    }
  }
  const clearOptionSearches = (keys: CatalogFacetKey[]) => {
    for (const key of keys) { states[key].search = ''; searches[key].cancel() }
  }
  const restore = (query: Record<string, unknown>) => {
    clearOptionSearches(CATALOG_FACET_KEYS)
    const parsed = parseFacetQuery(query)
    routeError.value = !parsed.ok
    filters.value = parsed.ok ? parsed.filters : {}
  }
  const choose = (key: CatalogFacetKey, value: string | null) => {
    if (value !== null && !validFacetId(value)) { routeError.value = true; return }
    routeError.value = false
    clearOptionSearches(CATALOG_FACET_KEYS.slice(CATALOG_FACET_KEYS.indexOf(key) + 1))
    filters.value = selectFacet(filters.value, key, value)
  }
  const clear = () => { routeError.value = false; clearOptionSearches(CATALOG_FACET_KEYS); filters.value = {} }
  const optionSearch = (key: CatalogFacetKey, value: string) => {
    if (!supported.value || disposed) return
    versions[key] += 1
    states[key].search = [...value].slice(0, 160).join('')
    states[key].loading = true
    searches[key]()
  }
  const initialize = async (waitForOptions = true) => {
    await loadCapabilities()
    if (disposed) return
    initialized.value = true
    const options = reload()
    if (waitForOptions) await options
  }
  const retryCapabilities = async () => { await loadCapabilities(); await reload() }

  watch([category, search, filters], ([nextCategory, nextSearch], [oldCategory, oldSearch]) => {
    if (nextCategory !== oldCategory || nextSearch !== oldSearch) clearOptionSearches(CATALOG_FACET_KEYS)
    invalidate()
    debouncedReload.cancel()
    for (const key of CATALOG_FACET_KEYS) searches[key].cancel()
    if (initialized.value && !routeError.value) debouncedReload()
  }, { deep: true, flush: 'sync' })
  onBeforeUnmount(() => {
    disposed = true
    invalidate()
    capabilityVersion += 1
    debouncedReload.cancel()
    for (const key of CATALOG_FACET_KEYS) searches[key].cancel()
  })
  return { filters, states, supported, keys, visibleKeys, hasSelection, blocked, initialized, capabilityLoading, capabilityError, routeError, choose, clear, restore, initialize, reload, fetchOptions, optionSearch, retryCapabilities }
}
