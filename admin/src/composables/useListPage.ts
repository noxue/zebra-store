import { ref, shallowRef, type Ref } from 'vue'
import type { ApiResponse, Pagination } from '@/api/types'

export type { Pagination }

export interface ListResult<T> {
  items: T[]
  pagination?: Pagination
}

export type ListFetchFn<T> = (page: number, pageSize: number) => Promise<ListResult<T> | ApiResponse<T[] | null>>

export interface UseListPageOptions<T> {
  /** Fetch one page. May return the raw envelope (`{data, pagination}`) or `{items, pagination}`. */
  fetchFn: ListFetchFn<T>
  pageSize?: number
  debounceMs?: number
}

export const PAGE_SIZE_OPTIONS = [10, 20, 50, 100] as const

function isEnvelope<T>(value: ListResult<T> | ApiResponse<T[] | null>): value is ApiResponse<T[] | null> {
  return 'status_code' in value || 'data' in value
}

export function normalizeListResult<T>(value: ListResult<T> | ApiResponse<T[] | null>): ListResult<T> {
  if (isEnvelope(value)) {
    return { items: Array.isArray(value.data) ? value.data : [], pagination: value.pagination }
  }
  return value
}

/** Clamp a jump-to-page input into [1, totalPage]; null when invalid or unchanged. */
export function resolveJumpTarget(raw: string | number, current: number, totalPage: number): number | null {
  if (raw === '' || raw === null || raw === undefined) return null
  const n = Number(raw)
  if (Number.isNaN(n)) return null
  const target = Math.min(Math.max(Math.floor(n), 1), Math.max(totalPage, 1))
  return target === current ? null : target
}

export function useListPage<T>(options: UseListPageOptions<T>) {
  const { fetchFn, debounceMs = 300 } = options
  const loading = ref(false)
  const items = shallowRef<T[]>([]) as Ref<T[]>
  const pagination = ref<Pagination>({ page: 1, page_size: options.pageSize ?? 20, total: 0, total_page: 1 })
  const jumpPage = ref('')
  let seq = 0
  let timer: ReturnType<typeof setTimeout> | undefined

  const fetchData = async (page = 1) => {
    const current = ++seq
    loading.value = true
    try {
      const result = normalizeListResult(await fetchFn(page, pagination.value.page_size))
      if (current !== seq) return
      items.value = result.items
      pagination.value = result.pagination
        ? { ...result.pagination, total_page: Math.max(result.pagination.total_page || 1, 1) }
        : { page, page_size: pagination.value.page_size, total: result.items.length, total_page: 1 }
    } catch {
      if (current === seq) items.value = []
    } finally {
      if (current === seq) loading.value = false
    }
  }

  const handleSearch = () => fetchData(1)
  const debouncedSearch = () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(handleSearch, debounceMs)
  }
  const refresh = () => fetchData(pagination.value.page)
  const changePage = (page: number) => {
    if (page < 1 || page > pagination.value.total_page) return
    void fetchData(page)
  }
  const changePageSize = (size: number) => {
    pagination.value = { ...pagination.value, page_size: size }
    void fetchData(1)
  }
  const jumpToPage = () => {
    const target = resolveJumpTarget(jumpPage.value, pagination.value.page, pagination.value.total_page)
    if (target !== null) changePage(target)
  }

  return {
    loading,
    items,
    pagination,
    jumpPage,
    fetchData,
    handleSearch,
    debouncedSearch,
    refresh,
    changePage,
    changePageSize,
    jumpToPage,
  }
}

export type ListPage<T> = ReturnType<typeof useListPage<T>>
