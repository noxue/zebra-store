import { computed, reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminResellerOperationsFinance, AdminResellerOperationsOverview } from '@/api/types'
import i18n from '@/i18n'
import { useAdminAuthStore } from '@/stores/auth'
import { toRFC3339 } from '@/utils/format'
import { hasFinancePermission, normalizeCurrencyRows } from '@/utils/resellerOperations'

export type OperationsRange = 'today' | '7d' | '30d' | 'custom'

export function useResellerOperations() {
  const t = i18n.global.t
  const auth = useAdminAuthStore()
  const overview = ref<AdminResellerOperationsOverview | null>(null)
  const finance = ref<AdminResellerOperationsFinance | null>(null)
  const loadingOverview = ref(false)
  const loadingFinance = ref(false)
  const pageError = ref('')
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai'
  const filters = reactive<{
    range: OperationsRange
    from: string
    to: string
  }>({ range: '7d', from: '', to: '' })

  const canViewFinance = computed(() => hasFinancePermission((p) => auth.hasPermission(p)))
  const periodRows = computed(() => normalizeCurrencyRows(finance.value?.period_currency_rows))
  const currentRows = computed(() => normalizeCurrencyRows(finance.value?.current_currency_rows))

  const buildParams = () => {
    const params: Record<string, string> = {
      range: filters.range,
      tz: timezone,
    }
    const from = toRFC3339(filters.from)
    const to = toRFC3339(filters.to)
    if (from) params.from = from
    if (to) params.to = to
    return params
  }

  const loadOverview = async () => {
    loadingOverview.value = true
    try {
      overview.value = (await adminAPI.getResellerOperationsOverview(buildParams())).data ?? null
    } catch (err) {
      pageError.value = err instanceof Error && err.message ? err.message : t('admin.resellerOperations.errors.loadFailed')
      overview.value = null
    } finally {
      loadingOverview.value = false
    }
  }

  const loadFinance = async () => {
    if (!canViewFinance.value) {
      finance.value = null
      return
    }
    loadingFinance.value = true
    try {
      finance.value = (await adminAPI.getResellerOperationsFinance(buildParams())).data ?? null
    } catch (err) {
      pageError.value = err instanceof Error && err.message ? err.message : t('admin.resellerOperations.errors.loadFailed')
      finance.value = null
    } finally {
      loadingFinance.value = false
    }
  }

  const loadAll = async () => {
    if (filters.range === 'custom' && !(toRFC3339(filters.from) && toRFC3339(filters.to))) {
      pageError.value = t('admin.resellerOperations.errors.customRangeRequired')
      return
    }
    pageError.value = ''
    await loadOverview()
    await loadFinance()
  }

  return {
    overview,
    finance,
    loadingOverview,
    loadingFinance,
    pageError,
    filters,
    canViewFinance,
    periodRows,
    currentRows,
    loadAll,
  }
}
