import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Order, PaymentCreateResult } from '@/api/types'
import { AmountRow } from '@/components/order/AmountRow'
import { hasPositive } from '@/utils/orderPayment'

/** Payable amount + order discount breakdown shown next to an active payment. */
export const PaymentAmountBreakdown = defineComponent({
  name: 'PaymentAmountBreakdown',
  props: {
    order: { type: Object as PropType<Order>, required: true },
    payment: { type: Object as PropType<PaymentCreateResult>, required: true },
    money: { type: Function as PropType<(amount: string | undefined) => string>, required: true },
    payableDisplay: { type: String, required: true },
    feeDisplay: { type: String, default: '' },
    walletPaidDisplay: { type: String, default: '-' },
    onlinePayDisplay: { type: String, default: '-' },
    showCountdown: Boolean,
    countdownText: { type: String, default: '' },
    polling: Boolean,
  },
  setup(props) {
    const { t } = useI18n()
    const disc = (v?: string) => `-${props.money(v)}`
    return () => {
      const o = props.order
      return (
        <div class="rounded-zs-lg border border-line bg-surface-strong p-4">
          <div class="text-xs text-muted">{t('payment.payableAmountLabel')}</div>
          <div class="zs-num mt-1 text-3xl font-bold zs-gradient-text">{props.payableDisplay}</div>
          <div class="mt-4 space-y-2 text-xs">
            <AmountRow label={t('orderDetail.amountTotal')} value={props.money(o.total_amount)} />
            {hasPositive(o.discount_amount) && <AmountRow label={t('orderDetail.amountDiscount')} value={disc(o.discount_amount)} tone="discount" />}
            {hasPositive(o.promotion_discount_amount) && <AmountRow label={t('orderDetail.promotionDiscountLabel')} value={disc(o.promotion_discount_amount)} tone="discount" />}
            {hasPositive(o.wholesale_discount_amount) && <AmountRow label={t('orderDetail.amountWholesaleDiscount')} value={disc(o.wholesale_discount_amount)} tone="wholesale" />}
            {hasPositive(o.member_discount_amount) && <AmountRow label={t('orderDetail.amountMemberDiscount')} value={disc(o.member_discount_amount)} tone="member" />}
            {props.feeDisplay && <AmountRow label={t('payment.feeAmountLabel')} value={props.feeDisplay} tone="warning" />}
            {props.payment.wallet_paid_amount !== undefined && <AmountRow label={t('payment.walletDeductLabel')} value={props.walletPaidDisplay} />}
            {props.payment.online_pay_amount !== undefined && <AmountRow label={t('payment.onlinePayLabel')} value={props.onlinePayDisplay} />}
          </div>
          {(props.showCountdown || props.polling) && (
            <div class="mt-4 space-y-2 border-t border-line pt-3 text-xs">
              {props.showCountdown && <AmountRow label={t('payment.countdownLabel')} value={props.countdownText} tone="warning" />}
              {props.polling && (
                <div class="flex items-center gap-2 text-muted">
                  <span class="relative flex size-2">
                    <span class="absolute inline-flex size-full animate-ping rounded-full bg-primary opacity-60" />
                    <span class="relative inline-flex size-2 rounded-full bg-primary" />
                  </span>
                  {t('payment.pollingHint')}
                </div>
              )}
            </div>
          )}
        </div>
      )
    }
  },
})
