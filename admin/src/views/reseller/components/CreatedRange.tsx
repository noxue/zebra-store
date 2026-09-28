import { defineComponent } from 'vue'
import { DateRangeInput } from '@/components/ui'

/** "Created range" filter: label + from/to datetime inputs, spanning two filter columns. */
export const CreatedRange = defineComponent({
  name: 'ResellerCreatedRange',
  props: {
    label: { type: String, required: true },
    from: { type: String, default: '' },
    to: { type: String, default: '' },
  },
  emits: {
    'update:from': (_v: string) => true,
    'update:to': (_v: string) => true,
    change: () => true,
  },
  setup(props, { emit }) {
    return () => (
      <div class="flex min-w-0 flex-col gap-1.5 sm:col-span-2 sm:flex-row sm:items-center">
        <span class="whitespace-nowrap text-xs text-muted">{props.label}</span>
        <div class="min-w-0 flex-1 [&_input]:min-w-0">
          <DateRangeInput
            from={props.from}
            to={props.to}
            onUpdate:from={(v: string) => {
              emit('update:from', v)
              emit('change')
            }}
            onUpdate:to={(v: string) => {
              emit('update:to', v)
              emit('change')
            }}
          />
        </div>
      </div>
    )
  },
})

export default CreatedRange
