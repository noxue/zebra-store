import { computed, reactive } from 'vue'
import type { ResellerLedgerParams } from '@/api/reseller'
import { useResellerFinance } from './useResellerFinance'

export interface ResellerLedgerFilters {
  type: string
  status: string
  order_id: string
}

export const buildResellerLedgerParams = (f: ResellerLedgerFilters): ResellerLedgerParams => {
  const orderId = Number(f.order_id)
  return {
    type: f.type && f.type !== 'all' ? f.type : undefined,
    status: f.status && f.status !== 'all' ? f.status : undefined,
    order_id: Number.isFinite(orderId) && orderId > 0 ? Math.floor(orderId) : undefined,
  }
}

export const useResellerLedger = () => {
  const finance = useResellerFinance()
  const filters = reactive<ResellerLedgerFilters>({ type: 'all', status: 'all', order_id: '' })
  const hasActiveFilter = computed(() => Object.values(buildResellerLedgerParams(filters)).some((v) => v !== undefined))
  const reload = () => finance.loadLedgerEntries({ ...buildResellerLedgerParams(filters), page: 1 })
  const goPage = (page: number) => finance.loadLedgerEntries({ ...buildResellerLedgerParams(filters), page })
  const resetFilters = () => {
    Object.assign(filters, { type: 'all', status: 'all', order_id: '' })
    void reload()
  }
  return { ...finance, filters, hasActiveFilter, reload, goPage, resetFilters }
}
