import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, Clock, Layers, RefreshCw, RotateCcw, Search, ShoppingBag, Wallet } from 'lucide-vue-next'
import { Price } from '@/components/common/Price'
import { Badge, Button, Card, EmptyState, Field, Input, Pagination, Select, StatCard, Tabs } from '@/components/ui'
import { useOrdersPanel } from '@/composables/personal/useOrdersPanel'
import { formatDateTime } from '@/utils/format'
import { OrderRow } from './OrderRow'
import { PanelHeading, SkeletonRows } from './PanelParts'

/** Product orders + recharge orders tabs. */
export const OrdersPanel = defineComponent({
  name: 'OrdersPanel',
  setup() {
    const { t } = useI18n()
    const p = useOrdersPanel()

    const filterBar = (opts: {
      keywordLabel: string
      placeholder: string
      keyword: string
      onKeyword: (v: string) => void
      status: string
      statusOptions: { value: string | number; label: string }[]
      onStatus: (v: string | number) => void
      onSearch: () => void
      onReset: () => void
      onRefresh: () => void
    }) => (
      <Card padding="sm">
        <div class="flex flex-col gap-3 lg:flex-row lg:items-end">
          <div class="w-full lg:max-w-xs">
            <Field label={opts.keywordLabel}>
              <Input modelValue={opts.keyword} placeholder={opts.placeholder} onUpdate:modelValue={opts.onKeyword} onEnter={opts.onSearch}>
                {{ prefix: () => <Search class="size-4" /> }}
              </Input>
            </Field>
          </div>
          <div class="w-full lg:w-52">
            <Field label={t('orders.filters.status')}>
              <Select modelValue={opts.status} options={opts.statusOptions} onUpdate:modelValue={opts.onStatus} />
            </Field>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <Button onClick={opts.onSearch}>
              <Search class="size-4" />
              {t('orders.filters.search')}
            </Button>
            <Button variant="secondary" onClick={opts.onReset}>
              <RotateCcw class="size-4" />
              {t('orders.filters.reset')}
            </Button>
            <Button variant="ghost" onClick={opts.onRefresh}>
              <RefreshCw class="size-4" />
              {t('orders.filters.refresh')}
            </Button>
          </div>
        </div>
      </Card>
    )

    const productTab = () => {
      const list = p.orders
      return (
        <>
          <div class="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <StatCard label={t('orders.stats.totalMatched')} value={list.pagination.total} tone="accent">
              {{ icon: () => <ShoppingBag class="size-5" /> }}
            </StatCard>
            <StatCard label={t('orders.stats.currentPage')} value={list.rows.value.length} tone="secondary">
              {{ icon: () => <Layers class="size-5" /> }}
            </StatCard>
            <StatCard label={t('orders.stats.pendingPayment')} value={p.pendingPaymentCount.value} tone="gold">
              {{ icon: () => <Clock class="size-5" /> }}
            </StatCard>
            <StatCard label={t('orders.stats.finished')} value={p.finishedCount.value} tone="success">
              {{ icon: () => <CheckCircle2 class="size-5" /> }}
            </StatCard>
          </div>
          {filterBar({
            keywordLabel: t('orders.filters.keyword'),
            placeholder: t('orders.filters.orderNoPlaceholder'),
            keyword: p.orderFilters.orderNo,
            onKeyword: p.onOrderNoInput,
            status: p.orderFilters.status,
            statusOptions: p.orderStatusOptions.value,
            onStatus: p.onOrderStatus,
            onSearch: () => void p.loadOrders(1),
            onReset: p.resetOrderFilters,
            onRefresh: () => void p.loadOrders(list.pagination.page),
          })}
          {list.loading.value && list.rows.value.length === 0 ? (
            <SkeletonRows height="h-24" />
          ) : list.rows.value.length === 0 ? (
            <Card>
              <EmptyState title={p.hasOrderFilters.value ? t('orders.emptyFiltered') : t('orders.empty')}>
                <Button to="/products">{t('orders.emptyAction')}</Button>
              </EmptyState>
            </Card>
          ) : (
            <div class={['space-y-3 transition-opacity', list.loading.value && 'opacity-60']}>
              {list.rows.value.map((o) => (
                <OrderRow key={o.order_no} order={o} showDiscounts />
              ))}
            </div>
          )}
          <Pagination page={list.pagination.page} totalPages={list.pagination.total_page} onChange={(pg: number) => void p.loadOrders(pg)} />
        </>
      )
    }

    const rechargeTab = () => {
      const list = p.recharges
      return (
        <>
          <div class="grid grid-cols-2 gap-3 lg:grid-cols-3">
            <StatCard label={t('orders.stats.totalMatched')} value={list.pagination.total} tone="accent">
              {{ icon: () => <Wallet class="size-5" /> }}
            </StatCard>
            <StatCard label={t('orders.stats.currentPage')} value={list.rows.value.length} tone="secondary">
              {{ icon: () => <Layers class="size-5" /> }}
            </StatCard>
            <StatCard label={t('orders.stats.pendingPayment')} value={p.rechargePendingCount.value} tone="gold">
              {{ icon: () => <Clock class="size-5" /> }}
            </StatCard>
          </div>
          {filterBar({
            keywordLabel: t('orders.rechargeFilters.keyword'),
            placeholder: t('orders.rechargeFilters.rechargeNoPlaceholder'),
            keyword: p.rechargeFilters.rechargeNo,
            onKeyword: p.onRechargeNoInput,
            status: p.rechargeFilters.status,
            statusOptions: p.rechargeStatusOptions.value,
            onStatus: p.onRechargeStatus,
            onSearch: () => void p.loadRecharges(1),
            onReset: p.resetRechargeFilters,
            onRefresh: () => void p.loadRecharges(list.pagination.page),
          })}
          {list.loading.value && list.rows.value.length === 0 ? (
            <SkeletonRows height="h-24" />
          ) : list.rows.value.length === 0 ? (
            <Card>
              <EmptyState title={t('orders.rechargeEmpty')}>
                <Button to="/me/wallet">{t('orders.rechargeEmptyAction')}</Button>
              </EmptyState>
            </Card>
          ) : (
            <div class={['space-y-3 transition-opacity', list.loading.value && 'opacity-60']}>
              {list.rows.value.map((r) => (
                <div key={r.recharge_no} class="zs-card-hover rounded-zs border border-line bg-surface-strong px-4 py-4 sm:px-5">
                  <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                    <div class="min-w-0">
                      <div class="truncate text-xs text-muted">
                        {t('personalCenter.wallet.rechargeNoLabel')}：<span class="zs-num">{r.recharge_no}</span>
                      </div>
                      <div class="mt-1.5">
                        <Price amount={r.amount} currency={r.currency} size="lg" highlight />
                      </div>
                      <div class="mt-1.5 text-xs text-muted">{formatDateTime(r.created_at)}</div>
                    </div>
                    <div class="flex flex-wrap items-center gap-2">
                      <Badge tone={p.rechargeStatusTone(r.status)}>{p.rechargeStatusText(r.status)}</Badge>
                      <Button size="sm" variant="secondary" to={`/recharge-orders/${encodeURIComponent(r.recharge_no)}`}>
                        {t('orders.viewDetails')}
                      </Button>
                      {r.status === 'pending' && (
                        <Button size="sm" to={`/recharge-orders/${encodeURIComponent(r.recharge_no)}`}>
                          {t('orders.payNow')}
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
          <Pagination page={list.pagination.page} totalPages={list.pagination.total_page} onChange={(pg: number) => void p.loadRecharges(pg)} />
        </>
      )
    }

    return () => {
      const pag = p.activeTab.value === 'product' ? p.orders.pagination : p.recharges.pagination
      return (
        <div class="space-y-5">
          <Card>
            <PanelHeading title={t('orders.title')} description={t('orders.subtitle')} icon={ShoppingBag}>
              {{
                actions: () => (
                  <>
                    <Badge tone="neutral">{t('orders.pageInfo', { page: pag.page, total: Math.max(pag.total_page, 1) })}</Badge>
                    <Button size="sm" variant="ghost" to="/products">
                      {t('orders.continueShopping')}
                    </Button>
                  </>
                ),
              }}
            </PanelHeading>
            <Tabs
              modelValue={p.activeTab.value}
              items={[
                { key: 'product', label: t('orders.tabs.product'), icon: ShoppingBag },
                { key: 'recharge', label: t('orders.tabs.recharge'), icon: Wallet },
              ]}
              onUpdate:modelValue={p.switchTab}
            />
          </Card>
          {p.activeTab.value === 'product' ? productTab() : rechargeTab()}
        </div>
      )
    }
  },
})
