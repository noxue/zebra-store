import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleCheck, CreditCard, Wallet } from 'lucide-vue-next'
import type { PaymentChannel } from '@/api/types'
import { Badge, cn } from '@/components/ui'
import { getImageUrl } from '@/utils/image'
import { formatFeeRate } from '@/utils/orderPayment'

/** Payment channel grid: icon, name, customer surcharge fees and amount-limit hints. */
export const PaymentChannelSelector = defineComponent({
  name: 'PaymentChannelSelector',
  props: {
    channels: { type: Array as PropType<PaymentChannel[]>, required: true },
    modelValue: { type: Number as PropType<number | null>, default: null },
    isDisabled: { type: Function as PropType<(c: PaymentChannel) => boolean>, default: () => false },
    limitHint: { type: Function as PropType<(c: PaymentChannel) => string>, default: () => '' },
    /** Formats the fixed fee (currency aware); omitted → plain number. */
    fixedFee: { type: Function as PropType<(c: PaymentChannel) => string>, default: undefined },
    compact: Boolean,
    emptyText: { type: String, default: '' },
  },
  emits: { 'update:modelValue': (_id: number) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const feeText = (c: PaymentChannel) => {
      if (props.fixedFee) return `${t('payment.feeRateLabel')} ${formatFeeRate(c.fee_rate)} · ${t('payment.fixedFeeLabel')} ${props.fixedFee(c)}`
      const parts: string[] = []
      const rate = Number(c.fee_rate || 0)
      const fixed = Number(c.fixed_fee || 0)
      if (rate > 0) parts.push(`${rate.toFixed(2)}%`)
      if (fixed > 0) parts.push(fixed.toFixed(2))
      return parts.join(' + ') || t('payment.feeFree')
    }
    return () => {
      if (props.channels.length === 0) return <div class="rounded-zs border border-dashed border-line px-4 py-6 text-center text-sm text-muted">{props.emptyText || t('payment.channelEmpty')}</div>
      return (
        <div class={cn('grid gap-3', props.compact ? 'grid-cols-1' : 'grid-cols-1 sm:grid-cols-2')}>
          {props.channels.map((channel) => {
            const disabled = props.isDisabled(channel)
            const selected = props.modelValue === Number(channel.id) && !disabled
            return (
              <button
                key={channel.id}
                type="button"
                disabled={disabled}
                title={disabled ? props.limitHint(channel) : ''}
                class={cn(
                  'group relative rounded-zs border p-3 text-left transition-all disabled:cursor-not-allowed disabled:opacity-55',
                  selected ? 'border-primary bg-primary-soft shadow-glow' : 'border-line bg-surface-strong hover:-translate-y-0.5 hover:border-line-strong hover:shadow-zs',
                )}
                onClick={() => {
                  if (!disabled) emit('update:modelValue', Number(channel.id))
                }}
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="flex min-w-0 items-center gap-2">
                    <span class={cn('flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-zs-sm', selected ? 'zs-gradient-bg text-on-primary' : 'bg-surface-muted text-primary-text')}>
                      {channel.icon ? (
                        <img src={getImageUrl(channel.icon)} alt="" loading="lazy" class="size-6 object-contain" />
                      ) : channel.channel_type === 'wallet' ? (
                        <Wallet class="size-4" />
                      ) : (
                        <CreditCard class="size-4" />
                      )}
                    </span>
                    <span class="truncate text-sm font-bold text-fg">{channel.name}</span>
                  </div>
                  {selected &&
                    (props.compact ? (
                      <CircleCheck class="size-4 shrink-0 text-primary" />
                    ) : (
                      <Badge tone="primary" size="xs">
                        {t('payment.selected')}
                      </Badge>
                    ))}
                </div>
                {channel.fee_policy === 'customer_surcharge' && (
                  <div class="mt-2 text-xs font-bold text-warning-text">
                    {t('payment.feeLabel')}：{feeText(channel)}
                  </div>
                )}
                {disabled && <div class="mt-2 text-xs text-warning-text">{props.limitHint(channel)}</div>}
              </button>
            )
          })}
        </div>
      )
    }
  },
})
