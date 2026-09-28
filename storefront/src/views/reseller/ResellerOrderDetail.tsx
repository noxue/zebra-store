import { computed, defineComponent, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, Check, RotateCw } from 'lucide-vue-next'
import type { ResellerOrderItemData } from '@/api/types'
import { useResellerOrders } from '@/composables/reseller/useResellerOrders'
import { useLocalized } from '@/composables/useLocalized'
import { ResellerPageHeader, ResellerPageState } from '@/components/reseller/ConsoleParts'
import { Badge, Button, Card, DataTable, columns, cn } from '@/components/ui'
import { formatResellerConsoleAmount, formatResellerConsoleDate, resellerOrderStatusTone, resellerProfitStatusKey, resellerProfitTone } from '@/utils/reseller/console'
import { buildSkuDisplayTextFromSnapshot } from '@/utils/sku'

export default defineComponent({
  name: 'ResellerOrderDetail',
  setup() {
    const { t, te, locale } = useI18n()
    const route = useRoute()
    const { getLocalizedText } = useLocalized()
    const o = useResellerOrders()
    const orderNo = computed(() => String(route.params.order_no || ''))
    const reload = () => {
      if (orderNo.value) void o.loadDetail(orderNo.value)
    }
    onMounted(reload)
    watch(orderNo, reload)

    const statusLabel = (s?: string) => {
      if (!s) return '-'
      const key = `resellerConsole.orders.statusMap.${s}`
      return te(key) ? t(key) : s
    }
    const timeline = computed(() => {
      const d = o.detail.value
      if (!d) return []
      const refunded = ['refunded', 'partially_refunded', 'canceled'].includes(d.status)
      const completed = d.status === 'completed' || d.status === 'delivered'
      return [
        { label: t('resellerConsole.orderDetail.tlCreated'), time: d.created_at, done: true },
        { label: t('resellerConsole.orderDetail.tlPaid'), time: d.paid_at || '', done: Boolean(d.paid_at) },
        refunded ? { label: t('resellerConsole.orderDetail.tlRefunded'), time: '', done: true } : { label: t('resellerConsole.orderDetail.tlCompleted'), time: '', done: completed },
      ]
    })
    const cur = computed(() => o.detail.value?.currency || '')
    const itemColumns = computed(() =>
      columns<ResellerOrderItemData>([
        {
          key: 'title',
          title: t('resellerConsole.orderDetail.product'),
          render: (row) => (
            <div>
              <div class="font-bold text-fg">{getLocalizedText(row.title) || '-'}</div>
              <div class="text-xs text-muted">{buildSkuDisplayTextFromSnapshot(row.sku_snapshot, { locale: String(locale.value), fallback: t('productDetail.skuFallback') })}</div>
            </div>
          ),
        },
        { key: 'quantity', title: t('resellerConsole.orderDetail.quantity'), align: 'center', render: (row) => <span class="whitespace-nowrap zs-num">× {row.quantity}</span> },
        { key: 'unit', title: t('resellerConsole.orderDetail.unitPrice'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num">{formatResellerConsoleAmount(row.unit_price, cur.value)}</span> },
        { key: 'base', title: t('resellerConsole.orderDetail.baseUnit'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num text-muted">{formatResellerConsoleAmount(row.base_unit_amount, cur.value)}</span> },
        { key: 'total', title: t('resellerConsole.orders.totalAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num font-bold">{formatResellerConsoleAmount(row.total_price, cur.value)}</span> },
        { key: 'profit', title: t('resellerConsole.orders.profitAmount'), align: 'right', render: (row) => <span class="whitespace-nowrap zs-num text-success-text">{formatResellerConsoleAmount(row.profit_amount, cur.value)}</span> },
      ]),
    )

    return () => {
      const d = o.detail.value
      return (
        <div class="space-y-5">
          <ResellerPageHeader title={t('resellerConsole.orderDetail.title')} description={t('resellerConsole.orderDetail.description')}>
            {{
              actions: () => (
                <>
                  <Button variant="secondary" size="sm" to="/reseller/orders">
                    <ArrowLeft class="size-4" />
                    {t('resellerConsole.orderDetail.back')}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={reload}>
                    <RotateCw class="size-4" />
                    {t('orders.filters.refresh')}
                  </Button>
                </>
              ),
            }}
          </ResellerPageHeader>
          {o.detailLoading.value ? (
            <ResellerPageState loading title={t('resellerConsole.common.loading')} />
          ) : !d ? (
            <ResellerPageState title={t('resellerConsole.orderDetail.notFound')} description={o.detailError.value}>
              <Button class="mt-3" size="sm" onClick={reload}>
                {t('resellerConsole.common.retry')}
              </Button>
            </ResellerPageState>
          ) : (
            <>
              <Card>
                <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                  <div class="min-w-0">
                    <div class="text-xs font-bold text-muted">{t('resellerConsole.orders.orderNo')}</div>
                    <div class="break-all font-mono text-lg font-bold text-fg">{d.order_no}</div>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <Badge tone={resellerOrderStatusTone(d.status)}>{statusLabel(d.status)}</Badge>
                    <Badge tone={resellerProfitTone(d.profit_status)}>{t(`resellerConsole.orders.profit.${resellerProfitStatusKey(d.profit_status)}`)}</Badge>
                  </div>
                </div>
                <dl class="mt-5 grid grid-cols-2 gap-4 text-sm md:grid-cols-4">
                  {[
                    [t('resellerConsole.orders.domain'), d.domain || '-'],
                    [t('resellerConsole.orders.buyer'), d.buyer_label || '-'],
                    [t('resellerConsole.orders.createdAt'), formatResellerConsoleDate(d.created_at)],
                    [t('resellerConsole.orders.paidAt'), formatResellerConsoleDate(d.paid_at)],
                  ].map(([label, value]) => (
                    <div key={label}>
                      <dt class="text-xs text-muted">{label}</dt>
                      <dd class="mt-0.5 break-all font-bold text-fg">{value}</dd>
                    </div>
                  ))}
                </dl>
              </Card>

              <div class="grid gap-5 lg:grid-cols-[1fr_320px]">
                <Card>
                  <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.orderDetail.profitSnapshot')}</h3>
                  <div class="grid gap-3 sm:grid-cols-3">
                    {[
                      { label: t('resellerConsole.orders.totalAmount'), value: d.total_amount, cls: 'text-fg' },
                      { label: t('resellerConsole.orders.baseAmount'), value: d.base_amount, cls: 'text-muted' },
                      { label: t('resellerConsole.orders.profitAmount'), value: d.profit_amount, cls: 'text-success-text' },
                    ].map((c) => (
                      <div key={c.label} class="rounded-zs border border-line bg-surface-strong p-4">
                        <div class="text-xs font-bold text-muted">{c.label}</div>
                        <div class={cn('zs-num mt-1 text-xl font-bold', c.cls)}>{formatResellerConsoleAmount(c.value, d.currency)}</div>
                      </div>
                    ))}
                  </div>
                </Card>
                <Card>
                  <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.orderDetail.timeline')}</h3>
                  <ol class="space-y-4">
                    {timeline.value.map((step, i) => (
                      <li key={i} class="flex items-start gap-3">
                        <span class={cn('flex size-7 shrink-0 items-center justify-center rounded-full text-xs font-bold', step.done ? 'zs-gradient-bg text-on-primary' : 'bg-surface-muted text-muted')}>
                          {step.done ? <Check class="size-4" /> : i + 1}
                        </span>
                        <div>
                          <div class={cn('text-sm font-bold', step.done ? 'text-fg' : 'text-muted')}>{step.label}</div>
                          <div class="text-xs text-muted">{step.time ? formatResellerConsoleDate(step.time) : step.done ? '' : t('resellerConsole.orderDetail.tlPending')}</div>
                        </div>
                      </li>
                    ))}
                  </ol>
                </Card>
              </div>

              <Card>
                <h3 class="zs-title mb-4 text-lg text-fg">{t('resellerConsole.orderDetail.items')}</h3>
                <DataTable columns={itemColumns.value} rows={d.items || []} rowKey={(_r: ResellerOrderItemData, i: number) => i} />
              </Card>
            </>
          )}
        </div>
      )
    }
  },
})
