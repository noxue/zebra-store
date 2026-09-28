import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { resellerAPI } from '@/api/reseller'
import type { ResellerSiteConfigSnapshotData } from '@/api/types'
import { formatResellerConsoleAmount, resellerCurrencyColor, resellerOrderStatusColor } from '@/utils/reseller/console'
import { pickPrimaryResellerBalance } from '@/utils/reseller/finance'
import { useResellerFinance } from './useResellerFinance'
import { useResellerOrders } from './useResellerOrders'
import { useResellerProfileContext } from './useResellerProfile'

export interface DonutSegment {
  value: number
  color: string
  label: string
}

export type SetupItemKey = 'profile' | 'domain' | 'site' | 'products' | 'orders'

export interface SetupItem {
  key: SetupItemKey
  to: string
  label: string
  description: string
  done: boolean
}

export const useResellerDashboard = () => {
  const { t, te } = useI18n()
  const profile = useResellerProfileContext()
  const finance = useResellerFinance()
  const orders = useResellerOrders()
  const setupLoading = ref(false)
  const siteSnapshot = ref<ResellerSiteConfigSnapshotData | null>(null)
  const configuredProductCount = ref(0)
  const initialized = ref(false)
  const setupCollapsed = ref(false)

  const isActive = computed(() => profile.state.value.profileStatus === 'active')
  const loading = computed(
    () => !initialized.value || profile.loading.value || finance.dashboardLoading.value || orders.loading.value || setupLoading.value,
  )

  const statusLabel = (status?: string) => {
    if (!status) return '-'
    const key = `resellerConsole.orders.statusMap.${status}`
    return te(key) ? t(key) : status
  }

  const settlementText = computed(() => {
    const status = profile.snapshot.value?.profile?.settlement_status
    const key = `personalCenter.reseller.settlementStatusMap.${status}`
    return status && te(key) ? t(key) : t('personalCenter.reseller.settlementStatus')
  })

  const benefits = computed(() => [t('resellerConsole.dashboard.benefit1'), t('resellerConsole.dashboard.benefit2'), t('resellerConsole.dashboard.benefit3')])

  const total = computed(() => orders.stats.value?.total || 0)
  const paidCount = computed(() => orders.stats.value?.by_status?.paid || 0)
  const paidRatioText = computed(() => (total.value > 0 ? `${((paidCount.value / total.value) * 100).toFixed(0)}%` : ''))

  const balanceList = computed(() => (finance.balances.value.length ? finance.balances.value : finance.dashboard.value?.balances || []))
  const primaryBalance = computed(() => pickPrimaryResellerBalance(balanceList.value))
  const primaryBalanceText = computed(() =>
    primaryBalance.value ? formatResellerConsoleAmount(primaryBalance.value.available_amount, primaryBalance.value.currency) : '-',
  )
  const primaryBalanceHint = computed(() =>
    balanceList.value.length > 1 ? t('personalCenter.reseller.moreCurrencies', { count: balanceList.value.length - 1 }) : '',
  )

  const statusSegments = computed<DonutSegment[]>(() =>
    Object.entries(orders.stats.value?.by_status || {})
      .filter(([, count]) => count > 0)
      .map(([status, count]) => ({ value: count, color: resellerOrderStatusColor(status), label: statusLabel(status) })),
  )

  const balanceSegments = computed<DonutSegment[]>(() =>
    finance.balances.value
      .map((b, i) => ({ value: Number(b.available_amount) || 0, color: resellerCurrencyColor(i), label: b.currency }))
      .filter((s) => s.value > 0),
  )

  const siteBrandConfigured = computed(() => {
    const cfg = siteSnapshot.value?.config
    return Boolean(cfg?.site_name || cfg?.logo || cfg?.favicon)
  })

  const setupChecklist = computed<SetupItem[]>(() => [
    { key: 'profile', to: '/reseller/apply', label: t('resellerConsole.dashboard.setup.profileEnabled'), description: settlementText.value, done: isActive.value },
    {
      key: 'domain',
      to: '/reseller/domains',
      label: t('resellerConsole.dashboard.setup.primaryDomain'),
      description: profile.primaryDomain.value || t('resellerConsole.dashboard.setup.primaryDomainEmpty'),
      done: Boolean(profile.primaryDomain.value),
    },
    { key: 'site', to: '/reseller/site', label: t('resellerConsole.dashboard.setup.siteBrand'), description: t('resellerConsole.dashboard.setup.siteBrandDescription'), done: siteBrandConfigured.value },
    {
      key: 'products',
      to: '/reseller/products',
      label: t('resellerConsole.dashboard.setup.productRules'),
      description: t('resellerConsole.dashboard.setup.productRulesDescription'),
      done: configuredProductCount.value > 0,
    },
    {
      key: 'orders',
      to: '/reseller/orders',
      label: t('resellerConsole.dashboard.setup.firstOrder'),
      description: total.value
        ? t('resellerConsole.dashboard.setup.firstOrderCount', { count: total.value })
        : t('resellerConsole.dashboard.setup.firstOrderDescription'),
      done: total.value > 0,
    },
  ])
  const completedSetupCount = computed(() => setupChecklist.value.filter((item) => item.done).length)
  const allSetupDone = computed(() => completedSetupCount.value === setupChecklist.value.length)

  const loadSetupState = async () => {
    setupLoading.value = true
    try {
      const [site, products] = await Promise.all([
        resellerAPI.siteConfig(),
        resellerAPI.productSettings({ configured: 'configured', page: 1, page_size: 1 }),
      ])
      siteSnapshot.value = site.data || null
      configuredProductCount.value = Number(products.pagination?.total || 0)
    } catch {
      siteSnapshot.value = null
      configuredProductCount.value = 0
    } finally {
      setupLoading.value = false
    }
  }

  const initialize = async () => {
    await profile.load()
    if (isActive.value) {
      await Promise.all([finance.loadDashboard(), finance.loadBalances(), orders.loadStats(), orders.load({ page: 1, page_size: 5 }), loadSetupState()])
      setupCollapsed.value = allSetupDone.value
    }
    initialized.value = true
  }

  return {
    profile,
    finance,
    orders,
    loading,
    isActive,
    settlementText,
    benefits,
    total,
    paidCount,
    paidRatioText,
    primaryBalanceText,
    primaryBalanceHint,
    statusSegments,
    balanceSegments,
    setupChecklist,
    completedSetupCount,
    allSetupDone,
    setupCollapsed,
    statusLabel,
    initialize,
  }
}
