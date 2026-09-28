import { defineComponent } from 'vue'
import { useI18n } from 'vue-i18n'
import { Wallet } from 'lucide-vue-next'
import { Checkbox } from '@/components/ui'

/** Wallet balance + "use balance" toggle with deduction preview. */
export const WalletBalanceBox = defineComponent({
  name: 'WalletBalanceBox',
  props: {
    balanceText: { type: String, required: true },
    loading: Boolean,
    modelValue: Boolean,
    walletOnly: Boolean,
    walletPaidText: { type: String, default: '' },
    onlinePayText: { type: String, default: '' },
    insufficient: Boolean,
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    return () => (
      <div class="rounded-zs border border-line zs-soft-bg p-4">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="flex items-center gap-3">
            <span class="flex size-10 items-center justify-center rounded-full bg-gold-soft text-warning-text">
              <Wallet class="size-5" />
            </span>
            <div>
              <div class="text-xs text-muted">{t('payment.walletBalanceLabel')}</div>
              <div class="zs-num text-base font-bold text-fg">{props.loading ? t('common.loading') : props.balanceText}</div>
            </div>
          </div>
          <Checkbox modelValue={props.modelValue} disabled={props.walletOnly} onUpdate:modelValue={(v: boolean) => emit('update:modelValue', v)}>
            <span class="text-xs text-muted">{t('payment.useBalance')}</span>
          </Checkbox>
        </div>
        {props.walletOnly && <div class="mt-3 text-xs text-warning-text">{t('payment.walletOnlyHint')}</div>}
        {props.modelValue && (
          <div class="mt-3 space-y-1 border-t border-line pt-3 text-xs text-muted">
            <div class="flex justify-between">
              <span>{t('payment.walletDeductLabel')}</span>
              <span class="zs-num font-bold text-fg">{props.walletPaidText}</span>
            </div>
            {!props.walletOnly && (
              <div class="flex justify-between">
                <span>{t('payment.onlinePayLabel')}</span>
                <span class="zs-num font-bold text-fg">{props.onlinePayText}</span>
              </div>
            )}
            {props.walletOnly && props.insufficient && <div class="text-warning-text">{t('payment.walletInsufficientHint')}</div>}
          </div>
        )}
      </div>
    )
  },
})
