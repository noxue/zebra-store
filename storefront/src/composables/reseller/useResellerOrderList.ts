import { computed, reactive } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ResellerOrderListParams } from '@/api/types'
import { useResellerOrders } from './useResellerOrders'

export interface ResellerOrderFilters {
  order_no: string
  status: string
  created_from: string
  created_to: string
}

/** Converts UI filters ('' / 'all' = unset) to API params. */
export const buildResellerOrderParams = (f: ResellerOrderFilters): ResellerOrderListParams => ({
  order_no: f.order_no.trim() || undefined,
  status: f.status && f.status !== 'all' ? f.status : undefined,
  created_from: f.created_from || undefined,
  created_to: f.created_to || undefined,
})

export const useResellerOrderList = () => {
  const { t, te } = useI18n()
  const orders = useResellerOrders()
  const filters = reactive<ResellerOrderFilters>({ order_no: '', status: 'all', created_from: '', created_to: '' })
  const hasActiveFilter = computed(() => Object.values(buildResellerOrderParams(filters)).some((v) => v !== undefined))
  const statusLabel = (status?: string) => {
    if (!status) return '-'
    const key = `resellerConsole.orders.statusMap.${status}`
    return te(key) ? t(key) : status
  }
  const reload = () => Promise.all([orders.load({ ...buildResellerOrderParams(filters), page: 1 }), orders.loadStats(buildResellerOrderParams(filters))])
  const goPage = (page: number) => orders.load({ ...buildResellerOrderParams(filters), page })
  const resetFilters = () => {
    Object.assign(filters, { order_no: '', status: 'all', created_from: '', created_to: '' })
    void reload()
  }
  const currencyKinds = computed(() => Object.keys(orders.stats.value?.by_currency || {}).length)
  return { ...orders, filters, hasActiveFilter, statusLabel, reload, goPage, resetFilters, currencyKinds }
}
