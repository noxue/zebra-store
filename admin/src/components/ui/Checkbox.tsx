import { defineComponent } from 'vue'
import { Check, Minus } from 'lucide-vue-next'
import { cn } from './cn'

export const Checkbox = defineComponent({
  name: 'ZsCheckbox',
  props: {
    modelValue: Boolean,
    indeterminate: Boolean,
    disabled: Boolean,
    label: String,
  },
  emits: { 'update:modelValue': (_v: boolean) => true, change: (_v: boolean) => true },
  setup(props, { emit, slots }) {
    const toggle = (e: Event) => {
      e.preventDefault()
      if (props.disabled) return
      emit('update:modelValue', !props.modelValue)
      emit('change', !props.modelValue)
    }
    return () => {
      const on = props.modelValue || props.indeterminate
      return (
        <label
          class={cn('inline-flex select-none items-center gap-2', props.disabled ? 'opacity-50' : 'cursor-pointer')}
          onClick={toggle}
        >
          <span
            role="checkbox"
            tabindex={0}
            aria-checked={props.indeterminate ? 'mixed' : props.modelValue}
            onKeydown={(e: KeyboardEvent) => (e.key === ' ' || e.key === 'Enter') && toggle(e)}
            class={cn(
              'flex h-4 w-4 shrink-0 items-center justify-center rounded-[5px] border transition-all',
              on ? 'zs-gradient-bg border-transparent text-on-primary' : 'border-line-strong bg-surface-strong',
            )}
          >
            {props.indeterminate ? <Minus class="h-3 w-3" /> : props.modelValue ? <Check class="h-3 w-3" /> : null}
          </span>
          {(props.label || slots.default) && <span class="text-sm text-fg">{slots.default ? slots.default() : props.label}</span>}
        </label>
      )
    }
  },
})

export default Checkbox
