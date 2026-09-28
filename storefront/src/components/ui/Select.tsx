import { defineComponent, type PropType } from 'vue'
import { ChevronDown } from 'lucide-vue-next'
import { cn } from './cn'

export interface SelectOption {
  label: string
  value: string | number
  disabled?: boolean
}

/** Styled native select (accessible, mobile friendly). Use with `v-model`. */
export const Select = defineComponent({
  name: 'ZsSelect',
  props: {
    modelValue: { type: [String, Number] as PropType<string | number | null>, default: '' },
    options: { type: Array as PropType<SelectOption[]>, default: () => [] },
    placeholder: { type: String, default: '' },
    disabled: Boolean,
    invalid: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: string | number) => true },
  setup(props, { emit }) {
    return () => (
      <div class="relative">
        <select
          value={props.modelValue ?? ''}
          disabled={props.disabled}
          class={cn(
            'w-full appearance-none rounded-zs border bg-surface-strong pl-4 pr-10 text-fg outline-none transition',
            'focus:border-primary focus:shadow-[var(--zs-ring)] disabled:opacity-60',
            props.size === 'sm' ? 'h-9 text-sm' : 'h-11 text-sm',
            props.invalid ? 'border-danger' : 'border-line',
          )}
          onChange={(e: Event) => {
            const raw = (e.target as HTMLSelectElement).value
            const match = props.options.find((o) => String(o.value) === raw)
            emit('update:modelValue', match ? match.value : raw)
          }}
        >
          {props.placeholder && (
            <option value="" disabled={false}>
              {props.placeholder}
            </option>
          )}
          {props.options.map((o) => (
            <option key={String(o.value)} value={o.value} disabled={o.disabled} selected={String(o.value) === String(props.modelValue ?? '')}>
              {o.label}
            </option>
          ))}
        </select>
        <ChevronDown class="pointer-events-none absolute right-3.5 top-1/2 size-4 -translate-y-1/2 text-muted" />
      </div>
    )
  },
})
