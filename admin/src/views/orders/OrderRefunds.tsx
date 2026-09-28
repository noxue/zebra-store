import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Search } from 'lucide-vue-next'
import type { AdminOrderRefund } from '@/api/types'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, RangeFilter, type DataTableColumn } from '@/components/ui'
import { formatDate, getLocalizedText } from '@/utils/format'
import { formatSkuDisplayLabel } from '@/utils/sku'
import { adminUrl } from '@/utils/adminBase'
import OrderRefundsDialog from './components/OrderRefundsDialog'
import { toQueryId, toQueryText } from './orderUtils'
import { refundTypeCode, useOrderRefunds } from './useOrderRefunds'

export default defineComponent({
  name: 'OrderRefundsView',
  setup() {
    const { t, locale } = useI18n()
    const route = useRoute()
    const p = useOrderRefunds()

    onMounted(() => {
      if (route.query.user_id) p.filters.userId = toQueryText(route.query.user_id)
      void p.list.fetchData(1)
      const id = toQueryId(route.query.refund_id)
      if (id) p.openDetail(id)
    })
    watch(
      () => route.query.user_id,
      (v) => {
        if (v === undefined) return
        p.filters.userId = toQueryText(v)
        void p.list.fetchData(1)
      },
    )
    watch(
      () => route.query.refund_id,
      (v) => {
        const id = toQueryId(v)
        if (id) p.openDetail(id)
      },
    )

    const typeLabel = (r: AdminOrderRefund) => {
      const code = refundTypeCode(r)
      if (code === 'wallet') return <Badge tone="secondary">{t('admin.orderRefunds.typeWallet')}</Badge>
      if (code === 'manual') return <Badge tone="info">{t('admin.orderRefunds.typeManual')}</Badge>
      return code || '-'
    }

    const columns = (): DataTableColumn<AdminOrderRefund>[] => [
      { key: 'id', title: t('admin.orderRefunds.table.id'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'orderId',
        title: t('admin.orderRefunds.table.orderId'),
        class: 'min-w-[120px] font-mono text-xs',
        render: (r) =>
          r.order_id ? (
            <a href={adminUrl(`/orders?order_id=${r.order_id}`)} target="_blank" rel="noopener" class="text-accent underline-offset-4 hover:underline">
              #{r.order_id}
            </a>
          ) : (
            '-'
          ),
      },
      {
        key: 'productInfo',
        title: t('admin.orderRefunds.table.productInfo'),
        class: 'min-w-[200px]',
        render: (r) =>
          r.items?.length ? (
            <div class="space-y-1">
              {r.items.map((entry) => {
                const sku = formatSkuDisplayLabel(entry.sku_snapshot, locale.value)
                return (
                  <div key={entry.id} class="text-xs">
                    <span class="text-fg">{getLocalizedText(entry.title) || '-'}</span>
                    {sku && <span class="ml-1 text-muted">({sku})</span>}
                    <span class="ml-1 text-muted">x{entry.quantity}</span>
                  </div>
                )
              })}
            </div>
          ) : (
            <span class="text-xs text-muted">-</span>
          ),
      },
      {
        key: 'user',
        title: t('admin.orderRefunds.table.user'),
        class: 'min-w-[180px] text-xs text-muted',
        render: (r) =>
          r.guest_email ? (
            <div class="break-all">
              {t('admin.orderRefunds.guestLabel')}: {r.guest_email}
            </div>
          ) : r.user_id ? (
            <div>
              <div class="break-words text-fg">{r.user_display_name || '-'}</div>
              <div class="break-all">{r.user_email || '-'}</div>
              <a href={adminUrl(`/users/${r.user_id}`)} target="_blank" rel="noopener" class="mt-0.5 inline-block text-accent underline-offset-4 hover:underline">
                #{r.user_id}
              </a>
            </div>
          ) : (
            '-'
          ),
      },
      { key: 'refundType', title: t('admin.orderRefunds.table.refundType'), render: typeLabel },
      {
        key: 'amount',
        title: t('admin.orderRefunds.table.amount'),
        class: 'whitespace-nowrap',
        render: (r) => (
          <span class="zs-num font-semibold text-fg">
            {r.amount} {r.currency}
          </span>
        ),
      },
      {
        key: 'paymentFeeRefund',
        title: t('admin.orderRefunds.table.paymentFeeRefund'),
        class: 'text-xs',
        render: (r) =>
          r.payment_fee_refunded ? (
            <span class="font-mono text-success-text">
              {r.payment_fee_refunded_amount} {r.currency}
            </span>
          ) : (
            <span class="text-muted">{t('admin.orderRefunds.paymentFeeNotRefunded')}</span>
          ),
      },
      { key: 'createdAt', title: t('admin.orderRefunds.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.orderRefunds.table.action'),
        align: 'right',
        render: (r) => (
          <Button size="sm" variant="primary" onClick={() => p.openDetail(r.id)}>
            {t('admin.orderRefunds.view')}
          </Button>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.orderRefunds.title')} />
        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input icon={Search} v-model={p.filters.userId} placeholder={t('admin.orderRefunds.filterUserId')} onUpdate:modelValue={p.list.debouncedSearch} onEnter={p.list.handleSearch} />
                <Input v-model={p.filters.userKeyword} placeholder={t('admin.orderRefunds.filterUserKeyword')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.orderNo} placeholder={t('admin.orderRefunds.filterOrderNo')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.guestEmail} placeholder={t('admin.orderRefunds.filterGuestEmail')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.productKeyword} placeholder={t('admin.orderRefunds.filterProductKeyword')} onUpdate:modelValue={p.list.debouncedSearch} />
                <RangeFilter
                  label={t('admin.orderRefunds.filterCreatedRange')}
                  from={p.filters.createdFrom}
                  to={p.filters.createdTo}
                  fromPlaceholder={t('admin.orderRefunds.filterCreatedFrom')}
                  toPlaceholder={t('admin.orderRefunds.filterCreatedTo')}
                  onUpdate:from={(v: string) => (p.filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (p.filters.createdTo = v)}
                  onChange={p.list.handleSearch}
                />
              </>
            ),
            actions: () => (
              <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                <RefreshCw class="h-3.5 w-3.5" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </FilterBar>

        <div>
          <DataTable
            columns={columns()}
            rows={p.list.items.value}
            rowKey={(r) => r.id}
            loading={p.list.loading.value}
            emptyText={t('admin.orderRefunds.empty')}
            minWidth="1040px"
          />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <OrderRefundsDialog modelValue={p.showDetail.value} refundId={p.selectedRefundId.value} onUpdate:modelValue={p.onDetailToggle} onUpdated={() => void p.list.refresh()} />
      </div>
    )
  },
})
