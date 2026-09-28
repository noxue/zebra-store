import { reactive, ref, type Ref } from 'vue'
import type { ApiResponse, Pagination } from '@/api/client'

/** Generic paginated loader around an API list call. */
export function usePagedList<T, P extends object = Record<string, never>>(
  fetcher: (params: { page: number; page_size: number } & Partial<P>) => Promise<ApiResponse<T[]>>,
  pageSize = 20,
) {
  const rows = ref<T[]>([]) as Ref<T[]>
  const loading = ref(false)
  const loaded = ref(false)
  const pagination = reactive<Pagination>({ page: 1, page_size: pageSize, total: 0, total_page: 1 })
  let seq = 0

  const load = async (page = 1, extra?: Partial<P>) => {
    const current = ++seq
    loading.value = true
    try {
      const res = await fetcher({ page, page_size: pagination.page_size, ...(extra || {}) } as { page: number; page_size: number } & Partial<P>)
      if (current !== seq) return
      rows.value = Array.isArray(res.data) ? res.data : []
      Object.assign(pagination, res.pagination || { page, total: rows.value.length, total_page: 1 })
      loaded.value = true
    } catch {
      if (current !== seq) return
      rows.value = []
    } finally {
      if (current === seq) loading.value = false
    }
  }

  return { rows, loading, loaded, pagination, load }
}
