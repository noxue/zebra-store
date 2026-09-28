import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Order } from '@/api/types'
import { OrderStatusBadge } from '@/components/common/OrderStatusBadge'
import { Price } from '@/components/common/Price'
import { Badge, Button } from '@/components/ui'
import { useLocalized } from '@/composables/useLocalized'
import { formatDateTime } from '@/utils/format'
import { isPositiveAmount } from '@/utils/money'

/** Compact order card row with status + view / pay actions. */
export const OrderRow = defineComponent({
  name: 'OrderRow',
  props: {
    order: { type: Object as PropType<Order>, required: true },
    showDiscounts: Boolean,
  },
  setup(props) {
    const { t } = useI18n()
    const { formatPrice } = useLocalized()
    return () => {
      const o = props.order
      return (
        <div class="zs-card-hover rounded-zs border border-line bg-surface-strong px-4 py-4 sm:px-5">
          <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div class="min-w-0">
              <div class="truncate text-xs text-muted">
                {t('orders.orderNo')}：<span class="zs-num">{o.order_no}</span>
              </div>
              <div class="mt-1.5">
                <Price amount={o.total_amount} currency={o.currency} size="lg" highlight />
              </div>
              {props.showDiscounts && (isPositiveAmount(o.discount_amount) || isPositiveAmount(o.promotion_discount_amount)) && (
                <div class="mt-2 flex flex-wrap gap-2">
                  {isPositiveAmount(o.discount_amount) && (
                    <Badge tone="success" size="xs">
                      {t('orderDetail.couponDiscountLabel')}：-{formatPrice(o.discount_amount, o.currency)}
                    </Badge>
                  )}
                  {isPositiveAmount(o.promotion_discount_amount) && (
                    <Badge tone="danger" size="xs">
                      {t('orderDetail.promotionDiscountLabel')}：-{formatPrice(o.promotion_discount_amount, o.currency)}
                    </Badge>
                  )}
                </div>
              )}
              <div class="mt-1.5 text-xs text-muted">{formatDateTime(o.created_at)}</div>
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <OrderStatusBadge status={o.status} />
              <Button size="sm" variant="secondary" to={`/orders/${encodeURIComponent(o.order_no)}`}>
                {t('orders.viewDetails')}
              </Button>
              {o.status === 'pending_payment' && (
                <Button size="sm" to={`/pay?order_no=${encodeURIComponent(o.order_no)}`}>
                  {t('orders.payNow')}
                </Button>
              )}
            </div>
          </div>
        </div>
      )
    }
  },
})
