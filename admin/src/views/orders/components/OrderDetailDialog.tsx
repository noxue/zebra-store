import { defineComponent, watch, type PropType, type VNodeChild } from 'vue'
import { useI18n } from 'vue-i18n'
import { Download, Package } from 'lucide-vue-next'
import type { AdminFulfillment, AdminOrder, AdminOrderItem, AdminPayment, Money } from '@/api/types'
import { Badge, Button, Checkbox, DataTable, Dialog, IdCell, Input, Tabs, cn, type BadgeTone, type DataTableColumn } from '@/components/ui'
import { formatDate, formatMoney, getLocalizedText, hasPositiveAmount } from '@/utils/format'
import { orderStatusLabel, orderStatusTone, paymentStatusLabel, paymentStatusTone } from '@/utils/status'
import { fulfillmentStatusLabel, fulfillmentTypeLabel } from '@/utils/fulfillment'
import { resolveSkuCodeFromSnapshot, resolveSkuSpecFromSnapshot } from '@/utils/sku'
import { adminUrl } from '@/utils/adminBase'
import {
  canCreateChildFulfillment,
  canManualRefund,
  canRefundToWallet,
  fulfillmentDeliveryLines,
  formatFeeRate,
  isFulfillmentTruncated,
  itemDiscountTotal,
  itemPaidAmount,
  itemProfit,
  itemRefundAmount,
  manualSubmissionRows,
  orderPaymentFee,
  orderProfit,
  parseOrderItemSkuId,
  refundableAmountValue,
  shouldShowRefundCard,
} from '../orderUtils'
import { useOrderDetail } from '../useOrderDetail'

const PROVIDER_TYPES = ['official', 'epay', 'bepusdt', 'epusdt', 'tokenpay', 'wallet']
const CHANNEL_TYPE_KEYS: Record<string, string> = {
  wechat: 'wechat',
  alipay: 'alipay',
  qqpay: 'qqpay',
  paypal: 'paypal',
  stripe: 'stripe',
  usdt: 'usdt',
  'usdt-trc20': 'usdtTrc20',
  'usdc-trc20': 'usdcTrc20',
  trx: 'trx',
  bepusdt: 'bepusdtCashier',
  epusdt: 'epusdtCashier',
  balance: 'balance',
}

const procurementTone = (status: string): BadgeTone => {
  if (status === 'pending' || status === 'partially_refunded') return 'warning'
  if (['submitted', 'accepted', 'refunded'].includes(status)) return 'info'
  if (['fulfilled', 'completed'].includes(status)) return 'success'
  if (['failed', 'rejected'].includes(status)) return 'danger'
  return 'neutral'
}

const Tile = (p: { label: string; tone?: 'default' | 'warning' | 'success' }, { slots }: { slots: { default?: () => VNodeChild } }) => (
  <div
    class={cn(
      'min-w-0 rounded-zs border px-3 py-2.5',
      p.tone === 'warning' ? 'border-warning/40 bg-warning-soft' : p.tone === 'success' ? 'border-success/40 bg-success-soft' : 'border-line bg-surface-strong',
    )}
  >
    <div class={cn('text-xs', p.tone === 'warning' ? 'text-warning-text' : p.tone === 'success' ? 'text-success-text' : 'text-muted')}>{p.label}</div>
    <div class={cn('mt-1 min-w-0 break-all text-sm', p.tone === 'warning' ? 'text-warning-text' : p.tone === 'success' ? 'text-success-text' : 'text-fg')}>
      {slots.default?.()}
    </div>
  </div>
)

const Section = (p: { title: string; class?: string }, { slots }: { slots: { default?: () => VNodeChild; extra?: () => VNodeChild } }) => (
  <section class={cn('rounded-zs-lg border border-line bg-surface-muted/50 p-4', p.class)}>
    <div class="mb-3 flex items-center justify-between gap-3">
      <h3 class="zs-display flex items-center gap-1.5 text-sm text-fg">
        <span class="text-xs text-primary">✦</span>
        {p.title}
      </h3>
      {slots.extra?.()}
    </div>
    {slots.default?.()}
  </section>
)

const Alert = (p: { tone: 'danger' | 'success' | 'warning' }, { slots }: { slots: { default?: () => VNodeChild } }) => (
  <div
    class={cn(
      'rounded-zs border px-3 py-2 text-sm',
      p.tone === 'danger' && 'border-danger/35 bg-danger-soft text-danger-text',
      p.tone === 'success' && 'border-success/40 bg-success-soft text-success-text',
      p.tone === 'warning' && 'border-warning/40 bg-warning-soft text-xs text-warning-text',
    )}
  >
    {slots.default?.()}
  </div>
)

/** Order detail dialog (amounts, discounts, items, children, fulfillment, procurement, payments, refund). */
export default defineComponent({
  name: 'OrderDetailDialog',
  props: {
    modelValue: Boolean,
    /** Seed order; only `id` is required — full detail is always re-fetched. */
    order: { type: Object as PropType<AdminOrder | null>, default: null },
    siteCurrency: { type: String, default: '' },
    maxRefundDays: { type: Number, default: 30 },
  },
  emits: {
    'update:modelValue': (_v: boolean) => true,
    refresh: () => true,
    openFulfillment: (_order: AdminOrder, _parentId?: number) => true,
  },
  setup(props, { emit }) {
    const { t, te, locale } = useI18n()
    const d = useOrderDetail({ maxRefundDays: () => props.maxRefundDays, onChanged: () => emit('refresh') })

    watch(
      [() => props.modelValue, () => props.order?.id],
      ([open]) => {
        if (open && props.order) d.open(props.order)
        if (!open) d.close()
      },
      { immediate: true },
    )

    const close = () => emit('update:modelValue', false)
    const openFulfillment = (order: AdminOrder, parentId?: number) => {
      close()
      emit('openFulfillment', order, parentId)
    }

    const discountMoney = (amount?: Money, currency?: string) => (hasPositiveAmount(amount) ? `-${formatMoney(amount, currency)}` : formatMoney(amount, currency))
    const providerTypeLabel = (v?: string) => (!v ? '-' : PROVIDER_TYPES.includes(v) ? t(`admin.paymentChannels.providerTypes.${v}`) : v)
    const channelTypeLabel = (payment: AdminPayment) => {
      const v = String(payment.display_channel_type || payment.channel_type || '').trim()
      if (!v) return '-'
      const key = CHANNEL_TYPE_KEYS[v]
      return key && te(`admin.paymentChannels.channelTypes.${key}`) ? t(`admin.paymentChannels.channelTypes.${key}`) : v
    }
    const paymentMethodLabel = (o: AdminOrder) => {
      const w = hasPositiveAmount(o.wallet_paid_amount)
      const on = hasPositiveAmount(o.online_paid_amount)
      if (w && on) return t('admin.orders.paymentMethodMixed')
      if (w) return t('admin.orders.paymentMethodWalletOnly')
      if (on) return t('admin.orders.paymentMethodOnlineOnly')
      return t('admin.orders.paymentMethodUnknown')
    }
    const skuCodeText = (item: AdminOrderItem) => {
      const code = resolveSkuCodeFromSnapshot(item.sku_snapshot, { defaultLabel: t('admin.orders.itemSkuDefaultCode') })
      if (code) return code
      const id = parseOrderItemSkuId(item)
      return id > 0 ? `#${id}` : t('admin.orders.itemSkuUnknown')
    }
    const skuSpecText = (item: AdminOrderItem) => resolveSkuSpecFromSnapshot(item.sku_snapshot, locale.value) || t('admin.orders.itemSkuSpecEmpty')

    const link = (href: string, text: VNodeChild, cls = '') => (
      <a href={href} target="_blank" rel="noopener" class={cn('text-accent underline-offset-4 hover:underline', cls)}>
        {text}
      </a>
    )

    // ---- item row (shared by parent + child orders) ----
    const renderItem = (owner: AdminOrder, root: AdminOrder, item: AdminOrderItem, index: number, showMeta: boolean) => {
      const cur = root.currency
      const manualRows = manualSubmissionRows(item.manual_form_submission, item.manual_form_schema_snapshot)
      const refund = itemRefundAmount(owner, index)
      return (
        <div key={item.id} class="flex flex-col gap-3 border-b border-line pb-3 text-sm text-muted last:border-b-0 last:pb-0 md:flex-row md:items-start md:justify-between">
          <div class="min-w-0 flex-1">
            {link(adminUrl(`/products?product_id=${item.product_id}`), getLocalizedText(item.title) || '-', 'break-words font-medium')}
            <div class="mt-1 text-xs">#{item.product_id}</div>
            <div class="text-xs">
              {t('orderDetail.quantityLabel')}：<span class="zs-num text-fg">{item.quantity}</span>
            </div>
            <div class="mt-2 rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
              <div class="flex flex-wrap items-center gap-2">
                <Badge tone="success">
                  <Package class="h-3 w-3" />
                  {t('admin.orders.itemSkuLabel')}
                </Badge>
                <span class="break-all font-mono text-xs text-fg">{skuCodeText(item)}</span>
              </div>
              <div class="mt-1 break-words text-xs">
                {t('admin.orders.itemSkuSpec')}：{skuSpecText(item)}
              </div>
            </div>
            {showMeta && (
              <>
                <div class="mt-1 text-xs">
                  {t('admin.orders.itemCouponCode')}：
                  {root.coupon_code ? link(adminUrl(`/coupons?code=${encodeURIComponent(root.coupon_code)}`), root.coupon_code, 'break-all') : '-'}
                </div>
                <div class="mt-1 text-xs">
                  {t('admin.orders.itemPromotionName')}：
                  {item.promotion_id ? link(adminUrl(`/promotions?id=${item.promotion_id}`), item.promotion_name || `#${item.promotion_id}`, 'break-words') : '-'}
                </div>
                {!!item.tags?.length && (
                  <div class="mt-2 flex flex-wrap gap-1.5">
                    {item.tags.map((tag, i) => (
                      <Badge key={i} tone="neutral">
                        {tag}
                      </Badge>
                    ))}
                  </div>
                )}
              </>
            )}
            {manualRows.length > 0 && (
              <div class="mt-3 rounded-zs-sm border border-line bg-surface-strong p-3">
                <div class="mb-2 text-xs font-semibold">{t('admin.orders.manualSubmissionTitle')}</div>
                <div class="space-y-1 text-xs">
                  {manualRows.map((row) => (
                    <div key={row.key} class="break-words">
                      <span class="text-fg">{row.label}</span>：<span class="break-all">{row.value}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
          <div class="shrink-0 space-y-1 text-left text-xs md:text-right">
            <div>
              {t('orderDetail.unitPriceLabel')}：<span class="font-mono">{formatMoney(item.original_unit_price, cur)}</span>
            </div>
            <div>
              {t('admin.orders.costPrice')}：<span class="font-mono">{formatMoney(item.cost_price, cur)}</span>
            </div>
            <div>
              {t('orderDetail.totalPriceLabel')}：<span class="font-mono">{formatMoney(item.original_total_price, cur)}</span>
            </div>
            {hasPositiveAmount(item.coupon_discount_amount) && (
              <div>
                {t('orderDetail.couponDiscountLabel')}：{discountMoney(item.coupon_discount_amount, cur)}
              </div>
            )}
            {hasPositiveAmount(item.promotion_discount_amount) && (
              <div>
                {t('orderDetail.promotionDiscountLabel')}：{discountMoney(item.promotion_discount_amount, cur)}
              </div>
            )}
            {hasPositiveAmount(item.wholesale_discount_amount) && (
              <div>
                {t('orderDetail.wholesaleDiscountLabel')}：{discountMoney(item.wholesale_discount_amount, cur)}
              </div>
            )}
            {hasPositiveAmount(item.member_discount_amount) && (
              <div>
                {t('orderDetail.memberDiscountLabel')}：{discountMoney(item.member_discount_amount, cur)}
              </div>
            )}
            {itemDiscountTotal(item) > 0 && (
              <div class="font-medium text-danger-text">
                {t('orderDetail.itemDiscountTotalLabel')}：{formatMoney(itemDiscountTotal(item).toFixed(2), cur)}
              </div>
            )}
            <div class="font-medium text-fg">
              {t('orderDetail.itemPaidAmountLabel')}：{formatMoney(itemPaidAmount(item).toFixed(2), cur)}
            </div>
            {refund > 0 && (
              <div class="text-info-text">
                {t('admin.orders.itemRefund')}：{formatMoney(refund.toFixed(2), cur)}
              </div>
            )}
            <div class="font-medium text-success-text">
              {t('admin.orders.itemProfit')}：{formatMoney(itemProfit(owner, item, index).toFixed(2), cur)}
            </div>
          </div>
        </div>
      )
    }

    const renderFulfillmentBody = (f: AdminFulfillment, orderId: number, orderNo: string, inChild: boolean) => {
      const lines = fulfillmentDeliveryLines(f)
      const box = 'mt-3 rounded-zs-sm border border-line bg-surface-strong p-3 text-xs text-fg break-all'
      if (isFulfillmentTruncated(f)) {
        return (
          <div class="mt-3 space-y-2">
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs text-muted">{t('admin.orders.fulfillmentTotalLines', { count: f.payload_line_count })}</span>
              {inChild && (
                <Button size="xs" loading={d.downloading.value} onClick={() => d.downloadFulfillment(orderId, orderNo)}>
                  <Download class="h-3 w-3" />
                  {d.downloading.value ? t('admin.orders.fulfillmentDownloading') : t('admin.orders.fulfillmentDownload')}
                </Button>
              )}
            </div>
            <Alert tone="warning">{t('admin.orders.fulfillmentTruncatedHint')}</Alert>
            <div class={cn(box, 'mt-0 max-h-64 overflow-y-auto whitespace-pre-wrap')}>{f.payload}</div>
          </div>
        )
      }
      if (lines.length) {
        return (
          <div class={cn(box, 'space-y-1')}>
            {lines.map((line, i) => (
              <div key={i}>{line}</div>
            ))}
          </div>
        )
      }
      if (f.payload || !inChild) return <div class={cn(box, 'whitespace-pre-wrap')}>{f.payload || '-'}</div>
      return null
    }

    const paymentColumns = (): DataTableColumn<AdminPayment>[] => [
      {
        key: 'id',
        title: t('admin.payments.table.paymentId'),
        render: (p) => (
          <div class="flex items-center gap-2">
            <IdCell value={p.id} />
            {link(adminUrl(`/payments?payment_id=${p.id}`), t('admin.payments.view'), 'text-xs')}
          </div>
        ),
      },
      {
        key: 'channel',
        title: t('admin.payments.table.channel'),
        class: 'text-xs',
        render: (p) => (
          <div>
            <div class="text-fg">{p.channel_name || '-'}</div>
            <div class="text-muted">
              {providerTypeLabel(p.provider_type)} / {channelTypeLabel(p)}
            </div>
          </div>
        ),
      },
      {
        key: 'status',
        title: t('admin.payments.table.status'),
        render: (p) => (
          <Badge tone={paymentStatusTone(p.status)} dot>
            {paymentStatusLabel(t, p.status)}
          </Badge>
        ),
      },
      { key: 'feeRate', title: t('admin.payments.table.feeRate'), class: 'text-xs text-muted', render: (p) => formatFeeRate(p) },
      { key: 'amount', title: t('admin.payments.table.amount'), class: 'font-mono text-xs text-fg', render: (p) => formatMoney(p.amount, p.currency) },
      { key: 'createdAt', title: t('admin.payments.table.createdAt'), class: 'text-xs text-muted whitespace-nowrap', render: (p) => formatDate(p.created_at) },
    ]

    const renderRefundCard = (o: AdminOrder) => {
      const days = props.maxRefundDays
      if (!shouldShowRefundCard(o, days)) return null
      const walletAvailable = !!o.user_id
      const walletActive = walletAvailable && d.refundTab.value === 'wallet'
      const canWallet = canRefundToWallet(o, days)
      const canManual = canManualRefund(o, days)
      const tabItems = [
        ...(walletAvailable ? [{ key: 'wallet', label: t('admin.orders.refundTabWallet') }] : []),
        { key: 'manual', label: t('admin.orders.refundTabManual') },
      ]
      const grid = 'grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,2fr)_auto]'
      return (
        <Section title={t('admin.orders.refundCardTitle')}>
          <div class="mb-3 text-xs text-muted">
            {t('admin.orders.refundableAmount')}：<span class="font-mono text-fg">{formatMoney(refundableAmountValue(o).toFixed(2), o.currency)}</span>
          </div>
          <div class="mb-4">
            <Tabs modelValue={walletActive ? 'wallet' : 'manual'} items={tabItems} onUpdate:modelValue={(v) => (d.refundTab.value = v === 'wallet' ? 'wallet' : 'manual')} />
          </div>
          {walletActive ? (
            <form
              class={grid}
              onSubmit={(e: Event) => {
                e.preventDefault()
                void d.submitWallet()
              }}
            >
              <Input v-model={d.wallet.amount} placeholder={t('admin.orders.refundAmountPlaceholder')} disabled={d.wallet.submitting || !canWallet} />
              <Input v-model={d.wallet.remark} placeholder={t('admin.orders.refundRemarkPlaceholder')} disabled={d.wallet.submitting || !canWallet} />
              <Button type="submit" variant="primary" loading={d.wallet.submitting} disabled={!canWallet}>
                {d.wallet.submitting ? t('admin.orders.refunding') : t('admin.orders.refundSubmit')}
              </Button>
            </form>
          ) : (
            <form
              class="space-y-3"
              onSubmit={(e: Event) => {
                e.preventDefault()
                void d.submitManual()
              }}
            >
              <div class={grid}>
                <Input v-model={d.manual.amount} placeholder={t('admin.orders.refundAmountPlaceholder')} disabled={d.manual.submitting || !canManual} />
                <Input v-model={d.manual.reason} placeholder={t('admin.orders.manualRefundReasonPlaceholder')} disabled={d.manual.submitting || !canManual} />
                <Button type="submit" variant="primary" loading={d.manual.submitting} disabled={!canManual}>
                  {d.manual.submitting ? t('admin.orders.refunding') : t('admin.orders.manualRefundSubmit')}
                </Button>
              </div>
              <div class="rounded-zs-sm border border-line bg-surface-strong px-3 py-2">
                <Checkbox v-model={d.manual.paymentFeeRefunded} disabled={d.manual.submitting || !canManual}>
                  <span class="block">
                    <span class="block text-sm font-medium text-fg">{t('admin.orders.paymentFeeRefunded')}</span>
                    <span class="block text-xs text-muted">{t('admin.orders.paymentFeeRefundedHint')}</span>
                  </span>
                </Checkbox>
              </div>
            </form>
          )}
          {((walletActive && !canWallet) || (!walletActive && !canManual)) && <div class="mt-3 text-xs text-muted">{t('admin.orders.refundNoRemaining')}</div>}
          {walletActive && d.wallet.error && (
            <div class="mt-3">
              <Alert tone="danger">{d.wallet.error}</Alert>
            </div>
          )}
          {walletActive && d.wallet.success && (
            <div class="mt-3">
              <Alert tone="success">{d.wallet.success}</Alert>
            </div>
          )}
          {!walletActive && d.manual.error && (
            <div class="mt-3">
              <Alert tone="danger">{d.manual.error}</Alert>
            </div>
          )}
          {!walletActive && d.manual.success && (
            <div class="mt-3">
              <Alert tone="success">{d.manual.success}</Alert>
            </div>
          )}
        </Section>
      )
    }

    const renderBody = (o: AdminOrder) => {
      const cur = o.currency
      const proc = d.procurement.value
      const fee = orderPaymentFee(o)
      return (
        <div class="space-y-5">
          {/* basic info */}
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <Tile label={t('admin.orders.table.id')}>
              <IdCell value={o.id} />
            </Tile>
            <Tile label={t('admin.orders.detailOrderNo')}>
              <span class="font-mono">{o.order_no}</span>
            </Tile>
            <Tile label={t('admin.orders.detailUser')}>
              {o.user_id ? (
                <span>
                  {t('admin.orders.userLabel')}: {link(adminUrl(`/users/${o.user_id}`), `#${o.user_id}`)}
                </span>
              ) : (
                <span class="break-all">
                  {t('admin.orders.guestLabel')}: {o.guest_email || '-'}
                </span>
              )}
            </Tile>
            <Tile label={t('admin.orders.detailStatus')}>
              <Badge tone={orderStatusTone(o.status)} dot>
                {orderStatusLabel(t, o.status)}
              </Badge>
            </Tile>
            <Tile label={t('admin.orders.detailCreatedAt')}>{formatDate(o.created_at)}</Tile>
            <Tile label={t('admin.orders.detailClientIp')}>
              <span class="font-mono">{o.client_ip || '-'}</span>
            </Tile>
          </div>

          {/* amounts */}
          <Section title={t('orderDetail.amountTitle')}>
            <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-3">
              <Tile label={t('orderDetail.amountOriginal')}>
                <span class="font-mono">{formatMoney(o.original_amount, cur)}</span>
              </Tile>
              <Tile label={t('orderDetail.amountDiscount')}>
                <span class={cn('font-mono', hasPositiveAmount(o.discount_amount) && 'text-danger-text')}>{discountMoney(o.discount_amount, cur)}</span>
              </Tile>
              <Tile label={t('orderDetail.amountTotal')}>
                <span class="zs-num font-semibold text-primary">{formatMoney(o.total_amount, cur)}</span>
              </Tile>
              {hasPositiveAmount(o.wallet_paid_amount) && (
                <Tile label={t('admin.orders.detailWalletPaid')}>
                  <span class="font-mono">{formatMoney(o.wallet_paid_amount, cur)}</span>
                </Tile>
              )}
              {hasPositiveAmount(o.online_paid_amount) && (
                <Tile label={t('admin.orders.detailOnlinePaid')}>
                  <span class="font-mono">{formatMoney(o.online_paid_amount, cur)}</span>
                </Tile>
              )}
              {hasPositiveAmount(o.refunded_amount) && (
                <Tile label={t('admin.orders.detailRefunded')}>
                  <span class="font-mono">{formatMoney(o.refunded_amount, cur)}</span>
                </Tile>
              )}
              <Tile label={t('admin.orders.detailPaymentMethod')}>{paymentMethodLabel(o)}</Tile>
            </div>
          </Section>

          {/* discounts */}
          <Section title={t('admin.orders.detailDiscountTitle')}>
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
              <Tile label={t('admin.orders.detailCouponDiscount')}>
                <span class={cn('font-mono', hasPositiveAmount(o.discount_amount) && 'text-danger-text')}>{discountMoney(o.discount_amount, cur)}</span>
              </Tile>
              <Tile label={t('admin.orders.detailPromotionDiscount')}>
                <span class={cn('font-mono', hasPositiveAmount(o.promotion_discount_amount) && 'text-danger-text')}>
                  {discountMoney(o.promotion_discount_amount, cur)}
                </span>
              </Tile>
              {hasPositiveAmount(o.member_discount_amount) && (
                <Tile label={t('admin.orders.detailMemberDiscount')} tone="warning">
                  <span class="font-mono">{discountMoney(o.member_discount_amount, cur)}</span>
                </Tile>
              )}
              {hasPositiveAmount(o.wholesale_discount_amount) && (
                <Tile label={t('admin.orders.detailWholesaleDiscount')} tone="success">
                  <span class="font-mono">{discountMoney(o.wholesale_discount_amount, cur)}</span>
                </Tile>
              )}
            </div>
          </Section>

          {/* items */}
          <Section title={t('orderDetail.itemsTitle')}>
            {o.items?.length ? (
              <>
                <div class="space-y-3">{o.items.map((item, i) => renderItem(o, o, item, i, true))}</div>
                <div class="mt-3 flex flex-wrap justify-end gap-2">
                  {hasPositiveAmount(o.refunded_amount) && (
                    <div class="rounded-zs border border-accent/40 bg-accent-soft px-4 py-2 text-sm font-semibold text-info-text">
                      {t('admin.orders.itemRefund')}：{formatMoney(o.refunded_amount, cur)}
                    </div>
                  )}
                  {fee > 0 && (
                    <div class="rounded-zs border border-warning/40 bg-warning-soft px-4 py-2 text-sm font-semibold text-warning-text">
                      {t('admin.payments.table.feeAmount')}：{formatMoney(fee.toFixed(2), cur)}
                    </div>
                  )}
                  <div class="rounded-zs border border-success/40 bg-success-soft px-4 py-2 text-sm font-semibold text-success-text">
                    {t('admin.orders.orderProfit')}：{formatMoney(orderProfit(o).toFixed(2), cur)}
                  </div>
                </div>
              </>
            ) : (
              <div class="text-xs text-muted">{t('orderDetail.noItems')}</div>
            )}
          </Section>

          {/* child orders */}
          {!!o.children?.length && (
            <Section title={t('orderDetail.childOrdersTitle')}>
              <div class="space-y-4">
                {o.children.map((child) => (
                  <div key={child.id} class="rounded-zs border border-line bg-surface-strong p-4">
                    <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
                      <div class="min-w-0 text-xs text-muted">
                        <div class="break-all">
                          {t('orderDetail.childOrderNo')}：<span class="font-mono text-fg">{child.order_no}</span>
                        </div>
                        <div class="mt-1">
                          {t('orderDetail.childOrderAmount')}：<span class="font-mono text-fg">{formatMoney(child.total_amount, child.currency || cur)}</span>
                        </div>
                      </div>
                      <div class="flex flex-wrap items-center gap-2 md:justify-end">
                        <Badge tone={orderStatusTone(child.status)} dot>
                          {orderStatusLabel(t, child.status)}
                        </Badge>
                        {canCreateChildFulfillment(child) && (
                          <Button size="sm" variant="secondary" onClick={() => openFulfillment(child, o.id)}>
                            {t('admin.orders.fulfillmentCreate')}
                          </Button>
                        )}
                      </div>
                    </div>
                    <div class="mt-4">
                      <h4 class="mb-2 text-xs font-semibold text-muted">{t('orderDetail.childItemsTitle')}</h4>
                      {child.items?.length ? (
                        <div class="space-y-3">{child.items.map((item, i) => renderItem(child, o, item, i, false))}</div>
                      ) : (
                        <div class="text-xs text-muted">{t('orderDetail.noItems')}</div>
                      )}
                    </div>
                    <div class="mt-4">
                      <h4 class="mb-2 text-xs font-semibold text-muted">{t('orderDetail.childFulfillmentTitle')}</h4>
                      {child.fulfillment ? (
                        <div class="text-xs text-muted">
                          <div>
                            {t('admin.orders.detailFulfillmentType')}：{fulfillmentTypeLabel(t, child.fulfillment.type, 'admin.orders')}
                          </div>
                          <div>
                            {t('admin.orders.detailFulfillmentStatus')}：{fulfillmentStatusLabel(t, child.fulfillment.status, 'admin.orders')}
                          </div>
                          {renderFulfillmentBody(child.fulfillment, child.id, child.order_no || o.order_no, true)}
                        </div>
                      ) : (
                        <div class="text-xs text-muted">{t('orderDetail.childFulfillmentEmpty')}</div>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </Section>
          )}

          {/* fulfillment */}
          {o.fulfillment && (
            <Section title={t('admin.orders.detailFulfillmentTitle')}>
              {{
                extra: () =>
                  isFulfillmentTruncated(o.fulfillment) ? (
                    <Button size="xs" loading={d.downloading.value} onClick={() => d.downloadFulfillment(o.id, o.order_no)}>
                      <Download class="h-3 w-3" />
                      {d.downloading.value ? t('admin.orders.fulfillmentDownloading') : t('admin.orders.fulfillmentDownload')}
                    </Button>
                  ) : null,
                default: () => (
                  <div class="text-sm text-muted">
                    <div>
                      {t('admin.orders.detailFulfillmentType')}：{fulfillmentTypeLabel(t, o.fulfillment?.type, 'admin.orders')}
                    </div>
                    <div>
                      {t('admin.orders.detailFulfillmentStatus')}：{fulfillmentStatusLabel(t, o.fulfillment?.status, 'admin.orders')}
                    </div>
                    {o.fulfillment && renderFulfillmentBody(o.fulfillment, o.id, o.order_no, false)}
                  </div>
                ),
              }}
            </Section>
          )}

          {/* procurement */}
          {proc && (
            <Section title={t('orderDetail.procurementTitle')} class="border-secondary/30 bg-secondary-soft/60">
              <div class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
                <div>
                  <span class="text-xs text-muted">{t('procurement.columns.id')}:</span>
                  <span class="ml-1 font-mono">{proc.id}</span>
                </div>
                <div>
                  <span class="text-xs text-muted">{t('procurement.columns.status')}:</span>
                  <span class="ml-1">
                    <Badge tone={procurementTone(proc.status)}>{te(`procurement.status.${proc.status}`) ? t(`procurement.status.${proc.status}`) : proc.status}</Badge>
                  </span>
                </div>
                <div>
                  <span class="text-xs text-muted">{t('procurement.columns.connection')}:</span>
                  <span class="ml-1 break-words">{proc.connection?.name || proc.connection_name || proc.connection_id || '-'}</span>
                </div>
                <div>
                  <span class="text-xs text-muted">{t('procurement.columns.upstreamOrderNo')}:</span>
                  <span class="ml-1 break-all font-mono">{proc.upstream_order_no || '-'}</span>
                </div>
                <div>
                  <span class="text-xs text-muted">{t('procurement.columns.upstreamAmount')}:</span>
                  <span class="ml-1 font-mono">{proc.upstream_amount || '-'}</span>
                </div>
                {proc.error_message && (
                  <div>
                    <span class="text-xs text-muted">{t('procurement.columns.errorMessage')}:</span>
                    <span class="ml-1 break-words text-xs text-danger-text">{proc.error_message}</span>
                  </div>
                )}
              </div>
            </Section>
          )}

          {/* payments */}
          <Section title={t('admin.orders.detailPayments')}>
            {hasPositiveAmount(o.wallet_paid_amount) && !o.payments?.length && (
              <div class="mb-3">
                <Alert tone="warning">{t('admin.orders.detailWalletOnlyNoOnlinePayments')}</Alert>
              </div>
            )}
            {o.payments?.length ? (
              <DataTable columns={paymentColumns()} rows={o.payments} rowKey={(p) => p.id} minWidth="760px" bare />
            ) : (
              <div class="text-xs text-muted">{t('admin.payments.empty')}</div>
            )}
          </Section>

          {renderRefundCard(o)}
        </div>
      )
    }

    return () => (
      <Dialog modelValue={props.modelValue} onUpdate:modelValue={(v) => emit('update:modelValue', v)} title={t('admin.orders.detailTitle')} size="2xl">
        {{
          default: () =>
            d.loading.value ? (
              <div class="space-y-3" aria-busy="true">
                <div class="zs-skeleton h-24 rounded-zs" />
                <div class="zs-skeleton h-32 rounded-zs" />
              </div>
            ) : d.error.value ? (
              <Alert tone="danger">{d.error.value}</Alert>
            ) : d.order.value ? (
              renderBody(d.order.value)
            ) : null,
        }}
      </Dialog>
    )
  },
})
