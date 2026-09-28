import { defineComponent, type PropType } from 'vue'
import { Minus, Plus } from 'lucide-vue-next'
import { cn } from './cn'

/** −/value/+ stepper. `max` null = unlimited. Emits clamped integers. */
export const QuantityStepper = defineComponent({
  name: 'ZsQuantityStepper',
  props: {
    modelValue: { type: Number, required: true },
    min: { type: Number, default: 1 },
    max: { type: Number as PropType<number | null>, default: null },
    disabled: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: number) => true, 'limit': (_kind: 'min' | 'max') => true },
  setup(props, { emit }) {
    const clamp = (v: number) => {
      let n = Math.floor(Number.isFinite(v) ? v : props.min)
      if (n < props.min) {
        emit('limit', 'min')
        n = props.min
      }
      if (props.max !== null && props.max > 0 && n > props.max) {
        emit('limit', 'max')
        n = props.max
      }
      return n
    }
    return () => {
      const h = props.size === 'sm' ? 'h-8' : 'h-10'
      const w = props.size === 'sm' ? 'w-8' : 'w-10'
      const atMax = props.max !== null && props.max > 0 && props.modelValue >= props.max
      return (
        <div class={cn('inline-flex items-center overflow-hidden rounded-full border border-line bg-surface-strong', props.disabled && 'opacity-60')}>
          <button type="button" aria-label="-" class={cn(h, w, 'flex items-center justify-center text-muted transition hover:bg-primary-soft hover:text-primary-text disabled:opacity-40')} disabled={props.disabled || props.modelValue <= props.min} onClick={() => emit('update:modelValue', clamp(props.modelValue - 1))}>
            <Minus class="size-4" />
          </button>
          <input
            type="number"
            inputmode="numeric"
            class={cn(h, 'w-14 border-x border-line bg-transparent text-center font-bold text-fg outline-none zs-num [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none')}
            value={props.modelValue}
            disabled={props.disabled}
            onChange={(e: Event) => {
              const input = e.target as HTMLInputElement
              const next = clamp(Number(input.value))
              input.value = String(next)
              emit('update:modelValue', next)
            }}
          />
          <button type="button" aria-label="+" class={cn(h, w, 'flex items-center justify-center text-muted transition hover:bg-primary-soft hover:text-primary-text disabled:opacity-40')} disabled={props.disabled || atMax} onClick={() => emit('update:modelValue', clamp(props.modelValue + 1))}>
            <Plus class="size-4" />
          </button>
        </div>
      )
    }
  },
})
