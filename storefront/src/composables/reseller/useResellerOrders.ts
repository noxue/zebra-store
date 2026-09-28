import { reactive, ref } from 'vue'
import type { Pagination } from '@/api/client'
import { resellerAPI } from '@/api/reseller'
import type { ResellerOrderData, ResellerOrderDetailData, ResellerOrderListParams, ResellerOrderStatsData } from '@/api/types'

export const useResellerOrders = () => {
  const loading = ref(false)
  const detailLoading = ref(false)
  const error = ref('')
  const detailError = ref('')
  const rows = ref<ResellerOrderData[]>([])
  const detail = ref<ResellerOrderDetailData | null>(null)
  const stats = ref<ResellerOrderStatsData | null>(null)
  const pagination = reactive<Pagination>({ page: 1, page_size: 20, total: 0, total_page: 1 })

  const load = async (params: ResellerOrderListParams = {}) => {
    loading.value = true
    error.value = ''
    try {
      const res = await resellerAPI.orders({ page: pagination.page, page_size: pagination.page_size, ...params })
      rows.value = res.data || []
      if (res.pagination) Object.assign(pagination, res.pagination)
    } catch (err) {
      rows.value = []
      error.value = err instanceof Error && err.message ? err.message : 'error'
    } finally {
      loading.value = false
    }
  }

  const loadStats = async (params: ResellerOrderListParams = {}) => {
    try {
      stats.value = (await resellerAPI.orderStats(params)).data || null
    } catch {
      stats.value = null
    }
  }

  const loadDetail = async (orderNo: string) => {
    detailLoading.value = true
    detailError.value = ''
    try {
      detail.value = (await resellerAPI.orderDetail(orderNo)).data || null
    } catch (err) {
      detail.value = null
      detailError.value = err instanceof Error && err.message ? err.message : 'error'
    } finally {
      detailLoading.value = false
    }
  }

  return { loading, detailLoading, error, detailError, rows, detail, stats, pagination, load, loadStats, loadDetail }
}
