import { ref, shallowRef } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminProduct } from '@/api/types'
import { ensureProductsInOptions } from './marketingUtils'

/**
 * Product picker options for coupons/promotions: loads every page (max 20 × 100) of
 * `getProducts({search})`, dedups, and keeps referenced ids present as placeholders.
 */
export function useProductOptions(referencedIds: () => number[]) {
  const products = shallowRef<AdminProduct[]>([])
  const loading = ref(false)
  let seq = 0

  const load = async (keyword = '') => {
    const current = ++seq
    loading.value = true
    try {
      const rows: AdminProduct[] = []
      let page = 1
      let totalPage = 1
      do {
        const res = await adminAPI.getProducts({ page, page_size: 100, ...(keyword.trim() ? { search: keyword.trim() } : {}) })
        rows.push(...(Array.isArray(res.data) ? res.data : []))
        totalPage = Number(res.pagination?.total_page || 1)
        page += 1
      } while (page <= totalPage && page <= 20)
      const dedup = new Map<number, AdminProduct>()
      rows.forEach((item) => {
        const id = Number(item?.id || 0)
        if (id > 0 && !dedup.has(id)) dedup.set(id, item)
      })
      if (current === seq) products.value = ensureProductsInOptions(Array.from(dedup.values()), referencedIds())
    } catch {
      if (current === seq) products.value = ensureProductsInOptions([], referencedIds())
    } finally {
      if (current === seq) loading.value = false
    }
  }

  return { products, loading, load }
}
