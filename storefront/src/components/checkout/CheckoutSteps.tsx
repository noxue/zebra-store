import { computed, defineComponent, Fragment, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check } from 'lucide-vue-next'
import { cn } from '@/components/ui'

export type CheckoutStepKey = 'cart' | 'checkout' | 'payment'

/** cart → checkout → payment progress bar. */
export const CheckoutSteps = defineComponent({
  name: 'CheckoutSteps',
  props: {
    current: { type: String as PropType<CheckoutStepKey>, required: true },
    stepKeys: { type: Array as PropType<CheckoutStepKey[]>, default: () => ['cart', 'checkout', 'payment'] },
  },
  setup(props) {
    const { t } = useI18n()
    const steps = computed(() => {
      const active = props.stepKeys.indexOf(props.current)
      return props.stepKeys.map((key, idx) => ({
        key,
        label: t(`${key}.title`),
        status: active < 0 ? 'upcoming' : idx < active ? 'done' : idx === active ? 'current' : 'upcoming',
      }))
    })
    return () => (
      <ol class="zs-card flex list-none items-center px-4 py-3 sm:px-6" aria-label={t('checkoutSteps.label')}>
        {steps.value.map((step, idx) => (
          <Fragment key={step.key}>
            <li class={cn('flex items-center gap-2', idx > 0 && 'flex-1')} aria-current={step.status === 'current' ? 'step' : undefined}>
              {idx > 0 && (
                <div aria-hidden="true" class={cn('mx-2 h-1 flex-1 rounded-full transition-colors', step.status !== 'upcoming' ? 'zs-gradient-bg' : 'bg-surface-muted')} />
              )}
              <div class="flex shrink-0 items-center gap-2">
                <span
                  class={cn(
                    'flex size-8 items-center justify-center rounded-full text-xs font-bold zs-num transition-colors',
                    step.status !== 'upcoming' ? 'zs-gradient-bg text-on-primary shadow-zs' : 'border-2 border-line text-muted',
                    step.status === 'current' && 'ring-4 ring-primary-soft',
                  )}
                >
                  {step.status === 'done' ? <Check class="size-4" stroke-width={3} /> : idx + 1}
                </span>
                <span class={cn('hidden text-sm font-bold sm:inline', step.status !== 'upcoming' ? 'text-fg' : 'text-muted')}>{step.label}</span>
              </div>
            </li>
          </Fragment>
        ))}
      </ol>
    )
  },
})
