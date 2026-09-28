import { defineComponent, onMounted } from 'vue'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import type { LucideIcon } from 'lucide-vue-next'
import { Banknote, CheckCircle2, ChevronDown, ChevronRight, ExternalLink, Globe, Palette, RotateCw, Settings, ShoppingBag, Tag, Upload } from 'lucide-vue-next'
import { useResellerDashboard, type SetupItemKey } from '@/composables/reseller/useResellerDashboard'
import { ResellerInactiveCard, ResellerPageHeader, ResellerPageState, ResellerTileLink } from '@/components/reseller/ConsoleParts'
import { ResellerDonut } from '@/components/reseller/ResellerDonut'
import { Badge, Button, Card, DataTable, StatCard, columns, cn } from '@/components/ui'
import type { ResellerOrderData } from '@/api/types'
import { formatResellerConsoleAmount, formatResellerConsoleDate, resellerOrderStatusTone } from '@/utils/reseller/console'

const setupIcons: Record<SetupItemKey, LucideIcon> = { profile: CheckCircle2, domain: Globe, site: Palette, products: Tag, orders: ShoppingBag }

export default defineComponent({
  name: 'ResellerDashboard',
  setup() {
    const { t } = useI18n()
    const d = useResellerDashboard()
    onMounted(() => void d.initialize())

    const quickActions: Array<{ to: string; label: string; icon: LucideIcon }> = [
      { to: '/reseller/withdraws', label: 'resellerConsole.nav.withdraws', icon: Upload },
      { to: '/reseller/domains', label: 'resellerConsole.nav.domains', icon: Globe },
      { to: '/reseller/products', label: 'resellerConsole.nav.products', icon: Tag },
      { to: '/reseller/site', label: 'resellerConsole.nav.site', icon: Settings },
    ]

    const orderColumns = columns<ResellerOrderData>([
      {
        key: 'order_no',
        title: t('resellerConsole.orders.orderNo'),
        render: (row) => (
          <RouterLink to={`/reseller/orders/${encodeURIComponent(row.order_no)}`} class="font-mono text-xs font-bold text-accent-text hover:underline">
            {row.order_no}
          </RouterLink>
        ),
      },
      { key: 'status', title: t('resellerConsole.orders.status'), render: (row) => <Badge tone={resellerOrderStatusTone(row.status)}>{d.statusLabel(row.status)}</Badge> },
      { key: 'total_amount', title: t('resellerConsole.orders.totalAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num">{formatResellerConsoleAmount(row.total_amount, row.currency)}</span> },
      { key: 'profit_amount', title: t('resellerConsole.orders.profitAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num text-success-text">{formatResellerConsoleAmount(row.profit_amount, row.currency)}</span> },
      { key: 'created_at', title: t('resellerConsole.orders.createdAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</span> },
    ])

    return () => (
      <div class="space-y-6">
        <ResellerPageHeader title={t('resellerConsole.dashboard.title')} description={t('resellerConsole.dashboard.description')}>
          {{
            actions: () => (
              <>
                {d.profile.primaryDomainUrl.value && (
                  <Button variant="secondary" size="sm" href={d.profile.primaryDomainUrl.value} target="_blank">
                    <ExternalLink class="size-4" />
                    {t('resellerConsole.dashboard.visitStore')}
                  </Button>
                )}
                <Button variant="secondary" size="sm" onClick={() => void d.initialize()}>
                  <RotateCw class="size-4" />
                  {t('orders.filters.refresh')}
                </Button>
              </>
            ),
          }}
        </ResellerPageHeader>

        {d.loading.value ? (
          <ResellerPageState loading title={t('resellerConsole.common.loading')} />
        ) : !d.isActive.value ? (
          <ResellerInactiveCard canApply={d.profile.state.value.canApply} opened={d.profile.state.value.opened} />
        ) : (
          <>
            <Card>
              <div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <div class="min-w-0">
                  <p class="text-xs font-bold tracking-[0.18em] text-primary-text">{t('resellerConsole.dashboard.storeLabel')}</p>
                  <h2 class="mt-1 break-all font-mono text-lg font-bold text-fg">{d.profile.primaryDomain.value || t('resellerConsole.dashboard.noDomain')}</h2>
                </div>
                <div class="flex flex-wrap gap-2">
                  <Badge tone="info">{d.settlementText.value}</Badge>
                  <Badge tone="success">{t('resellerConsole.dashboard.statusActive')}</Badge>
                </div>
              </div>
            </Card>

            <div class="grid grid-cols-2 gap-3 sm:gap-4 xl:grid-cols-4">
              <StatCard label={t('resellerConsole.dashboard.orderTotal')} value={d.total.value} tone="accent">
                {{ icon: () => <ShoppingBag class="size-5" /> }}
              </StatCard>
              <StatCard label={t('resellerConsole.orders.paidOrders')} tone="success">
                {{
                  default: () => (
                    <span>
                      {d.paidCount.value}
                      {d.paidRatioText.value && <span class="ml-2 text-sm text-muted">{d.paidRatioText.value}</span>}
                    </span>
                  ),
                  icon: () => <CheckCircle2 class="size-5" />,
                }}
              </StatCard>
              <StatCard label={t('personalCenter.reseller.primaryAvailable')} tone="gold">
                {{
                  default: () => (
                    <span>
                      <span class="text-xl">{d.primaryBalanceText.value}</span>
                      {d.primaryBalanceHint.value && <span class="mt-1 block text-xs font-medium text-muted">{d.primaryBalanceHint.value}</span>}
                    </span>
                  ),
                  icon: () => <Banknote class="size-5" />,
                }}
              </StatCard>
              <StatCard label={t('resellerConsole.dashboard.domainCount')} value={d.profile.snapshot.value?.domains?.length || 0} tone="secondary">
                {{ icon: () => <Globe class="size-5" /> }}
              </StatCard>
            </div>

            <Card>
              <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <h3 class="zs-title text-lg text-fg">{t('resellerConsole.dashboard.setup.title')}</h3>
                  <p class="mt-1 text-sm text-muted">{d.allSetupDone.value ? t('resellerConsole.dashboard.setup.allDone') : t('resellerConsole.dashboard.setup.description')}</p>
                </div>
                <div class="flex items-center gap-2">
                  <Badge tone={d.allSetupDone.value ? 'success' : 'accent'}>{`${d.completedSetupCount.value}/${d.setupChecklist.value.length}`}</Badge>
                  {d.allSetupDone.value && (
                    <Button variant="ghost" size="sm" onClick={() => (d.setupCollapsed.value = !d.setupCollapsed.value)}>
                      {d.setupCollapsed.value ? t('resellerConsole.dashboard.setup.expand') : t('resellerConsole.dashboard.setup.collapse')}
                      <ChevronDown class={cn('size-4 transition-transform', !d.setupCollapsed.value && 'rotate-180')} />
                    </Button>
                  )}
                </div>
              </div>
              {!d.setupCollapsed.value && (
                <div class="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-5">
                  {d.setupChecklist.value.map((item) => {
                    const Icon = setupIcons[item.key]
                    return (
                      <RouterLink key={item.key} to={item.to} class="zs-card-hover flex min-h-[124px] flex-col justify-between rounded-zs border border-line bg-surface-strong p-4">
                        <div class="flex items-start justify-between gap-2">
                          <span class={cn('flex size-9 items-center justify-center rounded-zs-sm', item.done ? 'bg-success-soft text-success-text' : 'bg-primary-soft text-primary-text')}>
                            <Icon class="size-5" />
                          </span>
                          <Badge tone={item.done ? 'success' : 'warning'} size="xs">
                            {item.done ? t('resellerConsole.dashboard.setup.ready') : t('resellerConsole.dashboard.setup.pending')}
                          </Badge>
                        </div>
                        <div>
                          <div class="mt-3 text-sm font-bold text-fg">{item.label}</div>
                          <p class="mt-1 text-xs text-muted">{item.description}</p>
                        </div>
                      </RouterLink>
                    )
                  })}
                </div>
              )}
            </Card>

            <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card>
                <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.dashboard.orderDistribution')}</h3>
                {d.statusSegments.value.length ? (
                  <ResellerDonut segments={d.statusSegments.value} centerValue={d.total.value} centerLabel={t('resellerConsole.dashboard.orderTotal')} />
                ) : (
                  <p class="py-6 text-center text-sm text-muted">{t('resellerConsole.orders.empty')}</p>
                )}
              </Card>
              <Card>
                <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.dashboard.balanceOverview')}</h3>
                {d.balanceSegments.value.length ? (
                  <ResellerDonut segments={d.balanceSegments.value} centerValue={d.finance.balances.value.length} centerLabel={t('personalCenter.reseller.currencyCount')} />
                ) : (
                  <p class="py-6 text-center text-sm text-muted">{t('personalCenter.reseller.balanceEmpty')}</p>
                )}
              </Card>
            </div>

            <section>
              <h3 class="zs-title mb-3 text-lg text-fg">{t('resellerConsole.dashboard.quickActions')}</h3>
              <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
                {quickActions.map((a) => {
                  const Icon = a.icon
                  return (
                    <ResellerTileLink key={a.to} to={a.to}>
                      <div class="flex items-center gap-3">
                        <span class="zs-gradient-bg flex size-10 shrink-0 items-center justify-center rounded-zs-sm text-on-primary transition-transform group-hover:scale-105">
                          <Icon class="size-5" />
                        </span>
                        <span class="text-sm font-bold text-fg">{t(a.label)}</span>
                      </div>
                    </ResellerTileLink>
                  )
                })}
              </div>
            </section>

            <Card>
              <div class="mb-4 flex items-center justify-between gap-3">
                <h3 class="zs-title text-lg text-fg">{t('zsReseller.recentOrders')}</h3>
                <RouterLink to="/reseller/orders" class="inline-flex items-center gap-1 text-sm font-bold text-accent-text hover:underline">
                  {t('resellerConsole.dashboard.viewOrders')}
                  <ChevronRight class="size-4" />
                </RouterLink>
              </div>
              <DataTable columns={orderColumns} rows={d.orders.rows.value} rowKey="order_no" emptyText={t('resellerConsole.orders.empty')} />
            </Card>
          </>
        )}
      </div>
    )
  },
})
