import { computed, defineComponent, onMounted } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import {
  AlertTriangle,
  Boxes,
  CalendarDays,
  CircleDollarSign,
  Clock,
  CreditCard,
  KeyRound,
  Percent,
  PiggyBank,
  ReceiptText,
  ShoppingBag,
  TrendingUp,
  UserPlus,
  Wallet,
  Zap,
} from 'lucide-vue-next'
import { Badge, Button, Card, DateTimeInput, EmptyState, Mascot, PageHeader, Select, StatCard } from '@/components/ui'
import { formatMoney, formatPercent, getLocalizedText } from '@/utils/format'
import { useAdminAuthStore } from '@/stores/auth'
import { formatSkuDisplayLabel } from '@/utils/sku'
import { useDashboard } from './dashboard/useDashboard'
import { TrendBars } from './dashboard/TrendBars'
import { barPercent, shortDate, visibleQuickActions, type DashboardRange } from './dashboard/dashboardUtils'

export default defineComponent({
  name: 'DashboardView',
  setup() {
    const { t, te, locale } = useI18n()
    const d = useDashboard()
    onMounted(() => void d.load())

    const currency = computed(() => d.overview.value?.currency || 'CNY')
    const alertLabel = (type: string) => (te(`admin.dashboard.alertTypes.${type}`) ? t(`admin.dashboard.alertTypes.${type}`) : type)

    const auth = useAdminAuthStore()
    const quickIcons = {
      orders: ShoppingBag,
      payments: CreditCard,
      products: Boxes,
      cardSecrets: KeyRound,
      users: UserPlus,
    }
    const quickActions = computed(() =>
      visibleQuickActions((p) => auth.hasPermission(p)).map((a) => ({
        label: t(a.labelKey),
        path: a.path,
        icon: quickIcons[a.key],
      })),
    )

    const renderKpis = () => {
      const k = d.overview.value?.kpi
      if (!k) return null
      const c = currency.value
      return (
        <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <StatCard label={t('admin.dashboard.kpi.ordersTotal')} value={k.orders_total} icon={ShoppingBag} tone="primary">
            <p>
              {t('admin.dashboard.kpi.paidOrders')}: {k.paid_orders}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.gmvPaid')} value={formatMoney(k.gmv_paid, c)} icon={CircleDollarSign} tone="gold">
            <p>
              {t('admin.dashboard.kpi.paymentSuccessRate')}: {formatPercent(k.payment_success_rate)}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.totalCost')} value={formatMoney(k.total_cost, c)} icon={ReceiptText} tone="secondary">
            <p>
              {t('admin.dashboard.kpi.paymentFee')}: {formatMoney(k.payment_fee, c)}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.totalProfit')} value={formatMoney(k.total_profit, c)} icon={PiggyBank} tone="success" />
          <StatCard label={t('admin.dashboard.kpi.profitMargin')} value={formatPercent(k.profit_margin)} icon={Percent} tone="accent" />
          <StatCard label={t('admin.dashboard.kpi.pendingOrders')} value={k.pending_payment_orders} icon={Clock} tone="gold">
            <p>
              {t('admin.dashboard.kpi.processingOrders')}: {k.processing_orders}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.newUsers')} value={k.new_users} icon={UserPlus} tone="primary">
            <p>
              {t('admin.dashboard.kpi.activeProducts')}: {k.active_products}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.totalUserBalance')} value={formatMoney(k.total_user_balance, c)} icon={Wallet} tone="secondary" />
          <StatCard label={t('admin.dashboard.kpi.lowStockProducts')} value={k.low_stock_products} icon={AlertTriangle} tone="danger">
            <p>
              {t('admin.dashboard.kpi.outOfStockProducts')}: {k.out_of_stock_products}
            </p>
            <p>
              {t('admin.dashboard.kpi.outOfStockSKUs')}: {k.out_of_stock_skus} / {t('admin.dashboard.kpi.lowStockSKUs')}: {k.low_stock_skus}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.autoAvailableSecrets')} value={k.auto_available_secrets} icon={KeyRound} tone="accent">
            <p>
              {t('admin.dashboard.kpi.manualAvailableUnits')}: {k.manual_available_units}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.kpi.paymentsSuccess')} value={k.payments_success} icon={Zap} tone="success">
            <p>
              {t('admin.dashboard.kpi.paymentsFailed')}: {k.payments_failed}
            </p>
          </StatCard>
          <StatCard label={t('admin.dashboard.period')} icon={CalendarDays} tone="secondary">
            {{
              value: () => (
                <span class="text-lg">
                  {shortDate(d.overview.value?.from?.slice(0, 10))} - {shortDate(d.overview.value?.to?.slice(0, 10))}
                </span>
              ),
              default: () => <p>{d.overview.value?.timezone === 'Local' ? Intl.DateTimeFormat().resolvedOptions().timeZone : d.overview.value?.timezone}</p>,
            }}
          </StatCard>
        </div>
      )
    }

    const renderTrends = () => {
      const pts = d.points.value
      const dates = pts.map((p) => p.date)
      return (
        <div class="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <Card title={t('admin.dashboard.trends.orderTitle')}>
            <TrendBars
              dates={dates}
              max={d.maxOrder.value}
              emptyText={t('admin.dashboard.emptyTrend')}
              footer={pts.map((p) => formatMoney(p.gmv_paid, currency.value))}
              series={[
                {
                  label: t('admin.dashboard.trends.ordersTotal'),
                  color: 'var(--zs-secondary)',
                  values: pts.map((p) => p.orders_total),
                },
                {
                  label: t('admin.dashboard.trends.ordersPaid'),
                  color: 'var(--zs-primary)',
                  values: pts.map((p) => p.orders_paid),
                },
              ]}
            />
          </Card>
          <Card title={t('admin.dashboard.trends.paymentTitle')}>
            <TrendBars
              dates={dates}
              max={d.maxPayment.value}
              emptyText={t('admin.dashboard.emptyTrend')}
              series={[
                {
                  label: t('admin.dashboard.trends.paymentsSuccess'),
                  color: 'var(--zs-accent)',
                  values: pts.map((p) => p.payments_success),
                },
                {
                  label: t('admin.dashboard.trends.paymentsFailed'),
                  color: 'var(--zs-danger)',
                  values: pts.map((p) => p.payments_failed),
                },
              ]}
            />
          </Card>
        </div>
      )
    }

    const renderFunnelAndRankings = () => {
      const f = d.overview.value?.funnel
      const products = d.rankings.value?.top_products ?? []
      const channels = d.rankings.value?.top_channels ?? []
      const c = currency.value
      return (
        <div class="grid grid-cols-1 gap-4 xl:grid-cols-3">
          <Card title={t('admin.dashboard.funnel.title')}>
            <div class="space-y-3">
              {d.funnelSteps.value.map((s) => (
                <div key={s.key}>
                  <div class="mb-1 flex justify-between text-xs">
                    <span class="text-muted">{s.label}</span>
                    <span class="zs-num font-semibold">{s.value}</span>
                  </div>
                  <div class="h-2.5 overflow-hidden rounded-full bg-surface-muted">
                    <div
                      class="zs-gradient-bg h-full rounded-full transition-all duration-700"
                      style={{
                        width: `${barPercent(s.value, d.maxFunnel.value)}%`,
                      }}
                    />
                  </div>
                </div>
              ))}
              {f && (
                <div class="grid grid-cols-2 gap-2 pt-1">
                  <div class="rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
                    <p class="text-[11px] text-muted">{t('admin.dashboard.funnel.paymentConversionRate')}</p>
                    <p class="zs-num font-semibold">{formatPercent(f.payment_conversion_rate)}</p>
                  </div>
                  <div class="rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
                    <p class="text-[11px] text-muted">{t('admin.dashboard.funnel.completionRate')}</p>
                    <p class="zs-num font-semibold">{formatPercent(f.completion_rate)}</p>
                  </div>
                </div>
              )}
            </div>
          </Card>
          <Card title={t('admin.dashboard.rankings.topProductsTitle')}>
            {products.length === 0 ? (
              <EmptyState title={t('admin.dashboard.rankings.empty')} size={72} />
            ) : (
              <ol class="space-y-2">
                {products.map((p, i) => {
                  const sku = formatSkuDisplayLabel({ sku_code: p.sku_code, spec_values: p.sku_spec_values }, locale.value)
                  return (
                    <li key={`${p.product_id}-${p.sku_id ?? 0}`} class="rounded-zs-sm border border-line bg-surface-strong p-3">
                      <div class="flex items-start gap-2">
                        <span
                          class={[
                            'zs-num flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-xs font-bold',
                            i < 3 ? 'zs-gradient-bg text-white' : 'bg-surface-muted text-muted',
                          ]}
                        >
                          {i + 1}
                        </span>
                        <div class="min-w-0 flex-1">
                          <p class="truncate text-sm font-medium">{p.title}</p>
                          {sku && (
                            <p class="text-[11px] text-muted">
                              {t('admin.dashboard.rankings.skuLabel')}: {sku}
                            </p>
                          )}
                          <div class="mt-1 grid grid-cols-2 gap-x-3 text-[11px] text-muted">
                            <span>
                              {t('admin.dashboard.rankings.paidOrders')}: {p.paid_orders}
                            </span>
                            <span>
                              {t('admin.dashboard.rankings.quantity')}: {p.quantity}
                            </span>
                            <span>
                              {t('admin.dashboard.rankings.paidAmount')}: <b class="text-fg">{formatMoney(p.paid_amount, c)}</b>
                            </span>
                            <span>
                              {t('admin.dashboard.rankings.profit')}: {formatMoney(p.profit, c)}
                            </span>
                          </div>
                        </div>
                      </div>
                    </li>
                  )
                })}
              </ol>
            )}
          </Card>
          <Card title={t('admin.dashboard.rankings.topChannelsTitle')}>
            {channels.length === 0 ? (
              <EmptyState title={t('admin.dashboard.rankings.empty')} size={72} />
            ) : (
              <ol class="space-y-2">
                {channels.map((ch) => (
                  <li key={ch.channel_id} class="rounded-zs-sm border border-line bg-surface-strong p-3">
                    <p class="text-sm font-medium">{ch.channel_name || `${ch.provider_type || '-'} / ${ch.channel_type || '-'}`}</p>
                    <div class="mt-1 grid grid-cols-2 gap-x-3 text-[11px] text-muted">
                      <span>
                        {t('admin.dashboard.rankings.successCount')}: {ch.success_count}
                      </span>
                      <span class="text-right">
                        {t('admin.dashboard.rankings.failedCount')}: {ch.failed_count}
                      </span>
                      <span>
                        {t('admin.dashboard.rankings.successAmount')}: <b class="text-fg">{formatMoney(ch.success_amount, c)}</b>
                      </span>
                      <span class="text-right">
                        {t('admin.dashboard.rankings.successRate')}: {formatPercent(ch.success_rate)}
                      </span>
                    </div>
                  </li>
                ))}
              </ol>
            )}
          </Card>
        </div>
      )
    }

    const renderAlertsAndActions = () => {
      const alerts = d.overview.value?.alerts ?? []
      const inv = d.inventoryAlerts.value
      const converterAlerts = d.cardConverterAlerts.value
      const pendingConverters = d.cardConverterPending.value
      const canViewOrders = auth.hasPermission('GET:/admin/orders')
      const canViewProcurement = auth.hasPermission('GET:/admin/procurement-orders')
      return (
        <div class="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <Card title={t('admin.dashboard.alerts.title')}>
            {alerts.length === 0 && inv.length === 0 && converterAlerts.length === 0 && pendingConverters.total === 0 ? (
              <EmptyState title={t('admin.dashboard.alerts.empty')} mood="happy" size={72} />
            ) : (
              <div class="space-y-3">
                {pendingConverters.total > 0 && (
                  <div class="rounded-zs-sm border border-warning/40 bg-warning-soft px-3 py-2 text-sm text-warning-text">
                    <div class="mb-2 flex items-center justify-between gap-3">
                      <span class="font-semibold">{t('admin.dashboard.alerts.converterPending', { count: pendingConverters.total })}</span>
                      <Badge tone="warning">{pendingConverters.total}</Badge>
                    </div>
                    <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                      {canViewOrders && <RouterLink class="underline underline-offset-2" to="/orders">{t('admin.dashboard.alerts.converterPendingLocal', { count: pendingConverters.local })}</RouterLink>}
                      {canViewProcurement && <RouterLink class="underline underline-offset-2" to="/procurement-orders">{t('admin.dashboard.alerts.converterPendingUpstream', { count: pendingConverters.upstream })}</RouterLink>}
                    </div>
                  </div>
                )}
                {alerts.map((a) => (
                  <div
                    key={a.type}
                    class={[
                      'flex items-center justify-between rounded-zs-sm border px-3 py-2 text-sm',
                      a.level === 'error'
                        ? 'border-danger/40 bg-danger-soft text-danger-text'
                        : a.level === 'warning'
                          ? 'border-warning/40 bg-warning-soft text-warning-text'
                          : 'border-line bg-surface-muted',
                    ]}
                  >
                    <span>{alertLabel(a.type)}</span>
                    <span class="zs-num font-semibold">{a.value}</span>
                  </div>
                ))}
                {converterAlerts.map((converter) => {
                  const products = new Set(d.cardConverterBindings.value.filter((binding) => binding.converter_id === converter.id).map((binding) => binding.product_id))
                  return (
                    <RouterLink
                      key={`converter-${converter.id}`}
                      to="/card-converters"
                      class="block rounded-zs-sm border border-danger/40 bg-danger-soft px-3 py-2 text-sm text-danger-text transition-colors hover:border-danger"
                    >
                      <div class="flex items-center justify-between gap-3">
                        <span class="font-semibold">{t('admin.dashboard.alerts.converterUnhealthy', { name: converter.name })}</span>
                        <Badge tone="danger">{t('admin.dashboard.alerts.affectedProducts', { count: products.size })}</Badge>
                      </div>
                      <p class="mt-1 text-xs">{converter.last_error || t('admin.dashboard.alerts.converterProbeFailed')}</p>
                      {converter.last_checked_at && <p class="mt-1 text-[11px] opacity-75">{converter.last_checked_at}</p>}
                    </RouterLink>
                  )
                })}
                {inv.length > 0 && (
                  <div class="space-y-2">
                    <p class="text-xs text-muted">{t('admin.dashboard.inventoryAlerts.title')}</p>
                    {inv.map((item) => {
                      const sku = formatSkuDisplayLabel(
                        {
                          sku_code: item.sku_code,
                          spec_values: item.sku_spec_values,
                        },
                        locale.value,
                      )
                      return (
                        <RouterLink
                          key={`${item.product_id}-${item.sku_id ?? 0}`}
                          to={`/products?product_id=${item.product_id}`}
                          class="flex items-center justify-between gap-3 rounded-zs-sm border border-line bg-surface-strong px-3 py-2 text-sm transition-colors hover:border-primary"
                        >
                          <span class="min-w-0 truncate">
                            {getLocalizedText(item.product_title)}
                            {sku && <span class="ml-2 text-xs text-muted">{sku}</span>}
                          </span>
                          <span class="flex shrink-0 items-center gap-2">
                            <span class="zs-num text-xs text-muted">{item.available_stock}</span>
                            <Badge tone={item.alert_type === 'out_of_stock_products' ? 'danger' : 'warning'}>
                              {item.alert_type === 'out_of_stock_products'
                                ? t('admin.dashboard.inventoryAlerts.outOfStock')
                                : t('admin.dashboard.inventoryAlerts.lowStock')}
                            </Badge>
                          </span>
                        </RouterLink>
                      )
                    })}
                  </div>
                )}
              </div>
            )}
          </Card>
          {quickActions.value.length > 0 && (
            <Card title={t('admin.dashboard.quickActions.title')}>
              <div class="flex items-center gap-4">
                <div class="grid flex-1 grid-cols-2 gap-2 sm:grid-cols-3">
                  {quickActions.value.map((q) => {
                    const Icon = q.icon
                    return (
                      <RouterLink
                        key={q.path}
                        to={q.path}
                        class="zs-card-hover flex items-center gap-2 rounded-zs border border-line bg-surface-strong px-3 py-2.5 text-sm font-medium hover:border-primary hover:text-primary"
                      >
                        <Icon class="h-4 w-4 text-secondary" />
                        {q.label}
                      </RouterLink>
                    )
                  })}
                </div>
                <div class="hidden shrink-0 sm:block">
                  <Mascot size={110} mood="wink" float />
                </div>
              </div>
            </Card>
          )}
        </div>
      )
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.dashboard.title')} subtitle={t('admin.dashboard.subtitle')}>
          {{
            actions: () => (
              <>
                <div class="w-36">
                  <Select
                    modelValue={d.filters.range}
                    options={[
                      {
                        value: 'today',
                        label: t('admin.dashboard.range.today'),
                      },
                      {
                        value: '7d',
                        label: t('admin.dashboard.range.last7Days'),
                      },
                      {
                        value: '30d',
                        label: t('admin.dashboard.range.last30Days'),
                      },
                      {
                        value: 'custom',
                        label: t('admin.dashboard.range.custom'),
                      },
                    ]}
                    onUpdate:modelValue={(v) => d.setRange(String(v) as DashboardRange)}
                  />
                </div>
                {d.filters.range === 'custom' && (
                  <>
                    <div class="w-40">
                      <DateTimeInput type="date" v-model={d.filters.from} onUpdate:modelValue={() => void d.load()} />
                    </div>
                    <div class="w-40">
                      <DateTimeInput type="date" v-model={d.filters.to} onUpdate:modelValue={() => void d.load()} />
                    </div>
                  </>
                )}
                <Button variant="primary" loading={d.loading.value} onClick={() => void d.load(true)}>
                  <TrendingUp class="h-4 w-4" />
                  {t('admin.dashboard.actions.refreshNow')}
                </Button>
              </>
            ),
          }}
        </PageHeader>
        {d.error.value && <p class="rounded-zs bg-danger-soft px-4 py-3 text-sm text-danger-text">{d.error.value}</p>}
        {!d.overview.value && d.loading.value ? (
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            {Array.from({ length: 8 }).map((_, i) => (
              <div key={i} class="zs-skeleton h-32 rounded-zs-lg" />
            ))}
          </div>
        ) : (
          renderKpis()
        )}
        {renderTrends()}
        {renderFunnelAndRankings()}
        {renderAlertsAndActions()}
      </div>
    )
  },
})
