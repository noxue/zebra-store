import { reactive, ref } from 'vue'
import type { Pagination } from '@/api/client'
import { resellerAPI, type ResellerLedgerParams } from '@/api/reseller'
import type { ResellerBalanceData, ResellerDashboardData, ResellerLedgerData, ResellerWithdrawApplyPayload, ResellerWithdrawData } from '@/api/types'

/**
 * Dashboard / balances / ledger / withdraws. Each loader swallows its own
 * errors so one failing section degrades to empty without breaking the page.
 */
export const useResellerFinance = () => {
  const dashboardLoading = ref(false)
  const balanceLoading = ref(false)
  const ledgerLoading = ref(false)
  const withdrawsLoading = ref(false)
  const submittingWithdraw = ref(false)
  const dashboard = ref<ResellerDashboardData | null>(null)
  const balances = ref<ResellerBalanceData[]>([])
  const ledgerEntries = ref<ResellerLedgerData[]>([])
  const withdraws = ref<ResellerWithdrawData[]>([])
  const ledgerPagination = reactive<Pagination>({ page: 1, page_size: 20, total: 0, total_page: 1 })
  const withdrawsPagination = reactive<Pagination>({ page: 1, page_size: 20, total: 0, total_page: 1 })

  const loadDashboard = async () => {
    dashboardLoading.value = true
    try {
      dashboard.value = (await resellerAPI.dashboard()).data || null
    } catch {
      dashboard.value = null
    } finally {
      dashboardLoading.value = false
    }
  }

  const loadBalances = async () => {
    balanceLoading.value = true
    try {
      balances.value = (await resellerAPI.balanceAccounts()).data || []
    } catch {
      balances.value = []
    } finally {
      balanceLoading.value = false
    }
  }

  const loadLedgerEntries = async (params: ResellerLedgerParams = {}) => {
    ledgerLoading.value = true
    try {
      const res = await resellerAPI.ledgerEntries({ page: ledgerPagination.page, page_size: ledgerPagination.page_size, ...params })
      ledgerEntries.value = res.data || []
      if (res.pagination) Object.assign(ledgerPagination, res.pagination)
    } catch {
      ledgerEntries.value = []
    } finally {
      ledgerLoading.value = false
    }
  }

  const loadWithdraws = async (params: { page?: number } = {}) => {
    withdrawsLoading.value = true
    try {
      const res = await resellerAPI.withdraws({ page: withdrawsPagination.page, page_size: withdrawsPagination.page_size, ...params })
      withdraws.value = res.data || []
      if (res.pagination) Object.assign(withdrawsPagination, res.pagination)
    } catch {
      withdraws.value = []
    } finally {
      withdrawsLoading.value = false
    }
  }

  /** Throws on failure; refreshes are best effort (allSettled avoids duplicate submits). */
  const applyWithdraw = async (payload: ResellerWithdrawApplyPayload) => {
    submittingWithdraw.value = true
    try {
      await resellerAPI.applyWithdraw(payload)
      await Promise.allSettled([loadDashboard(), loadBalances(), loadWithdraws({ page: 1 })])
    } finally {
      submittingWithdraw.value = false
    }
  }

  return {
    dashboardLoading,
    balanceLoading,
    ledgerLoading,
    withdrawsLoading,
    submittingWithdraw,
    dashboard,
    balances,
    ledgerEntries,
    withdraws,
    ledgerPagination,
    withdrawsPagination,
    loadDashboard,
    loadBalances,
    loadLedgerEntries,
    loadWithdraws,
    applyWithdraw,
  }
}
