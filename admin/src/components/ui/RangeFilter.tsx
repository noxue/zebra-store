import { defineComponent } from 'vue'
import { DateTimeInput } from './DateTimeInput'

/** Labelled from/to datetime range used in list filter bars (spans two filter columns). */
export const RangeFilter = defineComponent({
  name: 'ZsRangeFilter',
  props: {
    label: { type: String, required: true },
    from: { type: String, default: '' },
    to: { type: String, default: '' },
    fromPlaceholder: String,
    toPlaceholder: String,
  },
  emits: { 'update:from': (_v: string) => true, 'update:to': (_v: string) => true, change: () => true },
  setup(props, { emit }) {
    return () => (
      <div class="flex flex-col gap-1.5 sm:col-span-2">
        <span class="text-xs text-muted">{props.label}</span>
        <div class="flex items-center gap-2">
          <div class="min-w-0 flex-1">
            <DateTimeInput
              modelValue={props.from}
              placeholder={props.fromPlaceholder}
              onUpdate:modelValue={(v: string) => {
                emit('update:from', v)
                emit('change')
              }}
            />
          </div>
          <span class="text-muted">-</span>
          <div class="min-w-0 flex-1">
            <DateTimeInput
              modelValue={props.to}
              placeholder={props.toPlaceholder}
              onUpdate:modelValue={(v: string) => {
                emit('update:to', v)
                emit('change')
              }}
            />
          </div>
        </div>
      </div>
    )
  },
})

export default RangeFilter
