import { defineComponent, watch, type PropType, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { Copy } from 'lucide-vue-next'
import { Button, Checkbox, Dialog, IdCell, cn } from '@/components/ui'
import { copyText } from '@/utils/clipboard'
import { formatDate, getLocalizedText } from '@/utils/format'
import { formatSkuDisplayLabel } from '@/utils/sku'
import { adminUrl } from '@/utils/adminBase'
import { refundTypeCode, useOrderRefundDetail } from '../useOrderRefunds'

const Tile = (p: { label: string; class?: string }, { slots }: { slots: { default?: () => VNodeChild } }) => (
  <div class={cn('min-w-0 rounded-zs border border-line bg-surface-strong p-4', p.class)}>
    <div class="mb-2 text-xs text-muted">{p.label}</div>
    {slots.default?.()}
  </div>
)

/** Order refund record detail; manual refunds can toggle "payment fee refunded". */
export default defineComponent({
  name: 'OrderRefundsDialog',
  props: {
    modelValue: Boolean,
    refundId: { type: Number as PropType<number | null>, default: null },
  },
  emits: { 'update:modelValue': (_v: boolean) => true, updated: () => true },
  setup(props, { emit }) {
    const { t, locale } = useI18n()
    const d = useOrderRefundDetail(() => emit('updated'))

    watch(
      [() => props.modelValue, () => props.refundId],
      ([open, id]) => {
        if (!open) d.reset()
        else void d.load(id)
      },
      { immediate: true },
    )

    const typeLabel = (code: string) => (code === 'wallet' ? t('admin.orderRefunds.typeWallet') : code === 'manual' ? t('admin.orderRefunds.typeManual') : code || '-')

    const renderBody = () => {
      const r = d.refund.value
      if (!r) return null
      const isManual = r.type === 'manual'
      return (
        <div class="space-y-4 text-sm text-muted">
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            <Tile label={t('admin.orderRefunds.detailRefundId')}>
              <IdCell value={r.id} />
            </Tile>
            <Tile label={t('admin.orderRefunds.detailOrderNo')}>
              <div class="flex items-center gap-1.5 font-mono text-sm text-fg">
                {r.order_id ? (
                  <a href={adminUrl(`/orders?order_id=${r.order_id}`)} target="_blank" rel="noopener" class="break-all text-accent underline-offset-4 hover:underline">
                    {r.order_no || `#${r.order_id}`}
                  </a>
                ) : (
                  <span>{r.order_no || '-'}</span>
                )}
                {r.order_no && (
                  <button
                    type="button"
                    title={t('admin.common.copy')}
                    class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-zs-sm border border-line text-muted hover:border-primary/40 hover:text-primary"
                    onClick={() => copyText(r.order_no || '').catch(() => undefined)}
                  >
                    <Copy class="h-3 w-3" />
                  </button>
                )}
              </div>
            </Tile>
            <Tile label={t('admin.orderRefunds.detailProductName')}>
              {r.items?.length ? (
                <div class="space-y-1">
                  {r.items.map((entry) => {
                    const sku = formatSkuDisplayLabel(entry.sku_snapshot, locale.value)
                    return (
                      <div key={entry.id} class="text-xs">
                        <span class="text-fg">{getLocalizedText(entry.title) || '-'}</span>
                        {sku && <span class="ml-1">({sku})</span>}
                        <span class="ml-1">x{entry.quantity}</span>
                      </div>
                    )
                  })}
                </div>
              ) : (
                <span class="text-xs">-</span>
              )}
            </Tile>
            <Tile label={t('admin.orderRefunds.detailType')}>
              <span class="text-fg">{typeLabel(refundTypeCode(r))}</span>
            </Tile>
            <Tile label={t('admin.orderRefunds.detailAmount')}>
              <span class="zs-num font-semibold text-fg">
                {r.amount} {r.currency}
              </span>
            </Tile>
            <Tile label={t('admin.orderRefunds.detailCreatedAt')}>
              <span class="text-fg">{formatDate(r.created_at)}</span>
            </Tile>
            <Tile label={t('admin.orderRefunds.detailPaymentFeeRefund')} class="space-y-3 md:col-span-2">
              {isManual ? (
                <Checkbox v-model={d.fee.refunded} disabled={d.fee.updating}>
                  <span class="block">
                    <span class="block text-sm font-medium text-fg">{t('admin.orderRefunds.paymentFeeRefunded')}</span>
                    <span class="mt-0.5 block text-xs text-muted">{t('admin.orderRefunds.paymentFeeRefundedHint')}</span>
                  </span>
                </Checkbox>
              ) : (
                <div class="text-sm">{t('admin.orderRefunds.paymentFeeNotRefunded')}</div>
              )}
              <div class="text-sm text-fg">
                {t('admin.orderRefunds.detailPaymentFeeRefundedAmount')}：
                <span class="font-mono">
                  {r.payment_fee_refunded_amount} {r.currency}
                </span>
              </div>
              {isManual && (
                <Button size="sm" variant="primary" loading={d.fee.updating} disabled={d.fee.refunded === r.payment_fee_refunded} onClick={d.updatePaymentFee}>
                  {d.fee.updating ? t('admin.orderRefunds.paymentFeeUpdating') : t('admin.orderRefunds.paymentFeeUpdate')}
                </Button>
              )}
              {d.fee.error && <div class="rounded-zs-sm border border-danger/35 bg-danger-soft p-2 text-xs text-danger-text">{d.fee.error}</div>}
              {d.fee.success && <div class="rounded-zs-sm border border-success/40 bg-success-soft p-2 text-xs text-success-text">{d.fee.success}</div>}
            </Tile>
          </div>
          <Tile label={t('admin.orderRefunds.detailRemark')}>
            <div class="min-h-[140px] whitespace-pre-wrap break-words rounded-zs-sm border border-line bg-surface-muted/60 p-4 text-sm text-fg">
              {r.remark || t('admin.orderRefunds.noRemark')}
            </div>
          </Tile>
        </div>
      )
    }

    return () => (
      <Dialog modelValue={props.modelValue} onUpdate:modelValue={(v) => emit('update:modelValue', v)} title={t('admin.orderRefunds.detailTitle')} size="xl">
        {{
          default: () =>
            d.loading.value ? (
              <div class="zs-skeleton h-32 rounded-zs" />
            ) : d.error.value ? (
              <div class="rounded-zs border border-danger/35 bg-danger-soft px-3 py-2 text-sm text-danger-text">{d.error.value}</div>
            ) : (
              renderBody()
            ),
        }}
      </Dialog>
    )
  },
})
