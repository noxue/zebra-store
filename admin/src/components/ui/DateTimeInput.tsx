import { defineComponent, type PropType } from 'vue'
import { cn } from './cn'
import { inputBase } from './Input'

/**
 * datetime-local / date input. `modelValue` is the raw local value ("2026-01-02T10:00");
 * convert with `toRFC3339` / `toDateTimeLocal` from utils/format.
 */
export const DateTimeInput = defineComponent({
  name: 'ZsDateTimeInput',
  props: {
    modelValue: {
      type: String as PropType<string | null | undefined>,
      default: '',
    },
    type: {
      type: String as PropType<'datetime-local' | 'date'>,
      default: 'datetime-local',
    },
    placeholder: String,
    disabled: Boolean,
    size: { type: String as PropType<'sm' | 'md'>, default: 'md' },
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    return () => (
      <input
        type={props.type}
        value={props.modelValue ?? ''}
        disabled={props.disabled}
        placeholder={props.placeholder}
        // native date inputs never show a placeholder: expose it as the accessible name / tooltip
        title={props.placeholder}
        aria-label={props.placeholder}
        class={cn(inputBase, 'min-w-0', props.size === 'sm' ? 'h-8 px-2.5 text-xs' : 'h-9 px-3 text-sm', !props.modelValue && 'text-muted')}
        onInput={(e: Event) => emit('update:modelValue', (e.target as HTMLInputElement).value)}
      />
    )
  },
})

/** Two DateTimeInputs side by side for created_from / created_to style filters. */
export const DateRangeInput = defineComponent({
  name: 'ZsDateRangeInput',
  props: {
    from: { type: String, default: '' },
    to: { type: String, default: '' },
    type: {
      type: String as PropType<'datetime-local' | 'date'>,
      default: 'datetime-local',
    },
  },
  emits: {
    'update:from': (_v: string) => true,
    'update:to': (_v: string) => true,
  },
  setup(props, { emit }) {
    return () => (
      <div class="flex items-center gap-1.5">
        <DateTimeInput type={props.type} modelValue={props.from} onUpdate:modelValue={(v: string) => emit('update:from', v)} />
        <span class="text-xs text-muted">~</span>
        <DateTimeInput type={props.type} modelValue={props.to} onUpdate:modelValue={(v: string) => emit('update:to', v)} />
      </div>
    )
  },
})

export default DateTimeInput
