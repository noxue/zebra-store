import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check, Clock, Copy, Download, Info, Layers, PackageCheck, ReceiptText, RotateCcw, ShoppingBag, Wallet } from 'lucide-vue-next'
import type { Fulfillment, Order, OrderItem } from '@/api/types'
import { OrderStatusBadge } from '@/components/common/OrderStatusBadge'
import { Badge, Button, Card, CardHeader, DataTable, SmartImage, columns } from '@/components/ui'
import type { OrderDisplayHelpers } from '@/composables/useOrderDisplayHelpers'
import type { RefundRecord } from '@/api/types'
import { InfoTile } from './AmountRow'

const itemsProps = {
  items: { type: Array as PropType<OrderItem[]>, default: () => [] },
  currency: { type: String, default: '' },
  helpers: { type: Object as PropType<OrderDisplayHelpers>, required: true },
  compact: Boolean,
} as const

/** Order line items with discount breakdown and manual form submission. */
export const OrderItemsList = defineComponent({
  name: 'OrderItemsList',
  props: itemsProps,
  setup(props) {
    const { t } = useI18n()
    return () => {
      const h = props.helpers
      const cur = props.currency
      if (props.items.length === 0) return <div class="text-sm text-muted">{t('orderDetail.noItems')}</div>
      return (
        <div class="divide-y divide-line">
          {props.items.map((item, idx) => {
            const sku = h.orderItemSkuText(item)
            const rows = h.manualRows(item)
            return (
              <div key={idx} class="flex flex-col gap-3 py-4 first:pt-0 last:pb-0 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
                <div class="flex min-w-0 items-start gap-3">
                  <div class="size-14 shrink-0 overflow-hidden rounded-zs-sm border border-line">
                    <SmartImage src={h.orderItemImage(item)} alt={h.getLocalizedText(item.title)} />
                  </div>
                  <div class="min-w-0">
                    <div class="font-bold text-fg">{h.getLocalizedText(item.title)}</div>
                    <div class="mt-1 text-xs text-muted">
                      {t('orderDetail.quantityLabel')}：<span class="zs-num">{item.quantity}</span>
                    </div>
                    {sku && (
                      <div class="mt-0.5 text-xs text-muted">
                        {t('orderDetail.itemSkuLabel')}：{sku}
                      </div>
                    )}
                    <div class="mt-0.5 text-xs text-muted">
                      {t('orderDetail.itemFulfillmentLabel')}：{h.fulfillmentTypeText(item.fulfillment_type)}
                    </div>
                    {item.tags && item.tags.length > 0 && (
                      <div class="mt-2 flex flex-wrap gap-1.5">
                        {item.tags.map((tag) => (
                          <Badge key={tag} size="xs">
                            {tag}
                          </Badge>
                        ))}
                      </div>
                    )}
                    {rows.length > 0 && (
                      <div class="mt-3 rounded-zs-sm border border-line bg-surface-muted p-3 text-xs text-muted">
                        <div class="mb-1.5 font-bold">{t('orderDetail.manualSubmissionTitle')}</div>
                        {rows.map((row) => (
                          <div key={row.key} class="mb-1 last:mb-0">
                            <span class="font-bold text-fg">{row.label}</span>：{row.value}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
                <div class="zs-num shrink-0 space-y-1 pl-[4.25rem] text-left text-xs text-muted sm:pl-0 sm:text-right">
                  <div>
                    {t('orderDetail.unitPriceLabel')}：{h.money(item.original_unit_price ?? item.unit_price, cur)}
                  </div>
                  <div>
                    {t('orderDetail.totalPriceLabel')}：{h.money(item.original_total_price ?? item.total_price, cur)}
                  </div>
                  {h.hasPositive(item.coupon_discount_amount) && (
                    <div>
                      {t('orderDetail.couponDiscountLabel')}：{h.discountMoney(item.coupon_discount_amount, cur)}
                    </div>
                  )}
                  {h.hasPositive(item.promotion_discount_amount) && (
                    <div>
                      {t('orderDetail.promotionDiscountLabel')}：{h.discountMoney(item.promotion_discount_amount, cur)}
                    </div>
                  )}
                  {!props.compact && h.hasPositive(item.wholesale_discount_amount) && (
                    <div>
                      {t('orderDetail.wholesaleDiscountLabel')}：{h.discountMoney(item.wholesale_discount_amount, cur)}
                    </div>
                  )}
                  {h.hasPositive(item.member_discount_amount) && (
                    <div>
                      {t('orderDetail.memberDiscountLabel')}：{h.discountMoney(item.member_discount_amount, cur)}
                    </div>
                  )}
                  {h.hasPositive(h.itemDiscountTotal(item)) && (
                    <div class="font-bold text-danger-text">
                      {t('orderDetail.itemDiscountTotalLabel')}：{h.money(h.itemDiscountTotal(item), cur)}
                    </div>
                  )}
                  <div class="text-sm font-bold text-fg">
                    {t('orderDetail.itemPaidAmountLabel')}：{h.money(h.itemPaidAmount(item), cur)}
                  </div>
                </div>
              </div>
            )
          })}
        </div>
      )
    }
  },
})

/** Fulfillment payload / delivery lines / instructions with copy & download. */
export const FulfillmentBlock = defineComponent({
  name: 'FulfillmentBlock',
  props: {
    fulfillment: { type: Object as PropType<Fulfillment>, required: true },
    items: { type: Array as PropType<OrderItem[]>, default: () => [] },
    orderNo: { type: String, required: true },
    helpers: { type: Object as PropType<OrderDisplayHelpers>, required: true },
    downloading: Boolean,
    small: Boolean,
  },
  emits: { download: (_no: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => {
      const h = props.helpers
      const f = props.fulfillment
      const truncated = h.isFulfillmentTruncated(f)
      const lines = h.fulfillmentDeliveryLines(f)
      const blocks = f.status === 'delivered' ? h.instructionBlocks(props.items) : []
      const size = props.small ? 'xs' : 'sm'
      return (
        <div class="space-y-3">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex flex-wrap gap-2">
              <Badge tone="info">
                {t('orderDetail.fulfillmentType')}：{h.fulfillmentTypeText(f.type)}
              </Badge>
              <Badge tone={f.status === 'delivered' ? 'success' : 'warning'}>
                {t('orderDetail.fulfillmentStatus')}：{h.fulfillmentStatusText(f.status)}
              </Badge>
            </div>
            <div class="flex gap-2">
              {truncated && (
                <Button size={size} loading={props.downloading} onClick={() => emit('download', props.orderNo)}>
                  <Download class="size-3.5" />
                  {props.downloading ? t('orderDetail.fulfillmentDownloading') : t('orderDetail.fulfillmentDownload')}
                </Button>
              )}
              {f.status === 'delivered' && !truncated && (
                <Button size={size} variant={h.fulfillmentCopied.value ? 'soft' : 'primary'} onClick={() => void h.handleCopyFulfillment(f)}>
                  {h.fulfillmentCopied.value ? <Check class="size-3.5" /> : <Copy class="size-3.5" />}
                  {h.fulfillmentCopied.value ? t('orderDetail.fulfillmentCopied') : t('orderDetail.fulfillmentCopy')}
                </Button>
              )}
            </div>
          </div>
          {truncated ? (
            <>
              <div class="text-sm text-muted">{t('orderDetail.fulfillmentTotalLines', { count: f.payload_line_count })}</div>
              <div class="rounded-zs-sm border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-warning-text">{t('orderDetail.fulfillmentTruncatedHint')}</div>
              <pre class="max-h-64 overflow-y-auto whitespace-pre-wrap break-all rounded-zs border border-line bg-surface-strong p-4 font-mono text-sm text-fg">{f.payload}</pre>
            </>
          ) : lines.length > 0 ? (
            <div class="space-y-1 break-all rounded-zs border border-line bg-surface-strong p-4 text-sm text-fg">
              {lines.map((line, i) => (
                <div key={i}>{line}</div>
              ))}
            </div>
          ) : f.payload ? (
            <pre class="whitespace-pre-wrap break-all rounded-zs border border-line bg-surface-strong p-4 font-mono text-sm text-fg">{f.payload}</pre>
          ) : null}
          {blocks.map((block, bi) => (
            <div key={bi} class="rounded-zs border border-accent/35 bg-accent-soft p-4">
              <div class="mb-2 flex items-center gap-2 text-sm font-bold text-accent-text">
                <Info class="size-4" />
                {t('orderDetail.instructionsTitle')}
              </div>
              <div class="zs-prose text-sm" innerHTML={block.html} />
            </div>
          ))}
        </div>
      )
    }
  },
})

/** Full order detail body: header card, children, fulfillment, items, amounts, refunds, times. */
export const OrderBody = defineComponent({
  name: 'OrderBody',
  props: {
    order: { type: Object as PropType<Order>, required: true },
    helpers: { type: Object as PropType<OrderDisplayHelpers>, required: true },
    downloading: Boolean,
  },
  emits: { download: (_no: string) => true },
  setup(props, { slots, emit }) {
    const { t } = useI18n()
    return () => {
      const o = props.order
      const h = props.helpers
      const cur = o.currency
      return (
        <div class="space-y-6">
          <Card>
            <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
              <div class="flex items-start gap-3">
                <span class="zs-gradient-bg flex size-12 shrink-0 items-center justify-center rounded-zs text-on-primary shadow-zs">
                  <ReceiptText class="size-6" />
                </span>
                <div>
                  <div class="text-xs text-muted">{t('orders.orderNo')}</div>
                  <div class="zs-num mt-0.5 break-all text-base font-bold text-fg">{o.order_no}</div>
                  <div class="mt-1 text-xs text-muted">
                    {t('orderDetail.createdAtLabel')}：{h.formatDate(o.created_at)}
                  </div>
                </div>
              </div>
              <div class="flex flex-col items-start gap-2 md:items-end">
                <div class="text-xs text-muted">{t('orderDetail.amountTotal')}</div>
                <div class="zs-num text-2xl font-bold zs-gradient-text">{h.money(o.total_amount, cur)}</div>
                <div class="flex flex-wrap items-center gap-2">
                  <OrderStatusBadge status={o.status} />
                  {slots.actions?.()}
                </div>
              </div>
            </div>
          </Card>

          {o.children && o.children.length > 0 && (
            <Card>
              <CardHeader title={t('orderDetail.childOrdersTitle')}>{{ icon: () => <Layers class="size-5 text-primary" /> }}</CardHeader>
              <div class="space-y-4">
                {o.children.map((child) => (
                  <div key={child.order_no} class="rounded-zs-lg border border-line bg-surface-strong p-4">
                    <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                      <div>
                        <div class="text-sm text-muted">
                          {t('orderDetail.childOrderNo')}：<span class="zs-num font-bold text-fg">{child.order_no}</span>
                        </div>
                        <div class="mt-1 text-xs text-muted">
                          {t('orderDetail.childOrderAmount')}：<span class="zs-num">{h.money(child.total_amount, child.currency || cur)}</span>
                        </div>
                      </div>
                      <OrderStatusBadge status={h.resolvedChildStatus(child)} />
                    </div>
                    <div class="mt-4">
                      <h3 class="mb-3 text-sm font-bold text-fg">{t('orderDetail.childItemsTitle')}</h3>
                      <OrderItemsList items={child.items || []} currency={cur} helpers={h} compact />
                    </div>
                    <div class="mt-4 border-t border-dashed border-line pt-4">
                      <h3 class="mb-3 text-sm font-bold text-fg">{t('orderDetail.childFulfillmentTitle')}</h3>
                      {child.fulfillment ? (
                        <FulfillmentBlock
                          fulfillment={child.fulfillment}
                          items={child.items || []}
                          orderNo={child.order_no || o.order_no}
                          helpers={h}
                          downloading={props.downloading}
                          small
                          onDownload={(no: string) => emit('download', no)}
                        />
                      ) : (
                        <div class="text-sm text-muted">{t('orderDetail.childFulfillmentEmpty')}</div>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {o.fulfillment && (
            <Card>
              <CardHeader title={t('orderDetail.fulfillmentTitle')}>{{ icon: () => <PackageCheck class="size-5 text-primary" /> }}</CardHeader>
              <FulfillmentBlock fulfillment={o.fulfillment} items={o.items || []} orderNo={o.order_no} helpers={h} downloading={props.downloading} onDownload={(no: string) => emit('download', no)} />
            </Card>
          )}

          <Card>
            <CardHeader title={t('orderDetail.itemsTitle')}>{{ icon: () => <ShoppingBag class="size-5 text-primary" /> }}</CardHeader>
            <OrderItemsList items={o.items || []} currency={cur} helpers={h} />
          </Card>

          <Card>
            <CardHeader title={t('orderDetail.amountTitle')}>{{ icon: () => <Wallet class="size-5 text-primary" /> }}</CardHeader>
            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-3">
              <InfoTile label={t('orderDetail.amountOriginal')} value={h.money(o.original_amount, cur)} />
              <InfoTile label={t('orderDetail.amountDiscount')} value={h.discountMoney(o.discount_amount, cur)} tone={h.hasPositive(o.discount_amount) ? 'discount' : 'default'} />
              <InfoTile label={t('orderDetail.amountTotal')} value={h.money(o.total_amount, cur)} />
              {h.hasPositive(o.wallet_paid_amount) && <InfoTile label={t('orderDetail.amountWalletPaid')} value={h.money(o.wallet_paid_amount, cur)} />}
              {h.hasPositive(o.online_paid_amount) && <InfoTile label={t('orderDetail.amountOnlinePaid')} value={h.money(o.online_paid_amount, cur)} />}
              {h.hasPositive(o.refunded_amount) && <InfoTile label={t('orderDetail.amountRefunded')} value={h.money(o.refunded_amount, cur)} />}
              {h.hasPositive(o.promotion_discount_amount) && <InfoTile label={t('orderDetail.promotionDiscountLabel')} value={h.discountMoney(o.promotion_discount_amount, cur)} tone="discount" />}
              {h.hasPositive(o.member_discount_amount) && <InfoTile label={t('orderDetail.amountMemberDiscount')} value={h.discountMoney(o.member_discount_amount, cur)} tone="member" />}
              {h.hasPositive(o.wholesale_discount_amount) && <InfoTile label={t('orderDetail.amountWholesaleDiscount')} value={h.discountMoney(o.wholesale_discount_amount, cur)} tone="wholesale" />}
            </div>
          </Card>

          {h.showRefundRecordsCard.value && (
            <Card>
              <CardHeader title={t('orderDetail.refundRecordsTitle')}>{{ icon: () => <RotateCcw class="size-5 text-primary" /> }}</CardHeader>
              <DataTable
                rows={h.refundRecords.value}
                rowKey={(_r: never, i: number) => i}
                emptyText={t('orderDetail.refundRecordsEmpty')}
                columns={columns<RefundRecord>([
                  { key: 'created_at', title: t('orderDetail.refundRecordTime'), render: (r) => <span class="text-xs text-muted">{h.formatDate(r.created_at)}</span> },
                  { key: 'amount', title: t('orderDetail.refundRecordAmount'), render: (r) => <span class="zs-num font-bold">{h.money(r.amount, r.currency || cur)}</span> },
                  { key: 'remark', title: t('orderDetail.refundRecordReason'), render: (r) => <span class="whitespace-pre-wrap text-xs text-muted">{h.refundReasonText(r.remark)}</span> },
                ])}
              />
            </Card>
          )}

          {h.showTimeCard.value && (
            <Card>
              <CardHeader title={t('orderDetail.timeTitle')}>{{ icon: () => <Clock class="size-5 text-primary" /> }}</CardHeader>
              <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-4">
                <InfoTile label={t('orderDetail.createdAtLabel')} value={h.formatDate(o.created_at)} />
                {o.paid_at && <InfoTile label={t('orderDetail.paidAtLabel')} value={h.formatDate(o.paid_at)} />}
                {o.expires_at && <InfoTile label={t('orderDetail.expiresAtLabel')} value={h.formatDate(o.expires_at)} />}
                {o.canceled_at && <InfoTile label={t('orderDetail.canceledAtLabel')} value={h.formatDate(o.canceled_at)} />}
              </div>
            </Card>
          )}
        </div>
      )
    }
  },
})

/** Loading skeleton for order detail pages. */
export const OrderBodySkeleton = defineComponent({
  name: 'OrderBodySkeleton',
  setup() {
    return () => (
      <div class="space-y-6">
        {[0, 1].map((i) => (
          <div key={i} class="zs-card space-y-4 p-6">
            <div class="zs-skeleton h-4 w-24" />
            <div class="zs-skeleton h-6 w-64" />
            <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
              {[0, 1, 2, 3].map((j) => (
                <div key={j} class="zs-skeleton h-10" />
              ))}
            </div>
          </div>
        ))}
      </div>
    )
  },
})
