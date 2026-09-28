import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'
import type { SelectOption, SelectValue } from './Select'

export interface RadioOption extends SelectOption {
  description?: string
}

export const RadioGroup = defineComponent({
  name: 'ZsRadioGroup',
  props: {
    modelValue: { type: [String, Number] as PropType<SelectValue | null | undefined>, default: '' },
    options: { type: Array as PropType<RadioOption[]>, default: () => [] },
    variant: { type: String as PropType<'pills' | 'cards' | 'dots'>, default: 'pills' },
    disabled: Boolean,
  },
  emits: { 'update:modelValue': (_v: SelectValue) => true, change: (_v: SelectValue) => true },
  setup(props, { emit }) {
    const pick = (opt: RadioOption) => {
      if (props.disabled || opt.disabled) return
      emit('update:modelValue', opt.value)
      emit('change', opt.value)
    }
    return () => (
      <div role="radiogroup" class={cn('flex flex-wrap gap-2', props.variant === 'cards' && 'grid grid-cols-1 gap-3 sm:grid-cols-2')}>
        {props.options.map((opt) => {
          const active = String(opt.value) === String(props.modelValue)
          if (props.variant === 'cards') {
            return (
              <button
                type="button"
                role="radio"
                aria-checked={active}
                key={String(opt.value)}
                onClick={() => pick(opt)}
                class={cn(
                  'rounded-zs border p-4 text-left transition-all',
                  active ? 'border-primary bg-primary-soft shadow-glow' : 'border-line bg-surface-strong hover:border-line-strong',
                  (props.disabled || opt.disabled) && 'opacity-50',
                )}
              >
                <div class="flex items-center gap-2 font-medium text-fg">
                  <span class={cn('h-3.5 w-3.5 rounded-full border-2', active ? 'border-primary bg-primary' : 'border-line-strong')} />
                  {opt.label}
                </div>
                {opt.description && <p class="mt-1.5 text-xs text-muted">{opt.description}</p>}
              </button>
            )
          }
          if (props.variant === 'dots') {
            return (
              <label key={String(opt.value)} class="inline-flex cursor-pointer items-center gap-2 text-sm" onClick={() => pick(opt)}>
                <span
                  role="radio"
                  aria-checked={active}
                  class={cn('flex h-4 w-4 items-center justify-center rounded-full border-2', active ? 'border-primary' : 'border-line-strong')}
                >
                  {active && <span class="h-2 w-2 rounded-full bg-primary" />}
                </span>
                {opt.label}
              </label>
            )
          }
          return (
            <button
              type="button"
              role="radio"
              aria-checked={active}
              key={String(opt.value)}
              onClick={() => pick(opt)}
              class={cn(
                'h-8 rounded-full border px-3.5 text-xs font-medium transition-all',
                active ? 'zs-gradient-bg border-transparent text-on-primary shadow-zs-sm' : 'border-line-strong bg-surface-strong text-fg hover:border-primary hover:text-primary',
                (props.disabled || opt.disabled) && 'opacity-50',
              )}
            >
              {opt.label}
            </button>
          )
        })}
      </div>
    )
  },
})

export default RadioGroup
