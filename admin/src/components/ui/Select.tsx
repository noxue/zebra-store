import { defineComponent, type PropType } from 'vue'
import { ChevronDown } from 'lucide-vue-next'
import { cn } from './cn'
import { inputBase } from './Input'

export type SelectValue = string | number
export interface SelectOption {
  label: string
  value: SelectValue
  disabled?: boolean
}

/** Styled native select — values keep their original type (number/string). */
export const Select = defineComponent({
  name: 'ZsSelect',
  props: {
    modelValue: { type: [String, Number] as PropType<SelectValue | null | undefined>, default: '' },
    options: { type: Array as PropType<SelectOption[]>, default: () => [] },
    placeholder: String,
    disabled: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: SelectValue) => true, change: (_v: SelectValue) => true },
  setup(props, { emit }) {
    const onChange = (e: Event) => {
      const idx = Number((e.target as HTMLSelectElement).value)
      const opt = props.options[idx]
      const value = opt ? opt.value : ''
      emit('update:modelValue', value)
      emit('change', value)
    }
    return () => {
      const selectedIndex = props.options.findIndex((o) => String(o.value) === String(props.modelValue ?? ''))
      return (
        <div class="relative w-full">
          <select
            value={selectedIndex >= 0 ? String(selectedIndex) : ''}
            disabled={props.disabled}
            class={cn(
              inputBase,
              'cursor-pointer appearance-none pr-9',
              props.size === 'sm' ? 'h-8 pl-2.5 text-xs' : 'h-9 pl-3 text-sm',
              selectedIndex < 0 && 'text-muted',
            )}
            onChange={onChange}
          >
            {selectedIndex < 0 && (
              <option value="" disabled hidden>
                {props.placeholder ?? ''}
              </option>
            )}
            {props.options.map((opt, i) => (
              <option key={`${i}-${opt.value}`} value={String(i)} disabled={opt.disabled}>
                {opt.label}
              </option>
            ))}
          </select>
          <ChevronDown class="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-primary" />
        </div>
      )
    }
  },
})

export default Select
