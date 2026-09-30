import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { CircleCheck, Coins, CreditCard, Wallet } from 'lucide-vue-next'
import { siAlipay, siBitcoin, siDogecoin, siEthereum, siLitecoin, siPaypal, siPolygon, siSolana, siStripe, siTether, siWechat, siXrp, type SimpleIcon } from 'simple-icons'
import type { PaymentChannel } from '@/api/types'
import { Badge, cn } from '@/components/ui'
import { paymentChoiceLabel, paymentChoices } from '@/utils/paymentMethods'
import { formatFeeRate } from '@/utils/orderPayment'

const methodIcon = (type: string): SimpleIcon | null => {
  const value = type.toLowerCase()
  if (value === 'wechat' || value === 'wxpay') return siWechat
  if (value === 'alipay') return siAlipay
  if (value.includes('usdt')) return siTether
  if (value.includes('usdc')) return siCircleDollar
  if (value.includes('ethereum') || value.includes('erc20')) return siEthereum
  if (value.includes('polygon')) return siPolygon
  if (value.includes('solana')) return siSolana
  if (value.includes('bitcoin') || value === 'btc') return siBitcoin
  if (value.includes('doge')) return siDogecoin
  if (value.includes('litecoin') || value === 'ltc') return siLitecoin
  if (value === 'paypal') return siPaypal
  if (value === 'stripe') return siStripe
  if (value === 'xrp') return siXrp
  return null
}

const siCircleDollar: SimpleIcon = {
  title: 'USD Coin',
  slug: 'usd-coin',
  hex: '2775CA',
  source: 'https://www.circle.com/usdc',
  svg: '',
  path: 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm1 15.9V19h-2v-1.1c-1.7-.3-2.8-1.3-3-2.8h2c.1.7.6 1.1 1.4 1.1.8 0 1.2-.3 1.2-.8 0-.5-.4-.7-1.7-1-1.8-.4-2.7-1.1-2.7-2.6 0-1.3 1-2.3 2.8-2.6V8h2v1.1c1.5.3 2.4 1.2 2.6 2.6h-2c-.1-.6-.5-.9-1.2-.9-.7 0-1.1.3-1.1.7 0 .5.4.7 1.7 1 1.9.4 2.8 1.1 2.8 2.6 0 1.5-1.1 2.5-2.8 2.8Z',
}

/** Payment-type picker. Repeated payment types show their channel name as a disambiguator. */
export const PaymentChannelSelector = defineComponent({
  name: 'PaymentChannelSelector',
  props: {
    channels: { type: Array as PropType<PaymentChannel[]>, required: true },
    modelValue: { type: Number as PropType<number | null>, default: null },
    channelType: { type: String, default: '' },
    isDisabled: { type: Function as PropType<(c: PaymentChannel) => boolean>, default: () => false },
    limitHint: { type: Function as PropType<(c: PaymentChannel) => string>, default: () => '' },
    /** Formats the fixed fee (currency aware); omitted → plain number. */
    fixedFee: { type: Function as PropType<(c: PaymentChannel) => string>, default: undefined },
    compact: Boolean,
    emptyText: { type: String, default: '' },
  },
  emits: { 'update:modelValue': (_id: number) => true, 'update:channelType': (_type: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const choices = () => paymentChoices(props.channels)
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
      const options = choices()
      if (options.length === 1) return null
      return (
        <div class={cn('grid gap-3', props.compact ? 'grid-cols-1' : 'grid-cols-1 sm:grid-cols-2')}>
          {options.map(({ channel, type }) => {
            const disabled = props.isDisabled(channel)
            const selected = props.modelValue === Number(channel.id) && props.channelType === type && !disabled
            const icon = methodIcon(type)
            return (
              <button
                key={`${channel.id}:${type}`}
                type="button"
                disabled={disabled}
                title={disabled ? props.limitHint(channel) : ''}
                class={cn(
                  'group relative rounded-zs border p-3 text-left transition-all disabled:cursor-not-allowed disabled:opacity-55',
                  selected ? 'border-primary bg-primary-soft shadow-glow' : 'border-line bg-surface-strong hover:-translate-y-0.5 hover:border-line-strong hover:shadow-zs',
                )}
                onClick={() => {
                  if (!disabled) {
                    emit('update:modelValue', Number(channel.id))
                    emit('update:channelType', type)
                  }
                }}
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="flex min-w-0 items-center gap-2">
                    <span class={cn('flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-zs-sm', selected ? 'zs-gradient-bg text-on-primary' : 'bg-surface-muted text-primary-text')}>
                      {type === 'balance' ? (
                        <Wallet class="size-4" />
                      ) : ['trx', 'epusdt', 'bepusdt', 'tokenpay', 'okpay'].includes(type) || (type.includes('usdt') && !icon) ? (
                        <Coins class="size-4" />
                      ) : icon ? (
                        <svg viewBox="0 0 24 24" role="img" aria-hidden="true" class="size-5" fill={`#${icon.hex}`}>
                          <path d={icon.path} />
                        </svg>
                      ) : (
                        <CreditCard class="size-4" />
                      )}
                    </span>
                    <span class="truncate text-sm font-bold text-fg">{paymentChoiceLabel({ channel, type }, options, (key) => t(key))}</span>
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
