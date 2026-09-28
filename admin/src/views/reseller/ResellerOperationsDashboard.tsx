import { computed, defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Store, Globe, ShoppingBag, TriangleAlert } from 'lucide-vue-next'
import { Badge, Button, Card, DataTable, DateTimeInput, EmptyState, FormField, PageHeader, Select, cn, type DataTableColumn } from '@/components/ui'
import type { AdminResellerOperationsFinance, AdminResellerOperationsOverview } from '@/api/types'
import { formatDate, formatMoney } from '@/utils/format'
import { formatResellerOperationsPeriod } from '@/utils/resellerOperations'
import { operationsAlertClass } from './resellerUtils'
import { useResellerOperations, type OperationsRange } from './useResellerOperations'

type TopReseller = AdminResellerOperationsOverview['top_resellers'][number]
type PeriodRow = AdminResellerOperationsFinance['period_currency_rows'][number]
type CurrentRow = AdminResellerOperationsFinance['current_currency_rows'][number]

export default defineComponent({
  name: 'ResellerOperationsDashboardView',
  setup() {
    const { t } = useI18n()
    const o = useResellerOperations()
    onMounted(() => void o.loadAll())

    const lifecycleKpis = computed(() => {
      const r = o.overview.value?.lifecycle
      return [
        ['profilesTotal', r?.profiles_total],
        ['profilesPendingReview', r?.profiles_pending_review],
        ['profilesActive', r?.profiles_active],
        ['profilesSettlementFrozen', r?.profiles_settlement_frozen],
        ['domainsTotal', r?.domains_total],
        ['domainsPendingReview', r?.domains_pending_review],
        ['domainsActive', r?.domains_active],
        ['activeWithoutSiteConfig', r?.active_profiles_without_site_config],
      ] as const
    })
    const orderKpis = computed(() => {
      const r = o.overview.value?.orders
      return [
        ['ordersTotal', r?.orders_total],
        ['paidOrders', r?.paid_orders],
        ['completedOrders', r?.completed_orders],
        ['refundedOrders', r?.refunded_orders],
        ['selfDealingBlockedOrders', r?.self_dealing_blocked_orders],
        ['activeResellersWithOrders', r?.active_resellers_with_orders],
        ['averagePaidOrders', r?.average_paid_orders_per_active_reseller ?? '0.00'],
      ] as const
    })

    const kpiGrid = (items: readonly (readonly [string, number | string | undefined])[], accent: string) => (
      <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {items.map(([key, value]) => (
          <div key={key} class="zs-glass zs-card-hover relative overflow-hidden rounded-zs-lg p-4 shadow-zs-sm">
            <span class={cn('absolute inset-y-3 left-0 w-1 rounded-r-full', accent)} />
            <p class="text-xs font-medium text-muted">{t(`admin.resellerOperations.kpi.${key}`)}</p>
            <p class="zs-num mt-2 text-2xl font-bold text-fg">{o.loadingOverview.value ? '-' : (value ?? 0)}</p>
          </div>
        ))}
      </div>
    )

    const sectionTitle = (key: string, Icon: typeof Store, extra?: string) => (
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h2 class="zs-display flex items-center gap-2 text-base text-fg">
          <Icon class="h-4 w-4 text-primary" />
          {t(`admin.resellerOperations.sections.${key}`)}
        </h2>
        {extra && <span class="font-mono text-xs text-muted">{extra}</span>}
      </div>
    )

    const topColumns = (): DataTableColumn<TopReseller>[] => [
      {
        key: 'reseller',
        title: t('admin.resellerOperations.table.reseller'),
        render: (r) => (
          <div>
            <div class="text-sm font-medium">{r.display_name || r.email || `#${r.reseller_id}`}</div>
            <div class="mt-0.5 font-mono text-xs text-muted">
              #{r.reseller_id} / U{r.user_id}
            </div>
          </div>
        ),
      },
      {
        key: 'ordersTotal',
        title: t('admin.resellerOperations.table.ordersTotal'),
        class: 'zs-num text-sm',
        render: (r) => r.orders_total,
      },
      {
        key: 'paidOrders',
        title: t('admin.resellerOperations.table.paidOrders'),
        class: 'zs-num text-sm',
        render: (r) => r.paid_orders,
      },
      {
        key: 'activeDomains',
        title: t('admin.resellerOperations.table.activeDomains'),
        class: 'zs-num text-sm',
        render: (r) => r.active_domains,
      },
      {
        key: 'siteConfigured',
        title: t('admin.resellerOperations.table.siteConfigured'),
        render: (r) => <Badge tone={r.site_configured ? 'success' : 'neutral'}>{r.site_configured ? t('admin.common.yes') : t('admin.common.no')}</Badge>,
      },
      {
        key: 'lastOrderAt',
        title: t('admin.resellerOperations.table.lastOrderAt'),
        class: 'text-xs text-muted whitespace-nowrap',
        render: (r) => formatDate(r.last_order_at) || '-',
      },
    ]

    const financeEmpty = () => (o.loadingFinance.value ? t('admin.common.loading') : t('admin.resellerOperations.empty.financeRows'))

    const periodColumns = (): DataTableColumn<PeriodRow>[] => [
      {
        key: 'currency',
        title: t('admin.resellerOperations.table.currency'),
        class: 'font-mono text-sm font-semibold',
        render: (r) => r.currency,
      },
      {
        key: 'ordersTotal',
        title: t('admin.resellerOperations.table.ordersTotal'),
        class: 'zs-num text-sm',
        render: (r) => r.orders_total,
      },
      {
        key: 'paidOrders',
        title: t('admin.resellerOperations.table.paidOrders'),
        class: 'zs-num text-sm',
        render: (r) => r.paid_orders,
      },
      {
        key: 'gmvPaid',
        title: t('admin.resellerOperations.table.gmvPaid'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.gmv_paid, r.currency),
      },
      {
        key: 'profitEarned',
        title: t('admin.resellerOperations.table.profitEarned'),
        class: 'zs-num text-sm text-success-text',
        render: (r) => formatMoney(r.profit_earned, r.currency),
      },
      {
        key: 'refundDeducted',
        title: t('admin.resellerOperations.table.refundDeducted'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.refund_deducted, r.currency),
      },
      {
        key: 'withdrawPaid',
        title: t('admin.resellerOperations.table.withdrawPaid'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.withdraw_paid, r.currency),
      },
    ]

    const currentColumns = (): DataTableColumn<CurrentRow>[] => [
      {
        key: 'currency',
        title: t('admin.resellerOperations.table.currency'),
        class: 'font-mono text-sm font-semibold',
        render: (r) => r.currency,
      },
      {
        key: 'available',
        title: t('admin.resellerOperations.table.availableBalance'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.available_balance, r.currency),
      },
      {
        key: 'locked',
        title: t('admin.resellerOperations.table.lockedBalance'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.locked_balance, r.currency),
      },
      {
        key: 'negative',
        title: t('admin.resellerOperations.table.negativeBalance'),
        class: 'zs-num text-sm',
        render: (r) => formatMoney(r.negative_balance, r.currency),
      },
      {
        key: 'pendingWithdraw',
        title: t('admin.resellerOperations.table.pendingWithdraw'),
        render: (r) => (
          <div>
            <div class="zs-num text-sm">{formatMoney(r.pending_withdraw_amount, r.currency)}</div>
            <div class="mt-0.5 text-xs text-muted">
              {t('admin.resellerOperations.finance.pendingWithdrawCount', {
                count: r.pending_withdraw_count,
              })}
            </div>
          </div>
        ),
      },
      {
        key: 'abnormal',
        title: t('admin.resellerOperations.table.abnormalAccounts'),
        class: 'text-xs text-muted',
        render: (r) => (
          <div>
            <div>
              {t('admin.resellerOperations.finance.negativeBalanceAccounts', {
                count: r.negative_balance_accounts,
              })}
            </div>
            <div class="mt-0.5">
              {t('admin.resellerOperations.finance.frozenBalanceAccounts', {
                count: r.frozen_balance_accounts,
              })}
            </div>
          </div>
        ),
      },
    ]

    return () => {
      const overview = o.overview.value
      const busy = o.loadingOverview.value || o.loadingFinance.value
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.resellerOperations.title')} subtitle={t('admin.resellerOperations.subtitle')} />

          <div class="zs-glass rounded-zs-lg p-4 shadow-zs-sm">
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-[170px_220px_220px_auto] lg:items-end">
              <FormField label={t('admin.resellerOperations.filters.range')}>
                <Select
                  modelValue={o.filters.range}
                  onUpdate:modelValue={(v) => (o.filters.range = v as OperationsRange)}
                  options={[
                    {
                      label: t('admin.resellerOperations.filters.today'),
                      value: 'today',
                    },
                    {
                      label: t('admin.resellerOperations.filters.last7Days'),
                      value: '7d',
                    },
                    {
                      label: t('admin.resellerOperations.filters.last30Days'),
                      value: '30d',
                    },
                    {
                      label: t('admin.resellerOperations.filters.custom'),
                      value: 'custom',
                    },
                  ]}
                />
              </FormField>
              <FormField label={t('admin.resellerOperations.filters.from')}>
                <DateTimeInput v-model={o.filters.from} />
              </FormField>
              <FormField label={t('admin.resellerOperations.filters.to')}>
                <DateTimeInput v-model={o.filters.to} />
              </FormField>
              <div class="flex lg:justify-start">
                <Button variant="primary" size="md" disabled={busy} onClick={o.loadAll}>
                  <RefreshCw class={cn('h-4 w-4', busy && 'animate-spin')} />
                  {t('admin.resellerOperations.actions.refresh')}
                </Button>
              </div>
            </div>
          </div>

          {o.pageError.value && <div class="rounded-zs border border-danger/35 bg-danger-soft px-4 py-3 text-sm text-danger-text">{o.pageError.value}</div>}

          <section class="space-y-3">
            {sectionTitle('lifecycle', Store, overview ? formatResellerOperationsPeriod(overview.from, overview.to) : undefined)}
            {kpiGrid(lifecycleKpis.value, 'bg-primary')}
          </section>

          <section class="space-y-3">
            {sectionTitle('orders', ShoppingBag)}
            {kpiGrid(orderKpis.value, 'bg-secondary')}
          </section>

          <section class="space-y-3">
            {sectionTitle('alerts', TriangleAlert)}
            {overview?.alerts?.length ? (
              <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
                {overview.alerts.map((a) => (
                  <div key={a.type} class={cn('rounded-zs border px-4 py-3 text-sm', operationsAlertClass(a.level))}>
                    <div class="font-medium">{t(`admin.resellerOperations.alerts.${a.type}`)}</div>
                    <div class="zs-num mt-1 text-xl font-bold">{a.value}</div>
                  </div>
                ))}
              </div>
            ) : (
              <div class="rounded-zs border border-line bg-surface-muted/60 px-4 py-6 text-center text-sm text-muted">
                {t('admin.resellerOperations.empty.alerts')}
              </div>
            )}
          </section>

          <section class="space-y-3">
            {sectionTitle('topResellers', Globe)}
            <DataTable
              columns={topColumns()}
              rows={overview?.top_resellers ?? []}
              rowKey={(r) => r.reseller_id}
              loading={o.loadingOverview.value}
              emptyText={t('admin.resellerOperations.empty.topResellers')}
              minWidth="860px"
            />
          </section>

          {o.canViewFinance.value ? (
            <>
              <Card title={t('admin.resellerOperations.sections.financePeriod')} padded={false}>
                <div class="pt-3">
                  <DataTable bare columns={periodColumns()} rows={o.periodRows.value} rowKey={(r) => r.currency} emptyText={financeEmpty()} minWidth="860px" />
                </div>
              </Card>
              <Card title={t('admin.resellerOperations.sections.financeCurrent')} padded={false}>
                <div class="pt-3">
                  <DataTable
                    bare
                    columns={currentColumns()}
                    rows={o.currentRows.value}
                    rowKey={(r) => r.currency}
                    emptyText={financeEmpty()}
                    minWidth="860px"
                  />
                </div>
              </Card>
            </>
          ) : (
            <EmptyState title={t('admin.resellerOperations.finance.noPermission')} />
          )}
        </div>
      )
    }
  },
})
