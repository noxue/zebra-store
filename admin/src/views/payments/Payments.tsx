import { defineComponent, onMounted, watch, type VNodeChild } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Copy, Download, RefreshCw } from 'lucide-vue-next'
import { Badge, Button, cn, DataTable, Dialog, FilterBar, IdCell, Input, ListPagination, PageHeader, RangeFilter, Select, type DataTableColumn } from '@/components/ui'
import type { AdminPayment } from '@/api/types'
import { formatDate } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import { copyText } from '@/utils/clipboard'
import { PAYMENT_STATUSES, paymentStatusLabel, paymentStatusTone } from '@/utils/status'
import { formatAmountWithCurrency, formatFeeRate, formatPayload, interactionModeLabel, paymentChannelTypeLabel, providerTypeLabel } from './paymentLabels'
import { usePayments } from './usePayments'

const linkClass = 'text-accent underline-offset-4 hover:underline'

/** Label + value tile used in the detail dialog. */
function Tile(props: { label: string; tone?: 'warning'; class?: string }, { slots }: { slots: { default?: () => VNodeChild } }) {
  return (
    <div
      class={cn(
        'min-w-0 rounded-zs border p-4',
        props.tone === 'warning' ? 'border-line bg-warning-soft' : 'border-line bg-surface-muted',
        props.class,
      )}
    >
      <div class={cn('mb-2 text-xs', props.tone === 'warning' ? 'text-warning-text' : 'text-muted')}>{props.label}</div>
      <div class={cn('break-all text-sm', props.tone === 'warning' ? 'text-warning-text' : 'text-fg')}>{slots.default?.()}</div>
    </div>
  )
}

export default defineComponent({
  name: 'PaymentsView',
  setup() {
    const { t, te } = useI18n()
    const route = useRoute()
    const p = usePayments()
    const { filters, list } = p

    const statusLabel = (s?: string) => paymentStatusLabel(t, s)
    const translatedOr = (prefix: string, value?: string) => {
      if (!value) return '-'
      const key = `${prefix}.${value}`
      return te(key) ? t(key) : value
    }

    const orderLink = (id: number) => adminUrl(`/orders?order_id=${id}`)
    const userLink = (id: number) => adminUrl(`/users/${id}`)
    const channelLink = (id: number) => adminUrl(`/payment-channels?channel_id=${id}`)

    const openFromQuery = (raw: unknown) => {
      const id = Number(raw)
      if (Number.isFinite(id) && id > 0) void p.openDetail(id)
    }

    onMounted(() => {
      void p.fetchFilterOptions()
      if (route.query.user_id) filters.userId = String(route.query.user_id)
      void list.fetchData(1)
      openFromQuery(route.query.payment_id)
    })
    watch(
      () => route.query.user_id,
      (v) => {
        if (v) {
          filters.userId = String(v)
          void list.fetchData(1)
        }
      },
    )
    watch(() => route.query.payment_id, openFromQuery)

    const rechargeInfo = (r: AdminPayment) =>
      r.recharge_no ? (
        <div class="mt-1 space-y-0.5 text-xs text-muted">
          <div>
            {t('admin.payments.rechargeUser')}:{' '}
            {r.recharge_user_id ? (
              <a href={userLink(r.recharge_user_id)} target="_blank" rel="noopener" class={cn('font-mono', linkClass)}>
                #{r.recharge_user_id}
              </a>
            ) : (
              '-'
            )}
          </div>
          {r.recharge_status && (
            <div>
              {t('admin.payments.rechargeStatus')}: {statusLabel(r.recharge_status)}
            </div>
          )}
        </div>
      ) : null

    const columns = (): DataTableColumn<AdminPayment>[] => [
      { key: 'id', title: t('admin.payments.table.paymentId'), render: (r) => <IdCell value={r.id} /> },
      {
        key: 'order',
        title: t('admin.payments.table.orderId'),
        class: 'min-w-[160px]',
        render: (r) => (
          <div>
            {r.order_id ? (
              <a href={orderLink(r.order_id)} target="_blank" rel="noopener" class={linkClass}>
                #{r.order_id}
              </a>
            ) : r.recharge_no ? (
              <div class="break-all font-mono text-sm">{r.recharge_no}</div>
            ) : (
              '-'
            )}
            {rechargeInfo(r)}
          </div>
        ),
      },
      {
        key: 'channel',
        title: t('admin.payments.table.channel'),
        class: 'min-w-[180px] text-xs',
        render: (r) => (
          <div class="space-y-0.5">
            <div class="break-words text-sm text-fg">{r.channel_name || '-'}</div>
            <div class="break-words text-muted">
              {providerTypeLabel(t, r.provider_type)} / {paymentChannelTypeLabel(t, r)}
            </div>
            <div class="text-muted">
              {t('admin.payments.channelId')}:{' '}
              {r.channel_id ? (
                <a href={channelLink(r.channel_id)} target="_blank" rel="noopener" class={linkClass}>
                  #{r.channel_id}
                </a>
              ) : (
                '-'
              )}
            </div>
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.payments.table.status'),
        render: (r) => (
          <Badge tone={paymentStatusTone(r.status)} dot>
            {statusLabel(r.status)}
          </Badge>
        ),
      },
      { key: 'amount', title: t('admin.payments.table.amount'), class: 'zs-num whitespace-nowrap', render: (r) => formatAmountWithCurrency(r.amount, r.currency) },
      { key: 'feeRate', title: t('admin.payments.table.feeRate'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatFeeRate(r.fee_rate, r.fixed_fee) },
      {
        key: 'feeAmount',
        title: t('admin.payments.table.feeAmount'),
        class: 'zs-num whitespace-nowrap',
        render: (r) => formatAmountWithCurrency(r.fee_amount, r.currency),
      },
      { key: 'createdAt', title: t('admin.payments.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (r) => formatDate(r.created_at) },
      {
        key: 'action',
        title: t('admin.payments.table.action'),
        align: 'right',
        render: (r) => (
          <Button size="sm" onClick={() => p.openDetail(r.id)}>
            {t('admin.payments.view')}
          </Button>
        ),
      },
    ]

    const renderDetail = (d: AdminPayment) => (
      <div class="space-y-4">
        <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <Tile label={t('admin.payments.detailPaymentId')}>
            <IdCell value={d.id} />
            {d.payment_no && <div class="mt-1 font-mono text-xs text-muted">{d.payment_no}</div>}
          </Tile>
          <Tile label={t('admin.payments.detailOrderId')}>
            {d.order_id ? (
              <span class="inline-flex items-center gap-1.5">
                <a href={orderLink(d.order_id)} target="_blank" rel="noopener" class={cn('break-all font-mono', linkClass)}>
                  {d.order_no || `#${d.order_id}`}
                </a>
                {d.order_no && (
                  <button
                    type="button"
                    title={t('admin.common.copy')}
                    class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-zs-sm border border-line text-muted hover:border-line-strong hover:text-fg"
                    onClick={() => copyText(d.order_no || '').catch(() => undefined)}
                  >
                    <Copy class="h-3 w-3" />
                  </button>
                )}
              </span>
            ) : d.recharge_no ? (
              <span class="font-mono">{d.recharge_no}</span>
            ) : (
              '-'
            )}
            {rechargeInfo(d)}
          </Tile>
          <Tile label={t('admin.payments.detailChannel')}>
            <div class="break-words">{d.channel_name || '-'}</div>
            <div class="mt-1 text-xs text-muted">
              {providerTypeLabel(t, d.provider_type)} / {paymentChannelTypeLabel(t, d)}
            </div>
          </Tile>
          <Tile label={t('admin.payments.channelId')}>
            {d.channel_id ? (
              <a href={channelLink(d.channel_id)} target="_blank" rel="noopener" class={cn('font-mono', linkClass)}>
                #{d.channel_id}
              </a>
            ) : (
              '-'
            )}
          </Tile>
          <Tile label={t('admin.payments.detailStatus')}>
            <Badge tone={paymentStatusTone(d.status)} dot>
              {statusLabel(d.status)}
            </Badge>
          </Tile>
          <Tile label={t('admin.payments.detailAmount')}>
            <span class="zs-num">{formatAmountWithCurrency(d.amount, d.currency)}</span>
          </Tile>
          <Tile label={t('admin.payments.detailPayableAmount')}>
            <span class="zs-num">{formatAmountWithCurrency(d.payable_amount, d.currency)}</span>
          </Tile>
          <Tile label={t('admin.payments.detailFeeRate')}>
            <span class="zs-num">{formatFeeRate(d.fee_rate, d.fixed_fee)}</span>
          </Tile>
          <Tile label={t('admin.payments.detailFeeAmount')}>
            <span class="zs-num">{formatAmountWithCurrency(d.fee_amount, d.currency)}</span>
          </Tile>
          <Tile label={t('admin.payments.detailFeePolicy')}>{translatedOr('admin.payments.feePolicies', d.fee_policy)}</Tile>
          {d.exception_code && (
            <Tile label={t('admin.payments.detailException')} tone="warning">
              {translatedOr('admin.payments.exceptions', d.exception_code)}
            </Tile>
          )}
          <Tile label={t('admin.payments.detailInteraction')}>{interactionModeLabel(t, d.interaction_mode)}</Tile>
          {(d.superseded_at || d.superseded_by_payment_id) && (
            <Tile label={t('admin.payments.detailSupersededBy')} tone="warning">
              {d.superseded_by_payment_id ? (
                <a href={adminUrl(`/payments?payment_id=${d.superseded_by_payment_id}`)} class={cn('font-mono', linkClass)}>
                  #{d.superseded_by_payment_id}
                </a>
              ) : (
                '-'
              )}
              <div class="mt-1 text-xs">
                {t('admin.payments.detailSupersededAt')}: {formatDate(d.superseded_at)}
              </div>
            </Tile>
          )}
        </div>

        <div class="grid grid-cols-1 gap-3 lg:grid-cols-3">
          <Tile label={t('admin.payments.detailCreatedAt')}>{formatDate(d.created_at)}</Tile>
          <Tile label={t('admin.payments.detailPaidAt')}>{formatDate(d.paid_at)}</Tile>
          <Tile label={t('admin.payments.detailExpiredAt')}>{formatDate(d.expired_at)}</Tile>
        </div>

        <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <Tile label={t('admin.payments.detailProviderRef')}>{d.provider_ref || '-'}</Tile>
          <Tile label={t('admin.payments.detailProviderTradeNo')}>
            <span class="font-mono">{d.provider_trade_no || '-'}</span>
          </Tile>
        </div>
        {d.pay_url && <Tile label={t('admin.payments.detailPayUrl')}>{d.pay_url}</Tile>}
        {d.qr_code && <Tile label={t('admin.payments.detailQrCode')}>{d.qr_code}</Tile>}
        <Tile label={t('admin.payments.detailPayload')}>
          <pre class="max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-zs-sm border border-line bg-surface-solid p-3 font-mono text-xs text-muted">
            {formatPayload(d.provider_payload)}
          </pre>
        </Tile>
      </div>
    )

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.payments.title')} />

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <Input v-model={filters.userId} placeholder={t('admin.payments.filterUserId')} onUpdate:modelValue={list.debouncedSearch} onEnter={list.handleSearch} />
                <Input v-model={filters.orderId} placeholder={t('admin.payments.filterOrderId')} onUpdate:modelValue={list.debouncedSearch} onEnter={list.handleSearch} />
                <Input v-model={filters.channelId} placeholder={t('admin.payments.filterChannelId')} onUpdate:modelValue={list.debouncedSearch} onEnter={list.handleSearch} />
                <Select v-model={filters.status} options={[
                  { label: t('admin.payments.filterStatusAll'), value: '__all__' },
                  ...PAYMENT_STATUSES.map((s) => ({ label: statusLabel(s), value: s })),
                ]} onChange={list.handleSearch} />
                <Select v-model={filters.providerType} options={p.providerOptions.value} onChange={p.onProviderChange} />
                <Select v-model={filters.channelType} options={p.channelTypeOptions.value} onChange={list.handleSearch} />
                <RangeFilter
                  label={t('admin.payments.filterCreatedRange')}
                  from={filters.createdFrom}
                  to={filters.createdTo}
                  fromPlaceholder={t('admin.payments.filterCreatedFrom')}
                  toPlaceholder={t('admin.payments.filterCreatedTo')}
                  onUpdate:from={(v: string) => (filters.createdFrom = v)}
                  onUpdate:to={(v: string) => (filters.createdTo = v)}
                  onChange={list.handleSearch}
                />
              </>
            ),
            actions: () => (
              <>
                <Button size="sm" loading={p.refreshing.value} onClick={p.refresh}>
                  <RefreshCw class="h-3.5 w-3.5" />
                  {t('admin.common.refresh')}
                </Button>
                <Button size="sm" variant="primary" loading={p.exporting.value} onClick={p.handleExport}>
                  <Download class="h-3.5 w-3.5" />
                  {p.exporting.value ? t('admin.payments.exporting') : t('admin.payments.export')}
                </Button>
              </>
            ),
          }}
        </FilterBar>

        {p.exportError.value && <div class="rounded-zs border border-line bg-danger-soft p-3 text-sm text-danger-text">{p.exportError.value}</div>}

        <div>
          <DataTable
            columns={columns()}
            rows={list.items.value}
            rowKey={(r) => r.id}
            loading={list.loading.value}
            emptyText={t('admin.payments.empty')}
            minWidth="1000px"
          />
          <ListPagination pagination={list.pagination.value} onChangePage={list.changePage} onChangePageSize={list.changePageSize} />
        </div>

        <Dialog
          modelValue={p.showDetail.value}
          onUpdate:modelValue={(v: boolean) => !v && p.closeDetail()}
          title={t('admin.payments.detailTitle')}
          size="2xl"
        >
          {{
            default: () =>
              p.detailLoading.value ? (
                <div class="h-32 animate-pulse rounded-zs border border-line bg-surface-muted" />
              ) : p.detailError.value ? (
                <div class="rounded-zs border border-line bg-danger-soft p-3 text-sm text-danger-text">{p.detailError.value}</div>
              ) : p.detail.value ? (
                renderDetail(p.detail.value)
              ) : null,
          }}
        </Dialog>
      </div>
    )
  },
})
