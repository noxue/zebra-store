import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminDashboardInventoryAlert, DashboardOverview, DashboardRankings, DashboardTrends } from '@/api/types'
import { buildDashboardQuery, defaultCustomRange, type DashboardFilters, type DashboardRange } from './dashboardUtils'

export function useDashboard() {
  const t = i18n.global.t
  const loading = ref(false)
  const error = ref('')
  const overview = ref<DashboardOverview | null>(null)
  const trends = ref<DashboardTrends | null>(null)
  const rankings = ref<DashboardRankings | null>(null)
  const inventoryAlerts = ref<AdminDashboardInventoryAlert[]>([])
  const filters = reactive<DashboardFilters>({ range: '7d', from: '', to: '' })
  const tz = Intl.DateTimeFormat().resolvedOptions().timeZone

  const points = computed(() => trends.value?.points ?? [])
  const maxOrder = computed(() => Math.max(1, ...points.value.map((p) => Math.max(p.orders_total, p.orders_paid))))
  const maxPayment = computed(() => Math.max(1, ...points.value.map((p) => Math.max(p.payments_success, p.payments_failed))))
  const funnelSteps = computed(() => {
    const f = overview.value?.funnel
    if (!f) return []
    return [
      { key: 'ordersCreated', value: f.orders_created },
      { key: 'paymentsCreated', value: f.payments_created },
      { key: 'paymentsSuccess', value: f.payments_success },
      { key: 'ordersPaid', value: f.orders_paid },
      { key: 'ordersCompleted', value: f.orders_completed },
    ].map((s) => ({ ...s, label: t(`admin.dashboard.funnel.${s.key}`) }))
  })
  const maxFunnel = computed(() => Math.max(1, ...funnelSteps.value.map((s) => s.value)))

  const load = async (force = false) => {
    error.value = ''
    const params = buildDashboardQuery(filters, tz, force)
    if (!params) {
      error.value = t('admin.dashboard.errors.customRangeRequired')
      return
    }
    loading.value = true
    const [o, tr, r, a] = await Promise.allSettled([
      adminAPI.getDashboardOverview(params),
      adminAPI.getDashboardTrends(params),
      adminAPI.getDashboardRankings(params),
      adminAPI.getDashboardInventoryAlerts(),
    ])
    overview.value = o.status === 'fulfilled' ? o.value.data : null
    trends.value = tr.status === 'fulfilled' ? tr.value.data : null
    rankings.value = r.status === 'fulfilled' ? r.value.data : null
    inventoryAlerts.value = a.status === 'fulfilled' ? (a.value.data ?? []) : []
    if (o.status === 'rejected') error.value = t('admin.dashboard.errors.fetchFailed')
    loading.value = false
  }

  const setRange = (range: DashboardRange) => {
    filters.range = range
    if (range === 'custom') Object.assign(filters, defaultCustomRange())
    else Object.assign(filters, { from: '', to: '' })
    void load()
  }

  return { loading, error, overview, trends, rankings, inventoryAlerts, filters, points, maxOrder, maxPayment, funnelSteps, maxFunnel, load, setRange }
}
