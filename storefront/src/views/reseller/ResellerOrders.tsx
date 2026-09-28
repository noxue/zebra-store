import { computed, defineComponent, onMounted } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, Coins, Eye, RotateCw, Search, ShoppingBag } from 'lucide-vue-next'
import type { ResellerOrderData } from '@/api/types'
import { useResellerOrderList } from '@/composables/reseller/useResellerOrderList'
import { ResellerPageHeader } from '@/components/reseller/ConsoleParts'
import { Alert, Badge, Button, Card, DataTable, Field, Input, Pagination, Select, StatCard, columns } from '@/components/ui'
import { formatResellerConsoleAmount, formatResellerConsoleDate, resellerOrderStatusTone, resellerProfitStatusKey, resellerProfitTone } from '@/utils/reseller/console'
import { RESELLER_ORDER_FILTER_STATUSES } from '@/utils/reseller/constants'

export default defineComponent({
  name: 'ResellerOrders',
  setup() {
    const { t } = useI18n()
    const router = useRouter()
    const o = useResellerOrderList()
    onMounted(() => void o.reload())
    const statusOptions = computed(() => [
      { value: 'all', label: t('resellerConsole.orders.statusAll') },
      ...RESELLER_ORDER_FILTER_STATUSES.map((s) => ({ value: s, label: o.statusLabel(s) })),
    ])
    const detailPath = (no: string) => `/reseller/orders/${encodeURIComponent(no)}`
    const orderColumns = columns<ResellerOrderData>([
      {
        key: 'order_no',
        title: t('resellerConsole.orders.orderNo'),
        render: (row) => (
          <RouterLink to={detailPath(row.order_no)} class="font-mono text-xs font-bold text-accent-text hover:underline">
            {row.order_no}
          </RouterLink>
        ),
      },
      { key: 'status', title: t('resellerConsole.orders.status'), render: (row) => <Badge tone={resellerOrderStatusTone(row.status)}>{o.statusLabel(row.status)}</Badge> },
      { key: 'domain', title: t('resellerConsole.orders.domain'), render: (row) => <span class="font-mono text-xs text-muted">{row.domain || '-'}</span> },
      { key: 'buyer', title: t('resellerConsole.orders.buyer'), render: (row) => <span class="text-xs">{row.buyer_label || '-'}</span> },
      { key: 'total', title: t('resellerConsole.orders.totalAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num">{formatResellerConsoleAmount(row.total_amount, row.currency)}</span> },
      { key: 'profit', title: t('resellerConsole.orders.profitAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num text-success-text">{formatResellerConsoleAmount(row.profit_amount, row.currency)}</span> },
      {
        key: 'profit_status',
        title: t('resellerConsole.orders.profitStatus'),
        render: (row) => <Badge tone={resellerProfitTone(row.profit_status)}>{t(`resellerConsole.orders.profit.${resellerProfitStatusKey(row.profit_status)}`)}</Badge>,
      },
      { key: 'created_at', title: t('resellerConsole.orders.createdAt'), render: (row) => <span class="whitespace-nowrap text-xs text-muted">{formatResellerConsoleDate(row.created_at)}</span> },
      {
        key: 'actions',
        title: '',
        align: 'right',
        render: (row) => (
          <Button size="xs" variant="soft" onClick={() => void router.push(detailPath(row.order_no))}>
            <Eye class="size-3.5" />
            {t('resellerConsole.orders.viewDetail')}
          </Button>
        ),
      },
    ])

    return () => (
      <div class="space-y-5">
        <ResellerPageHeader title={t('resellerConsole.orders.title')} description={t('resellerConsole.orders.description')}>
          {{
            actions: () => (
              <Button variant="secondary" size="sm" onClick={() => void o.reload()}>
                <RotateCw class="size-4" />
                {t('orders.filters.refresh')}
              </Button>
            ),
          }}
        </ResellerPageHeader>

        <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <StatCard label={t('resellerConsole.orders.title')} value={o.stats.value?.total || 0} tone="accent">
            {{ icon: () => <ShoppingBag class="size-5" /> }}
          </StatCard>
          <StatCard label={t('resellerConsole.orders.paidOrders')} value={o.stats.value?.by_status?.paid || 0} tone="success">
            {{ icon: () => <CheckCircle2 class="size-5" /> }}
          </StatCard>
          <StatCard label={t('resellerConsole.orders.currencyKinds')} value={o.currencyKinds.value} tone="gold">
            {{ icon: () => <Coins class="size-5" /> }}
          </StatCard>
        </div>

        <Card>
          <form
            class="grid gap-3 md:grid-cols-2 xl:grid-cols-[1.3fr_1fr_1fr_1fr_auto] xl:items-end"
            onSubmit={(e: Event) => {
              e.preventDefault()
              void o.reload()
            }}
          >
            <Field label={t('resellerConsole.orders.orderNo')}>
              <Input v-model={o.filters.order_no} size="sm" placeholder={t('resellerConsole.orders.search')}>
                {{ prefix: () => <Search class="size-4" /> }}
              </Input>
            </Field>
            <Field label={t('resellerConsole.orders.status')}>
              <Select v-model={o.filters.status} options={statusOptions.value} size="sm" />
            </Field>
            <Field label={t('resellerConsole.common.dateFrom')}>
              <Input v-model={o.filters.created_from} type="date" size="sm" />
            </Field>
            <Field label={t('resellerConsole.common.dateTo')}>
              <Input v-model={o.filters.created_to} type="date" size="sm" />
            </Field>
            <div class="flex gap-2">
              <Button type="submit" size="sm">
                {t('zsReseller.filter')}
              </Button>
              <Button size="sm" variant="secondary" disabled={!o.hasActiveFilter.value} onClick={o.resetFilters}>
                {t('resellerConsole.common.reset')}
              </Button>
            </div>
          </form>
          <p class="mt-3 text-xs text-muted">{t('resellerConsole.orders.profitHelp')}</p>
        </Card>

        {o.error.value && (
          <Alert tone="error" title={t('resellerConsole.common.loadFailed')}>
            {o.error.value}
          </Alert>
        )}

        <Card>
          <DataTable
            columns={orderColumns}
            rows={o.rows.value}
            rowKey="order_no"
            loading={o.loading.value}
            emptyText={o.hasActiveFilter.value ? t('resellerConsole.common.noFilterResult') : t('resellerConsole.orders.empty')}
          />
          <div class="mt-4">
            <Pagination page={o.pagination.page} totalPages={o.pagination.total_page} onChange={(pg: number) => void o.goPage(pg)} />
          </div>
        </Card>
      </div>
    )
  },
})
