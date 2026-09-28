import { defineComponent } from 'vue'
import { Check } from 'lucide-vue-next'
import { cn } from './cn'

export const Switch = defineComponent({
  name: 'ZsSwitch',
  props: {
    modelValue: Boolean,
    disabled: Boolean,
    label: { type: String, default: '' },
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit }) {
    return () => (
      <button
        type="button"
        role="switch"
        aria-checked={props.modelValue}
        aria-label={props.label || undefined}
        disabled={props.disabled}
        class={cn(
          'relative inline-flex h-7 w-12 shrink-0 items-center rounded-full border border-line transition-colors disabled:opacity-50',
          props.modelValue ? 'zs-gradient-bg' : 'bg-surface-muted',
        )}
        onClick={() => emit('update:modelValue', !props.modelValue)}
      >
        <span
          class={cn(
            'inline-block size-5 rounded-full bg-surface-solid shadow-zs transition-transform duration-300 ease-[var(--zs-ease-bounce)]',
            props.modelValue ? 'translate-x-6' : 'translate-x-1',
          )}
        />
      </button>
    )
  },
})

export const Checkbox = defineComponent({
  name: 'ZsCheckbox',
  props: {
    modelValue: Boolean,
    disabled: Boolean,
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit, slots }) {
    return () => (
      <label class={cn('inline-flex cursor-pointer select-none items-start gap-2.5 text-sm', props.disabled && 'cursor-not-allowed opacity-60')}>
        <input
          type="checkbox"
          class="peer sr-only"
          checked={props.modelValue}
          disabled={props.disabled}
          onChange={(e: Event) => emit('update:modelValue', (e.target as HTMLInputElement).checked)}
        />
        <span
          class={cn(
            'mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md border transition peer-focus-visible:shadow-[var(--zs-ring)]',
            props.modelValue ? 'zs-gradient-bg border-transparent text-on-primary' : 'border-line-strong bg-surface-strong',
          )}
        >
          {props.modelValue && <Check class="size-3.5" stroke-width={3} />}
        </span>
        {slots.default && <span class="text-fg">{slots.default()}</span>}
      </label>
    )
  },
})
