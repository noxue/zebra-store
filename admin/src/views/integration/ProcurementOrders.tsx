import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Ban, Check, CheckCircle2, Clock, Download, HelpCircle, Layers, MoreHorizontal, Receipt, RefreshCw, Search, X } from 'lucide-vue-next'
import {
  Badge,
  Button,
  DateTimeInput,
  EmptyState,
  FilterBar,
  FormField,
  Input,
  ListPagination,
  PageHeader,
  Select,
  Sheet,
  StatCard,
  TableSkeleton,
  cn,
  type BadgeTone,
  type IconComponent,
} from '@/components/ui'
import { formatMoney, getLocalizedText, hasPositiveAmount } from '@/utils/format'
import { orderStatusLabel } from '@/utils/status'
import {
  PROCUREMENT_STATUSES,
  canCancelProcurement,
  canRetryProcurement,
  formatTime,
  localCostAmount,
  procurementProfit,
  procurementStatusTone,
  relativeTimeParts,
} from './integrationUtils'
import { useProcurementOrders, type ProcurementRow } from './useProcurementOrders'

const STATUS_ICON: Record<string, IconComponent> = {
  pending: Clock,
  accepted: CheckCircle2,
  manual_review: HelpCircle,
  rejected: AlertTriangle,
  failed: AlertTriangle,
  fulfilled: Check,
  completed: Check,
  partially_refunded: Receipt,
  refunded: Receipt,
  canceled: X,
}

const toneSurface: Record<BadgeTone, string> = {
  primary: 'bg-primary-soft text-primary border-primary/30',
  secondary: 'bg-secondary-soft text-secondary border-secondary/30',
  info: 'bg-accent-soft text-info-text border-accent/30',
  success: 'bg-success-soft text-success-text border-success/30',
  warning: 'bg-warning-soft text-warning-text border-warning/30',
  danger: 'bg-danger-soft text-danger-text border-danger/30',
  gold: 'bg-warning-soft text-warning-text border-warning/30',
  neutral: 'bg-surface-muted text-muted border-line',
}

export default defineComponent({
  name: 'ProcurementOrdersView',
  setup() {
    const { t, te } = useI18n()
    const p = useProcurementOrders()
    onMounted(p.init)

    const statusLabel = (s?: string) => (s && te(`procurement.status.${s}`) ? t(`procurement.status.${s}`) : s || '-')
    const relTime = (raw?: string) => {
      const r = relativeTimeParts(raw)
      return r ? t(`procurement.time.${r.key}`, { n: r.n }) : ''
    }
    const upstreamAmount = (o: ProcurementRow) =>
      o.upstream_amount && String(o.upstream_amount) !== '0.00' ? formatMoney(o.upstream_amount, o.upstream_currency || o.currency) : '-'
    const profit = (o: ProcurementRow) => {
      const v = procurementProfit(o)
      return { text: v !== null ? formatMoney(v, o.currency) : '-', cls: v === null ? '' : parseFloat(v) >= 0 ? 'text-success-text' : 'text-danger-text' }
    }
    const refundTypeLabel = (type: string) => {
      const n = type.trim().toLowerCase()
      if (n === 'manual') return t('admin.orderRefunds.typeManual')
      if (n === 'wallet') return t('admin.orderRefunds.typeWallet')
      return n || '-'
    }
    const statusIcon = (status: string, cls: string) => {
      const Icon = STATUS_ICON[status] ?? HelpCircle
      return <Icon class={cls} />
    }

    const statCards = () => {
      const s = p.stats.value
      const f = p.filters.status
      return (
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
          <div title={t('procurement.stats.totalHint')}>
            <StatCard label={t('procurement.stats.total')} value={s.total} icon={Layers} tone="primary" clickable active={f === '__all__'} onClick={() => p.selectStatus('__all__')} />
          </div>
          <StatCard label={t('procurement.stats.pending')} value={s.pending} icon={Clock} tone="gold" clickable active={f === 'pending'} onClick={() => p.selectStatus('pending')} />
          <StatCard label={t('procurement.stats.failed')} value={s.failed} icon={AlertTriangle} tone="danger" clickable active={f === 'failed'} onClick={() => p.selectStatus('failed')} />
          <StatCard label={t('procurement.stats.rejected')} value={s.rejected} icon={Ban} tone="secondary" clickable active={f === 'rejected'} onClick={() => p.selectStatus('rejected')} />
          <StatCard label={t('procurement.stats.fulfilled')} value={s.fulfilled} icon={CheckCircle2} tone="success" clickable active={f === 'fulfilled'} onClick={() => p.selectStatus('fulfilled')} />
          <div title={t('procurement.stats.otherHint')}>
            <StatCard label={t('procurement.stats.other')} value={s.other} icon={MoreHorizontal} tone="accent" />
          </div>
        </div>
      )
    }

    const infoCell = (label: string, value: unknown, extra?: unknown, valueClass = 'font-medium text-fg') => (
      <div>
        <span class="text-muted">{label}</span>
        <div class={cn('mt-0.5 break-all', valueClass)}>{value}</div>
        {extra}
      </div>
    )

    const orderCard = (o: ProcurementRow) => {
      const tone = procurementStatusTone(o.status)
      const bad = o.status === 'failed' || o.status === 'rejected'
      const cost = localCostAmount(o)
      const pr = profit(o)
      return (
        <div
          key={o.id}
          class={cn('zs-glass zs-card-hover cursor-pointer rounded-zs-lg p-4 shadow-zs-sm', bad && 'ring-1 ring-danger/30')}
          onClick={() => p.openDetail(o)}
        >
          <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div class="flex min-w-0 items-start gap-3">
              <div class={cn('flex h-9 w-9 shrink-0 items-center justify-center rounded-full border', toneSurface[tone])}>{statusIcon(o.status, 'h-4 w-4')}</div>
              <div class="min-w-0">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="break-words text-sm font-semibold text-fg">
                    {o.local_order?.items?.[0]?.title ? getLocalizedText(o.local_order.items[0].title) : o.local_order_no || '-'}
                  </span>
                  <Badge tone={tone}>{statusLabel(o.status)}</Badge>
                </div>
                <div class="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted">
                  <span class="font-mono">#{o.id}</span>
                  <span class="break-words">{o.connection?.name || '-'}</span>
                  <span>{relTime(o.created_at)}</span>
                </div>
              </div>
            </div>
            <div class="flex w-full flex-wrap items-center gap-2 sm:w-auto sm:justify-end" onClick={(e: Event) => e.stopPropagation()}>
              {canRetryProcurement(o.status) && (
                <Button size="xs" loading={p.retryingId.value === o.id} onClick={() => p.retry(o)}>
                  {t('procurement.actions.retry')}
                </Button>
              )}
              {o.has_held_delivery && <Button size="xs" variant="primary" loading={p.retryingDeliveryId.value === o.id} onClick={() => p.retryDelivery(o)}>{t('procurement.actions.retryDelivery')}</Button>}
              {canCancelProcurement(o.status) && (
                <Button size="xs" variant="ghost" loading={p.cancelingId.value === o.id} onClick={() => p.cancel(o)}>
                  {t('procurement.actions.cancel')}
                </Button>
              )}
            </div>
          </div>

          <div class="mt-3 grid grid-cols-2 gap-x-6 gap-y-1.5 text-xs md:grid-cols-4 lg:grid-cols-6">
            {infoCell(
              t('procurement.columns.localOrderNo'),
              o.local_order_no || '-',
              o.parent_order_no && (
                <div class="mt-0.5 text-[10px] text-muted">
                  {t('procurement.columns.parentOrderNo')}: <span class="font-mono">{o.parent_order_no}</span>
                </div>
              ),
              'font-mono text-fg',
            )}
            {infoCell(t('procurement.columns.upstreamOrderNo'), o.upstream_order_no || '-', null, 'font-mono text-fg')}
            {infoCell(t('procurement.columns.localSellAmount'), formatMoney(o.local_sell_amount, o.currency), null, 'zs-num font-medium text-fg')}
            {infoCell(t('procurement.columns.upstreamAmount'), upstreamAmount(o), null, 'zs-num font-medium text-fg')}
            {cost !== null && infoCell(t('procurement.columns.localCost'), formatMoney(cost, o.currency), null, 'zs-num font-medium text-fg')}
            {infoCell(t('procurement.detail.profit'), pr.text, null, cn('zs-num font-medium', pr.cls))}
            {infoCell(t('procurement.columns.retryCount'), o.retry_count ?? 0, null, 'text-fg')}
          </div>

          {o.error_message && (
            <div class="mt-3 flex items-start gap-2 rounded-zs border border-danger/30 bg-danger-soft px-3 py-2">
              <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0 text-danger-text" />
              <span class="break-words text-xs text-danger-text">{o.error_message}</span>
            </div>
          )}
        </div>
      )
    }

    const section = (title: string, body: unknown, extra?: unknown, danger = false) => (
      <div class={cn('overflow-hidden rounded-zs border', danger ? 'border-danger/30' : 'border-line')}>
        <div
          class={cn(
            'flex items-center justify-between gap-2 border-b px-4 py-2 text-xs font-semibold uppercase',
            danger ? 'border-danger/30 bg-danger-soft text-danger-text' : 'border-line bg-surface-muted text-muted',
          )}
        >
          <span>{title}</span>
          {extra}
        </div>
        {body}
      </div>
    )

    const kv = (label: string, value: unknown, valueClass = 'text-sm font-medium') => (
      <div>
        <div class="text-xs text-muted">{label}</div>
        <div class={cn('mt-1 break-all', valueClass)}>{value}</div>
      </div>
    )

    const moneyBox = (label: string, value: string, cls = '') => (
      <div class="rounded-zs border border-line bg-surface-muted p-4 text-center">
        <div class="text-xs text-muted">{label}</div>
        <div class={cn('zs-num mt-1 text-lg font-bold', cls)}>{value}</div>
      </div>
    )

    const timelineRow = (label: string, value: unknown) => (
      <div class="flex flex-col gap-1 text-xs sm:flex-row sm:items-start sm:gap-3">
        <div class="w-full shrink-0 text-muted sm:w-32">{label}</div>
        <div class="break-all">{value}</div>
      </div>
    )

    const detailBody = () => {
      const d = p.detail.value
      if (!d) return null
      const tone = procurementStatusTone(d.status)
      const cost = localCostAmount(d)
      const pr = profit(d)
      const lineCount = d.upstream_payload_line_count ?? 0
      return (
        <div class="space-y-5 p-5">
          <div class={cn('flex items-center gap-3 rounded-zs border px-4 py-3', toneSurface[tone])}>
            {statusIcon(d.status, 'h-5 w-5')}
            <div>
              <div class="text-sm font-semibold">{statusLabel(d.status)}</div>
              {d.error_message && <div class="mt-0.5 text-xs opacity-80">{d.error_message}</div>}
              {d.status === 'manual_review' && <div class="mt-1 text-xs opacity-80">{t('procurement.manualReviewHint')}</div>}
            </div>
            {p.detailLoading.value && <RefreshCw class="ml-auto h-4 w-4 animate-spin opacity-60" />}
          </div>

          {section(
            t('procurement.detail.orderInfo'),
            <div class="grid grid-cols-1 gap-4 p-4 sm:grid-cols-2">
              <div>
                {kv(t('procurement.columns.localOrderNo'), d.local_order_no || '-', 'font-mono text-sm font-medium')}
                {d.parent_order_no && (
                  <div class="mt-1 text-xs text-muted">
                    {t('procurement.columns.parentOrderNo')}: <span class="font-mono">{d.parent_order_no}</span>
                  </div>
                )}
              </div>
              {kv(t('procurement.columns.upstreamOrderNo'), d.upstream_order_no || '-', 'font-mono text-sm font-medium')}
              {kv(t('procurement.columns.connection'), d.connection?.name || d.connection_id || '-')}
              {kv(t('procurement.detail.traceId'), d.trace_id || '-', 'font-mono text-sm text-muted')}
              {kv(t('procurement.detail.currency'), d.currency || '-', 'text-sm')}
            </div>,
          )}

          {section(
            t('procurement.detail.financial'),
            <div class="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2 xl:grid-cols-3">
              {moneyBox(t('procurement.columns.localSellAmount'), formatMoney(d.local_sell_amount, d.currency))}
              {moneyBox(t('procurement.columns.upstreamAmount'), upstreamAmount(d))}
              {cost !== null && moneyBox(t('procurement.columns.localCost'), formatMoney(cost, d.currency))}
              {hasPositiveAmount(d.upstream_refunded_amount) &&
                moneyBox(t('procurement.detail.upstreamRefunded'), formatMoney(d.upstream_refunded_amount, d.upstream_currency || d.currency), 'text-danger-text')}
              {hasPositiveAmount(d.local_order?.refunded_amount) &&
                moneyBox(t('procurement.detail.localRefunded'), formatMoney(d.local_order?.refunded_amount, d.currency), 'text-danger-text')}
              {moneyBox(t('procurement.detail.profit'), pr.text, pr.cls)}
            </div>,
          )}

          {p.refundRecords.value.length > 0 &&
            section(
              t('procurement.detail.upstreamRefundRecords'),
              <div class="overflow-x-auto">
                <table class="min-w-full divide-y divide-line text-sm">
                  <thead class="bg-surface-muted text-xs text-muted">
                    <tr>
                      <th class="px-4 py-2 text-left">ID</th>
                      <th class="px-4 py-2 text-left">{t('admin.orderRefunds.table.refundType')}</th>
                      <th class="px-4 py-2 text-left">{t('admin.orderRefunds.table.amount')}</th>
                      <th class="px-4 py-2 text-left">{t('admin.orderRefunds.table.createdAt')}</th>
                      <th class="px-4 py-2 text-left">{t('admin.orderRefunds.detailRemark')}</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-line">
                    {p.refundRecords.value.map((r) => (
                      <tr key={r.id}>
                        <td class="px-4 py-2 font-mono">{r.id}</td>
                        <td class="px-4 py-2">{refundTypeLabel(r.type)}</td>
                        <td class="zs-num px-4 py-2 font-mono">{r.amount ? formatMoney(r.amount, r.currency || d.upstream_currency || d.currency) : '-'}</td>
                        <td class="px-4 py-2">{formatTime(r.createdAt)}</td>
                        <td class="px-4 py-2">{r.remark || '-'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>,
            )}

          {d.local_order &&
            section(
              t('procurement.detail.localOrder'),
              <div class="space-y-2 p-4">
                {(d.local_order.items || []).map((item, idx) => (
                  <div key={idx} class="flex flex-col gap-2 text-sm sm:flex-row sm:items-center sm:gap-3">
                    <div class="min-w-0 flex-1">
                      <span class="break-words font-medium">{getLocalizedText(item.title)}</span>
                      {item.sku_snapshot?.sku_code && <span class="mt-1 block text-xs text-muted sm:ml-2 sm:mt-0 sm:inline">SKU: {item.sku_snapshot.sku_code}</span>}
                    </div>
                    <span class="text-muted">x{item.quantity}</span>
                    <span class="zs-num font-mono">{formatMoney(item.total_amount, d.currency)}</span>
                  </div>
                ))}
                <div class="flex flex-col gap-1 border-t border-line pt-1 text-xs text-muted sm:flex-row sm:flex-wrap sm:items-center sm:gap-4">
                  <span>
                    {t('procurement.detail.orderStatus')}: {orderStatusLabel(t, d.local_order.status)}
                  </span>
                  {d.local_order.user_email && <span class="break-all">{d.local_order.user_email}</span>}
                </div>
              </div>,
            )}

          {section(
            t('procurement.detail.timeline'),
            <div class="space-y-3 p-4">
              {timelineRow(t('procurement.columns.createdAt'), formatTime(d.created_at))}
              {timelineRow(t('procurement.detail.updatedAt'), formatTime(d.updated_at))}
              {d.next_retry_at && timelineRow(t('procurement.detail.nextRetryAt'), formatTime(d.next_retry_at))}
              {timelineRow(t('procurement.columns.retryCount'), d.retry_count ?? 0)}
            </div>,
          )}

          {d.error_message &&
            section(t('procurement.columns.errorMessage'), <div class="whitespace-pre-wrap break-all p-4 font-mono text-sm text-danger-text">{d.error_message}</div>, null, true)}

          {d.upstream_payload &&
            section(
              t('procurement.detail.upstreamPayload'),
              <div class="max-h-48 overflow-y-auto whitespace-pre-wrap break-all p-4 font-mono text-xs text-muted">{d.upstream_payload}</div>,
              lineCount > 100 && (
                <div class="flex items-center gap-2 normal-case">
                  <span class="text-xs font-normal">{t('admin.orders.fulfillmentTotalLines', { count: lineCount })}</span>
                  <Button size="xs" loading={p.downloading.value} onClick={() => p.downloadPayload(d.id)}>
                    <Download class="h-3 w-3" />
                    {p.downloading.value ? t('admin.orders.fulfillmentDownloading') : t('admin.orders.fulfillmentDownload')}
                  </Button>
                </div>
              ),
            )}
        </div>
      )
    }

    const detailFooter = () => {
      const d = p.detail.value
      if (!d) return null
      return (
        <div class="flex flex-col-reverse gap-3 sm:flex-row sm:justify-end">
          {canRetryProcurement(d.status) && (
            <Button loading={p.retryingId.value === d.id} onClick={() => p.retry(d)}>
              {t('procurement.actions.retry')}
            </Button>
          )}
          {d.has_held_delivery && <Button variant="primary" loading={p.retryingDeliveryId.value === d.id} onClick={() => p.retryDelivery(d)}>{t('procurement.actions.retryDelivery')}</Button>}
          {canCancelProcurement(d.status) && (
            <Button variant="danger" loading={p.cancelingId.value === d.id} onClick={() => p.cancel(d)}>
              {t('procurement.actions.cancelOrder')}
            </Button>
          )}
          <Button onClick={() => (p.showDetail.value = false)}>{t('admin.common.cancel')}</Button>
        </div>
      )
    }

    const listBody = () => {
      const items = p.list.items.value
      if (p.list.loading.value && items.length === 0)
        return (
          <div class="zs-glass overflow-hidden rounded-zs-lg">
            <TableSkeleton cols={6} rows={5} />
          </div>
        )
      if (items.length === 0)
        return (
          <div class="zs-glass rounded-zs-lg">
            <EmptyState title={t('procurement.empty')} />
          </div>
        )
      return items.map(orderCard)
    }

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('procurement.title')} subtitle={t('procurement.subtitle')}>
          {{
            actions: () => (
              <Button loading={p.refreshing.value} onClick={p.refresh}>
                <RefreshCw class="h-4 w-4" />
                {t('admin.common.refresh')}
              </Button>
            ),
          }}
        </PageHeader>

        {statCards()}

        <FilterBar cols={4}>
          {{
            default: () => (
              <>
                <FormField label={t('procurement.columns.status')}>
                  <Select
                    v-model={p.filters.status}
                    onChange={p.search}
                    options={[
                      { label: t('procurement.filters.allStatus'), value: '__all__' },
                      ...PROCUREMENT_STATUSES.map((s) => ({ label: t(`procurement.status.${s}`), value: s })),
                    ]}
                  />
                </FormField>
                <FormField label={t('procurement.columns.connection')}>
                  <Select
                    v-model={p.filters.connection_id}
                    onChange={p.search}
                    options={[
                      { label: t('procurement.filters.allConnections'), value: '__all__' },
                      ...p.connections.value.map((c) => ({ label: c.name || `#${c.id}`, value: c.id })),
                    ]}
                  />
                </FormField>
                <FormField label={t('procurement.filters.orderNo')}>
                  <Input
                    icon={Search}
                    v-model={p.filters.order_no}
                    placeholder={t('procurement.filters.orderNoPlaceholder')}
                    onUpdate:modelValue={p.debouncedSearch}
                    onEnter={p.search}
                  />
                </FormField>
                <FormField label={t('procurement.filters.upstreamOrderNo')}>
                  <Input
                    v-model={p.filters.upstream_order_no}
                    placeholder={t('procurement.filters.upstreamOrderNoPlaceholder')}
                    onUpdate:modelValue={p.debouncedSearch}
                    onEnter={p.search}
                  />
                </FormField>
                <FormField label={`${t('procurement.filters.dateRange')} · ${t('admin.zebraIntegration.from')}`}>
                  <DateTimeInput v-model={p.filters.created_from} onUpdate:modelValue={p.search} />
                </FormField>
                <FormField label={`${t('procurement.filters.dateRange')} · ${t('admin.zebraIntegration.to')}`}>
                  <DateTimeInput v-model={p.filters.created_to} onUpdate:modelValue={p.search} />
                </FormField>
              </>
            ),
            actions: () => (
              <Button size="sm" variant="primary" onClick={p.search}>
                <Search class="h-3.5 w-3.5" />
                {t('procurement.filters.search')}
              </Button>
            ),
          }}
        </FilterBar>

        <div class="space-y-3">{listBody()}</div>
        <ListPagination pagination={p.list.pagination.value} onChangePage={p.list.changePage} onChangePageSize={p.list.changePageSize} />

        <Sheet v-model={p.showDetail.value} title={`${t('procurement.detail.title')}${p.detail.value ? ` #${p.detail.value.id}` : ''}`} width="w-[min(96vw,760px)]">
          {{ default: detailBody, footer: detailFooter }}
        </Sheet>
      </div>
    )
  },
})
