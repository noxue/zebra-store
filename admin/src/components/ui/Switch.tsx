import { defineComponent } from 'vue'
import { cn } from './cn'

export const Switch = defineComponent({
  name: 'ZsSwitch',
  props: {
    modelValue: Boolean,
    disabled: Boolean,
    label: String,
    size: { type: String as () => 'sm' | 'md', default: 'md' },
  },
  emits: { 'update:modelValue': (_v: boolean) => true, change: (_v: boolean) => true },
  setup(props, { emit, slots }) {
    const toggle = () => {
      if (props.disabled) return
      emit('update:modelValue', !props.modelValue)
      emit('change', !props.modelValue)
    }
    return () => (
      <label class={cn('inline-flex select-none items-center gap-2', props.disabled ? 'opacity-50' : 'cursor-pointer')}>
        <button
          type="button"
          role="switch"
          aria-checked={props.modelValue}
          disabled={props.disabled}
          onClick={toggle}
          class={cn(
            'relative inline-flex shrink-0 items-center rounded-full border border-transparent transition-colors duration-200',
            props.size === 'sm' ? 'h-5 w-9' : 'h-6 w-11',
            props.modelValue ? 'zs-gradient-bg' : 'bg-line-strong',
          )}
        >
          <span
            class={cn(
              'inline-block rounded-full bg-surface-solid shadow-zs-sm transition-transform duration-300 [transition-timing-function:var(--zs-ease-bounce)]',
              props.size === 'sm' ? 'h-4 w-4' : 'h-5 w-5',
              props.modelValue ? (props.size === 'sm' ? 'translate-x-4' : 'translate-x-5') : 'translate-x-0.5',
            )}
          />
        </button>
        {(props.label || slots.default) && <span class="text-sm text-fg">{slots.default ? slots.default() : props.label}</span>}
      </label>
    )
  },
})

export default Switch
