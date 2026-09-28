import { defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Copy, RefreshCw, Search } from 'lucide-vue-next'
import type { AdminOrder } from '@/api/types'
import { Badge, Button, DataTable, FilterBar, IdCell, Input, ListPagination, PageHeader, RangeFilter, Select, type DataTableColumn } from '@/components/ui'
import { copyText } from '@/utils/clipboard'
import { formatDate, formatMoney, getLocalizedText } from '@/utils/format'
import { formatSkuDisplayLabel } from '@/utils/sku'
import { ORDER_STATUSES, orderStatusLabel, orderStatusTone } from '@/utils/status'
import { adminUrl } from '@/utils/adminBase'
import OrderDetailDialog from './components/OrderDetailDialog'
import OrderFulfillmentModal from './components/OrderFulfillmentModal'
import { canCreateFulfillment, canUpdateStatus, editableOrderStatuses, ORDER_SORT_OPTIONS, toQueryId, toQueryText } from './orderUtils'
import { useOrders } from './useOrders'

const SORT_LABEL_KEYS: Record<(typeof ORDER_SORT_OPTIONS)[number], string> = {
  created_at_desc: 'sortCreatedDesc',
  created_at_asc: 'sortCreatedAsc',
  updated_at_desc: 'sortUpdatedDesc',
  updated_at_asc: 'sortUpdatedAsc',
  total_amount_desc: 'sortAmountDesc',
  total_amount_asc: 'sortAmountAsc',
}

export default defineComponent({
  name: 'OrdersView',
  setup() {
    const { t, locale } = useI18n()
    const route = useRoute()
    const p = useOrders()

    onMounted(() => {
      p.filters.userId = toQueryText(route.query.user_id)
      p.filters.orderNo = toQueryText(route.query.order_no)
      void p.fetchRefundConfig()
      void p.list.fetchData(1)
      const orderId = toQueryId(route.query.order_id)
      if (orderId) p.openDetailById(orderId)
    })
    watch(
      () => route.query.order_id,
      (v) => {
        const id = toQueryId(v)
        if (id) p.openDetailById(id)
      },
    )
    watch(
      () => route.query.user_id,
      (v) => {
        p.filters.userId = toQueryText(v)
        void p.list.fetchData(1)
      },
    )

    const statusOptions = () => ORDER_STATUSES.map((s) => ({ label: orderStatusLabel(t, s), value: s }))
    const editableStatusOptions = () => editableOrderStatuses(ORDER_STATUSES).map((s) => ({ label: orderStatusLabel(t, s), value: s }))

    const columns = (): DataTableColumn<AdminOrder>[] => [
      {
        key: 'id',
        title: t('admin.orders.table.id'),
        render: (o) => <IdCell value={o.id} />,
      },
      {
        key: 'orderNo',
        title: t('admin.orders.table.orderNo'),
        class: 'min-w-[128px]',
        render: (o) => (
          <div class="flex items-center gap-1.5">
            <span class="break-all font-mono text-xs font-medium text-fg">{o.order_no}</span>
            <button
              type="button"
              title={t('admin.common.copy')}
              class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-zs-sm border border-line text-muted transition-colors hover:border-primary/40 hover:text-primary"
              onClick={() => copyText(o.order_no).catch(() => undefined)}
            >
              <Copy class="h-3 w-3" />
            </button>
          </div>
        ),
      },
      {
        key: 'items',
        title: t('admin.orders.table.items'),
        class: 'min-w-[140px]',
        render: (o) =>
          o.items?.length ? (
            <div class="space-y-1">
              {o.items.map((item) => {
                const sku = formatSkuDisplayLabel(item.sku_snapshot, locale.value)
                return (
                  <div key={item.id} class="text-xs">
                    <span class="text-fg">{getLocalizedText(item.title) || '-'}</span>
                    {sku && <span class="ml-1 text-muted">({sku})</span>}
                    <span class="ml-1 text-muted">x{item.quantity}</span>
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
        title: t('admin.orders.table.user'),
        class: 'min-w-[130px] text-xs text-muted',
        render: (o) =>
          o.user_id ? (
            <div>
              <div class="break-words text-fg">{o.user_display_name || '-'}</div>
              <div class="break-all">{o.user_email || '-'}</div>
              <a href={adminUrl(`/users/${o.user_id}`)} target="_blank" rel="noopener" class="mt-0.5 inline-block text-accent underline-offset-4 hover:underline">
                #{o.user_id}
              </a>
            </div>
          ) : (
            <div class="break-all">
              <Badge tone="neutral">{t('admin.orders.guestLabel')}</Badge>
              <div class="mt-1">{o.guest_email || '-'}</div>
            </div>
          ),
      },
      {
        key: 'ip',
        title: t('admin.orders.table.ip'),
        class: 'font-mono text-xs text-muted whitespace-nowrap',
        render: (o) => o.client_ip || '-',
      },
      {
        key: 'amount',
        title: t('admin.orders.table.amount'),
        class: 'whitespace-nowrap',
        render: (o) => <span class="zs-num font-semibold text-fg">{formatMoney(o.total_amount, o.currency)}</span>,
      },
      {
        key: 'status',
        title: t('admin.orders.table.status'),
        render: (o) => (
          <Badge tone={orderStatusTone(o.status)} dot>
            {orderStatusLabel(t, o.status)}
          </Badge>
        ),
      },
      // Date and time on two lines so both timestamp columns stay visible next to the sticky actions (original shows both).
      {
        key: 'createdAt',
        title: t('admin.orders.table.createdAt'),
        class: 'min-w-[88px] text-xs text-muted',
        render: (o) => formatDate(o.created_at),
      },
      {
        key: 'updatedAt',
        title: t('admin.orders.table.updatedAt'),
        class: 'min-w-[88px] text-xs text-muted',
        render: (o) => formatDate(o.updated_at),
      },
      {
        key: 'action',
        title: t('admin.orders.table.action'),
        align: 'right',
        class: 'sticky right-0 z-10 min-w-[140px] bg-surface-solid/95 backdrop-blur-sm shadow-[-8px_0_12px_-10px_var(--zs-border)]',
        headerClass: 'sticky right-0 z-10 bg-surface-solid/95 backdrop-blur-sm',
        render: (o) => (
          <div class="flex flex-col items-end gap-1.5">
            {p.allowed.value.updateStatus && canUpdateStatus(o) && (
              <>
                <div class="w-[128px]">
                  <Select size="sm" v-model={p.statusEdits[o.id]} options={editableStatusOptions()} />
                </div>
                <Button size="xs" disabled={p.statusEdits[o.id] === o.status} onClick={() => p.updateStatus(o)}>
                  {t('admin.orders.update')}
                </Button>
              </>
            )}
            {p.allowed.value.fulfill && canCreateFulfillment(o) && (
              <Button size="xs" variant="secondary" onClick={() => p.openFulfillment(o)}>
                {t('admin.orders.fulfillmentCreate')}
              </Button>
            )}
            {p.allowed.value.updateStatus && o.status === 'delivered' && (
              <Button size="xs" variant="soft" onClick={() => p.markCompleted(o)}>
                {t('admin.orders.markCompleted')}
              </Button>
            )}
            <Button size="xs" variant="primary" onClick={() => p.openDetail(o)}>
              {t('admin.orders.view')}
            </Button>
          </div>
        ),
      },
    ]

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.orders.title')} />
        <FilterBar cols={5}>
          {{
            default: () => (
              <>
                <Input
                  icon={Search}
                  v-model={p.filters.userId}
                  placeholder={t('admin.orders.filterUserId')}
                  onUpdate:modelValue={p.list.debouncedSearch}
                  onEnter={p.list.handleSearch}
                />
                <Input v-model={p.filters.userKeyword} placeholder={t('admin.orders.filterUserKeyword')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.orderNo} placeholder={t('admin.orders.filterOrderNo')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.guestEmail} placeholder={t('admin.orders.filterGuestEmail')} onUpdate:modelValue={p.list.debouncedSearch} />
                <Input v-model={p.filters.productKeyword} placeholder={t('admin.orders.filterProductKeyword')} onUpdate:modelValue={p.list.debouncedSearch} />
                <RangeFilter
                  label={t('admin.orders.filterCreatedRange')}
                  from={p.filters.createdFrom}
                  to={p.filters.createdTo}
                  fromPlaceholder={t('admin.orders.filterCreatedFrom')}
                  toPlaceholder={t('admin.orders.filterCreatedTo')}
                  onUpdate:from={(v: string) => (p.filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (p.filters.createdTo = v)}
                  onChange={p.list.handleSearch}
                />
                <Select
                  v-model={p.filters.status}
                  onChange={p.list.handleSearch}
                  options={[
                    {
                      label: t('admin.orders.filterStatusAll'),
                      value: '__all__',
                    },
                    ...statusOptions(),
                  ]}
                />
                <Select
                  v-model={p.filters.sortBy}
                  onChange={p.list.handleSearch}
                  options={[
                    { label: t('admin.orders.sortDefault'), value: '__all__' },
                    ...ORDER_SORT_OPTIONS.map((s) => ({
                      label: t(`admin.orders.${SORT_LABEL_KEYS[s]}`),
                      value: s,
                    })),
                  ]}
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
          <DataTable columns={columns()} rows={p.list.items.value} rowKey={(o) => o.id} loading={p.list.loading.value} emptyText={t('admin.orders.empty')} minWidth="1080px" dense />
          <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />
        </div>

        <OrderDetailDialog
          modelValue={p.showDetail.value}
          order={p.selectedOrder.value}
          maxRefundDays={p.maxRefundDays.value}
          onUpdate:modelValue={p.onDetailToggle}
          onRefresh={() => void p.list.refresh()}
          onOpenFulfillment={p.openFulfillment}
        />
        <OrderFulfillmentModal
          modelValue={p.showFulfillment.value}
          order={p.selectedOrder.value}
          parentId={p.fulfillmentParentId.value}
          onUpdate:modelValue={p.onFulfillmentToggle}
          onSuccess={p.onFulfillmentSuccess}
        />
      </div>
    )
  },
})
